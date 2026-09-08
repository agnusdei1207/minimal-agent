use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

use super::constants::LEAKED_CONTROL_TOKENS;
use super::fault::ProviderFault;
use super::types::{ModelDelta, ModelTurn, TokenUsage, ToolCall, ToolCallDelta};

pub fn assemble_model_deltas(
    deltas: impl IntoIterator<Item = ModelDelta>,
) -> Result<ModelTurn, ProviderFault> {
    let mut accumulator = ModelAccumulator::default();
    for delta in deltas {
        accumulator.push(delta);
    }
    accumulator.finish()
}

#[derive(Default)]
pub struct PartialToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Default)]
pub struct ModelAccumulator {
    pub reasoning: String,
    pub text: String,
    pub tool_calls: BTreeMap<usize, PartialToolCall>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

impl ModelAccumulator {
    pub fn push(&mut self, delta: ModelDelta) {
        match delta {
            ModelDelta::Reasoning(reasoning) => self.reasoning.push_str(&reasoning),
            ModelDelta::Text(text) => self.text.push_str(&text),
            ModelDelta::ToolCall(delta) => {
                let call = self.tool_calls.entry(delta.index).or_default();
                if let Some(id) = delta.id {
                    call.id.push_str(&id);
                }
                if let Some(name) = delta.name {
                    call.name.push_str(&name);
                }
                call.arguments.push_str(&delta.arguments_fragment);
            }
            ModelDelta::Usage(usage) => self.usage = Some(usage),
            ModelDelta::Finished(reason) => self.finish_reason = reason,
        }
    }

    pub fn finish(mut self) -> Result<ModelTurn, ProviderFault> {
        if matches!(
            self.finish_reason.as_deref(),
            Some("length" | "content_filter")
        ) {
            return Err(ProviderFault::InvalidResponse {
                message: format!(
                    "provider stopped before a complete answer: {}",
                    self.finish_reason.as_deref().unwrap_or_default()
                ),
            });
        }
        let mut tool_calls = Vec::with_capacity(self.tool_calls.len());
        let mut tool_call_ids = HashSet::new();
        for (index, call) in self.tool_calls {
            if call.id.is_empty() || call.name.is_empty() {
                eprintln!(
                    "[provider] malformed tool call {index}: missing id or name (id='{}', name='{}')",
                    call.id, call.name
                );
                return Err(ProviderFault::MalformedToolCall {
                    index,
                    message: "tool call is missing an id or name".to_owned(),
                });
            }
            if !tool_call_ids.insert(call.id.clone()) {
                eprintln!(
                    "[provider] malformed tool call {index}: duplicate id '{}'",
                    call.id
                );
                return Err(ProviderFault::MalformedToolCall {
                    index,
                    message: format!("tool call id {} is duplicated", call.id),
                });
            }
            let trimmed_args = call.arguments.trim();
            let arguments = if trimmed_args.is_empty() {
                serde_json::Value::Object(Default::default())
            } else {
                match serde_json::from_str(&call.arguments) {
                    Ok(parsed) => parsed,
                    Err(error) => {
                        let arg_preview = if call.arguments.len() <= 200 {
                            call.arguments.clone()
                        } else {
                            format!(
                                "{}... [len={}]",
                                &call.arguments[..200],
                                call.arguments.len()
                            )
                        };
                        eprintln!(
                            "[provider] malformed tool call: index={}, id='{}', name='{}', finish_reason={:?}, args_len={}, raw_args='{}', error={}",
                            index,
                            call.id,
                            call.name,
                            self.finish_reason,
                            call.arguments.len(),
                            arg_preview,
                            error
                        );
                        return Err(ProviderFault::MalformedToolCall {
                            index,
                            message: format!("{error} (raw args: '{arg_preview}')"),
                        });
                    }
                }
            };
            tool_calls.push(ToolCall {
                id: call.id,
                name: call.name,
                arguments,
            });
        }
        if self.text.trim().is_empty() && tool_calls.is_empty() {
            if !self.reasoning.trim().is_empty() {
                // When a reasoning model finishes thinking but puts its thoughts in reasoning_content
                // without emitting content or tool_calls, preserve the thoughts as text rather than crashing.
                self.text = self.reasoning;
            } else {
                eprintln!(
                    "[provider] empty completion: finish_reason={:?}, text_len={}, reasoning_len={}, tool_calls_count={}, usage={:?}",
                    self.finish_reason,
                    self.text.len(),
                    self.reasoning.len(),
                    tool_calls.len(),
                    self.usage
                );
                return Err(ProviderFault::EmptyCompletion);
            }
        }
        Ok(ModelTurn {
            text: self.text,
            tool_calls,
            usage: self.usage,
            finish_reason: self.finish_reason,
        })
    }
}

pub fn delta_payload_bytes(delta: &ModelDelta) -> usize {
    match delta {
        ModelDelta::Reasoning(text) | ModelDelta::Text(text) => text.len(),
        ModelDelta::ToolCall(call) => {
            call.id.as_ref().map_or(0, String::len)
                + call.name.as_ref().map_or(0, String::len)
                + call.arguments_fragment.len()
        }
        ModelDelta::Usage(_) | ModelDelta::Finished(_) => 0,
    }
}

/// Whether text carries a leaked provider control token (DeepSeek `<｜…｜>` or
/// ChatML `<|…|>`). Shared by the stream sanitizer and the runtime's partial-
/// output guard so the detection lives in one place.
pub fn has_control_token(text: &str) -> bool {
    text.contains("<｜") || text.contains("<|")
}

pub fn sanitize_model_content(text: &str) -> String {
    if !has_control_token(text) {
        return text.to_owned();
    }
    let mut cleaned = text.to_owned();
    for token in LEAKED_CONTROL_TOKENS {
        if cleaned.contains(token) {
            eprintln!("[provider] stripping leaked control token from content: {token}");
            cleaned = cleaned.replace(token, "");
        }
    }
    cleaned
}

#[derive(Deserialize)]
pub struct OpenAiChunk {
    #[serde(default)]
    pub choices: Vec<OpenAiChoice>,
    pub usage: Option<OpenAiUsage>,
}

impl OpenAiChunk {
    pub fn into_deltas(self) -> Vec<ModelDelta> {
        let mut output = Vec::new();
        for choice in self.choices {
            if let Some(reasoning) = choice.delta.reasoning.filter(|text| !text.is_empty()) {
                output.push(ModelDelta::Reasoning(reasoning));
            } else {
                output.extend(
                    choice
                        .delta
                        .reasoning_details
                        .into_iter()
                        .filter_map(OpenAiReasoningDetail::visible_text)
                        .map(ModelDelta::Reasoning),
                );
            }
            if let Some(content) = choice.delta.content
                && !content.is_empty()
            {
                let sanitized = sanitize_model_content(&content);
                if !sanitized.is_empty() {
                    output.push(ModelDelta::Text(sanitized));
                }
            }
            for call in choice.delta.tool_calls {
                output.push(ModelDelta::ToolCall(ToolCallDelta {
                    index: call.index,
                    id: call.id,
                    name: call
                        .function
                        .as_ref()
                        .and_then(|function| function.name.clone()),
                    arguments_fragment: call
                        .function
                        .and_then(|function| function.arguments)
                        .unwrap_or_default(),
                }));
            }
            if choice.finish_reason.is_some() {
                output.push(ModelDelta::Finished(choice.finish_reason));
            }
        }
        if let Some(usage) = self.usage {
            output.push(ModelDelta::Usage(TokenUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
            }));
        }
        output
    }
}

#[derive(Deserialize)]
pub struct OpenAiChoice {
    #[serde(default)]
    pub delta: OpenAiResponseDelta,
    pub finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct OpenAiResponseDelta {
    #[serde(alias = "reasoning_content")]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub reasoning_details: Vec<OpenAiReasoningDetail>,
    pub content: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<OpenAiResponseToolCall>,
}

#[derive(Deserialize)]
pub struct OpenAiResponseToolCall {
    pub index: usize,
    pub id: Option<String>,
    pub function: Option<OpenAiResponseFunction>,
}

#[derive(Deserialize)]
pub struct OpenAiResponseFunction {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

#[derive(Deserialize)]
pub struct OpenAiUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
}

#[derive(Deserialize)]
pub struct OpenAiReasoningDetail {
    pub text: Option<String>,
    pub summary: Option<String>,
}

impl OpenAiReasoningDetail {
    pub fn visible_text(self) -> Option<String> {
        self.text
            .or(self.summary)
            .filter(|content| !content.is_empty())
    }
}
