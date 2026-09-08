use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::config::OpenAiConfig;
use super::constants::*;
use super::fault::ProviderFault;
use super::stream::{ModelAccumulator, OpenAiChunk, delta_payload_bytes};
use super::traits::ModelProvider;
use super::types::{ModelDelta, ModelMessage, ModelRequest, ModelRole, ModelTurn};

pub struct OpenAiChatProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
}

impl OpenAiChatProvider {
    pub fn new(config: OpenAiConfig) -> Result<Self, ProviderFault> {
        if config.api_key.trim().is_empty() || config.model.trim().is_empty() {
            return Err(ProviderFault::Configuration {
                message: "provider API key and model are required".to_owned(),
            });
        }
        let mut headers = HeaderMap::new();
        for (name, value) in &config.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
                ProviderFault::Configuration {
                    message: format!("invalid provider header name: {error}"),
                }
            })?;
            let value =
                HeaderValue::from_str(value).map_err(|error| ProviderFault::Configuration {
                    message: format!("invalid provider header value: {error}"),
                })?;
            headers.insert(name, value);
        }
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .default_headers(headers)
            .user_agent(concat!("pentesting/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| ProviderFault::Transport {
                message: error.to_string(),
            })?;
        Ok(Self { config, client })
    }

    async fn complete_once(
        &self,
        request: &ModelRequest,
        deltas: Option<mpsc::UnboundedSender<ModelDelta>>,
    ) -> Result<ModelTurn, ProviderAttemptFault> {
        let body = OpenAiRequest::from_model_request(
            &self.config.model,
            request,
            self.config.max_output_tokens,
        )
        .map_err(ProviderAttemptFault::terminal)?;
        let response = self
            .client
            .post(
                self.config
                    .endpoint()
                    .map_err(ProviderAttemptFault::terminal)?,
            )
            .bearer_auth(&self.config.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|error| {
                ProviderAttemptFault::retryable(ProviderFault::Transport {
                    message: error.to_string(),
                })
            })?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(parse_retry_after);
            let message = bounded_error_body(response).await;
            let fault = ProviderFault::from_http(status, message);
            return Err(if is_retryable_fault(&fault) {
                ProviderAttemptFault {
                    fault,
                    retry_after,
                    output_started: false,
                }
            } else {
                ProviderAttemptFault::terminal(fault)
            });
        }

        let effective_max_tokens = self
            .config
            .max_output_tokens
            .unwrap_or(request.max_output_tokens);
        let max_output_bytes = usize::try_from(effective_max_tokens)
            .unwrap_or(usize::MAX)
            .saturating_mul(16)
            .clamp(1_024, MAX_PROVIDER_OUTPUT_BYTES);
        let max_stream_bytes = max_output_bytes
            .saturating_mul(STREAM_OVERHEAD_MULTIPLIER)
            .clamp(MIN_PROVIDER_STREAM_BYTES, MAX_PROVIDER_STREAM_BYTES);
        if response
            .content_length()
            .is_some_and(|length| length > max_stream_bytes as u64)
        {
            return Err(ProviderAttemptFault::terminal(
                ProviderFault::InvalidResponse {
                    message: format!(
                        "provider stream exceeded the {max_stream_bytes}-byte stream envelope"
                    ),
                },
            ));
        }
        let stream_limit_hit = Arc::new(AtomicBool::new(false));
        let stream_limit_signal = stream_limit_hit.clone();
        let mut stream_bytes = 0_usize;
        let bounded_stream = response.bytes_stream().map(move |result| {
            let chunk = result.map_err(io::Error::other)?;
            stream_bytes = stream_bytes.saturating_add(chunk.len());
            if stream_bytes > max_stream_bytes {
                stream_limit_signal.store(true, Ordering::Release);
                return Err(io::Error::other(format!(
                    "provider stream exceeded the {max_stream_bytes}-byte stream envelope"
                )));
            }
            Ok(chunk)
        });
        let mut accumulator = ModelAccumulator::default();
        let mut events = bounded_stream.eventsource();
        let mut output_started = false;
        let mut stream_completed = false;
        let mut output_bytes = 0_usize;
        while let Some(event) = events.next().await {
            let event = event.map_err(|error| {
                if stream_limit_hit.load(Ordering::Acquire) {
                    ProviderAttemptFault::terminal(ProviderFault::InvalidResponse {
                        message: format!(
                            "provider stream exceeded the {max_stream_bytes}-byte stream envelope"
                        ),
                    })
                } else {
                    ProviderAttemptFault {
                        fault: ProviderFault::Stream {
                            message: error.to_string(),
                        },
                        retry_after: None,
                        output_started,
                    }
                }
            })?;
            if event.data.trim() == STREAM_DONE_SENTINEL {
                stream_completed = true;
                break;
            }
            let chunk: OpenAiChunk = serde_json::from_str(&event.data).map_err(|error| {
                ProviderAttemptFault::terminal(ProviderFault::InvalidResponse {
                    message: format!("invalid stream event: {error}"),
                })
            })?;
            for delta in chunk.into_deltas() {
                output_bytes = output_bytes.saturating_add(delta_payload_bytes(&delta));
                if output_bytes > max_output_bytes {
                    return Err(ProviderAttemptFault::terminal(
                        ProviderFault::InvalidResponse {
                            message: format!(
                                "provider output exceeded the {max_output_bytes}-byte response envelope"
                            ),
                        },
                    ));
                }
                output_started = true;
                if let Some(sender) = &deltas {
                    let _ = sender.send(delta.clone());
                }
                accumulator.push(delta);
            }
        }
        if !stream_completed && accumulator.finish_reason.is_none() {
            eprintln!(
                "[provider] stream ended before completion marker: output_bytes={}, finish_reason={:?}",
                output_bytes, accumulator.finish_reason
            );
            return Err(ProviderAttemptFault {
                fault: ProviderFault::Stream {
                    message: "provider stream ended before a completion marker".to_owned(),
                },
                retry_after: None,
                output_started,
            });
        }
        accumulator.finish().map_err(|fault| {
            eprintln!("[provider] accumulator finish failed: {fault}");
            ProviderAttemptFault::terminal(fault)
        })
    }
}

async fn bounded_error_body(response: reqwest::Response) -> String {
    let mut stream = response.bytes_stream();
    let mut body = Vec::with_capacity(MAX_PROVIDER_ERROR_BYTES);
    let mut truncated = false;
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            return "provider returned an unreadable error".to_owned();
        };
        let remaining = MAX_PROVIDER_ERROR_BYTES.saturating_sub(body.len());
        if remaining == 0 {
            truncated = true;
            break;
        }
        let copied = remaining.min(chunk.len());
        body.extend_from_slice(&chunk[..copied]);
        if copied < chunk.len() {
            truncated = true;
            break;
        }
    }
    let mut message = String::from_utf8_lossy(&body).into_owned();
    if truncated {
        message.push_str("\n[truncated]");
    }
    message
}

#[async_trait]
impl ModelProvider for OpenAiChatProvider {
    fn context_limit(&self) -> u64 {
        self.config.context_tokens
    }

    async fn complete(
        &self,
        request: ModelRequest,
        deltas: Option<mpsc::UnboundedSender<ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        let operation = async {
            for attempt in 1..=MAX_PROVIDER_ATTEMPTS {
                match self.complete_once(&request, deltas.clone()).await {
                    Ok(turn) => return Ok(turn),
                    Err(failure) if failure.can_retry() && attempt < MAX_PROVIDER_ATTEMPTS => {
                        tokio::time::sleep(retry_delay(attempt, failure.retry_after)).await;
                    }
                    Err(failure) => return Err(failure.fault),
                }
            }
            unreachable!("provider attempt loop always returns")
        };
        tokio::time::timeout(self.config.timeout, operation)
            .await
            .map_err(|_| ProviderFault::Transport {
                message: format!(
                    "logical provider call timed out after {:?}",
                    self.config.timeout
                ),
            })?
    }
}

pub struct ProviderAttemptFault {
    pub fault: ProviderFault,
    pub retry_after: Option<Duration>,
    pub output_started: bool,
}

impl ProviderAttemptFault {
    pub fn retryable(fault: ProviderFault) -> Self {
        Self {
            fault,
            retry_after: None,
            output_started: false,
        }
    }

    pub fn terminal(fault: ProviderFault) -> Self {
        Self {
            fault,
            retry_after: None,
            output_started: true,
        }
    }

    pub fn can_retry(&self) -> bool {
        !self.output_started && is_retryable_fault(&self.fault)
    }
}

pub fn is_retryable_fault(fault: &ProviderFault) -> bool {
    matches!(
        fault,
        ProviderFault::RateLimited { .. }
            | ProviderFault::Upstream { .. }
            | ProviderFault::Transport { .. }
            | ProviderFault::Stream { .. }
            | ProviderFault::Http {
                status: 408 | 425,
                ..
            }
    )
}

pub fn retry_delay(attempt: usize, retry_after: Option<Duration>) -> Duration {
    let exponent = attempt.saturating_sub(1).min(4) as u32;
    let backoff = INITIAL_RETRY_DELAY.saturating_mul(1_u32 << exponent);
    backoff
        .max(retry_after.unwrap_or_default())
        .min(MAX_RETRY_DELAY)
}

pub fn parse_retry_after(value: &HeaderValue) -> Option<Duration> {
    let value = value.to_str().ok()?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds).min(MAX_RETRY_DELAY));
    }
    let retry_at = DateTime::parse_from_rfc2822(value)
        .ok()?
        .with_timezone(&Utc);
    retry_at
        .signed_duration_since(Utc::now())
        .to_std()
        .ok()
        .map(|delay| delay.min(MAX_RETRY_DELAY))
}

#[derive(Serialize)]
pub struct OpenAiRequest<'a> {
    pub model: &'a str,
    pub messages: Vec<OpenAiMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<OpenAiTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<&'static str>,
    pub stream: bool,
    pub include_reasoning: bool,
    pub max_completion_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

impl<'a> OpenAiRequest<'a> {
    pub fn from_model_request(
        model: &'a str,
        request: &'a ModelRequest,
        max_output_tokens: Option<u64>,
    ) -> Result<Self, ProviderFault> {
        let messages = request
            .messages
            .iter()
            .map(OpenAiMessage::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let tools = if request.tools_enabled {
            request
                .tools
                .iter()
                .map(|definition| OpenAiTool {
                    kind: FUNCTION_TYPE,
                    function: OpenAiFunctionDefinition {
                        name: &definition.name,
                        description: &definition.description,
                        parameters: &definition.parameters,
                    },
                })
                .collect()
        } else {
            Vec::new()
        };
        let tool_choice = (!tools.is_empty()).then_some(TOOL_CHOICE_AUTO);
        Ok(Self {
            model,
            messages,
            tools,
            tool_choice,
            stream: true,
            include_reasoning: true,
            max_completion_tokens: max_output_tokens.unwrap_or(request.max_output_tokens),
            temperature: request.temperature,
        })
    }
}

#[derive(Serialize)]
pub struct OpenAiMessage<'a> {
    pub role: ModelRole,
    pub content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<OpenAiRequestToolCall>,
}

impl<'a> TryFrom<&'a ModelMessage> for OpenAiMessage<'a> {
    type Error = ProviderFault;

    fn try_from(message: &'a ModelMessage) -> Result<Self, Self::Error> {
        let tool_calls = message
            .tool_calls
            .iter()
            .map(|call| {
                Ok(OpenAiRequestToolCall {
                    id: call.id.clone(),
                    kind: FUNCTION_TYPE,
                    function: OpenAiRequestFunction {
                        name: call.name.clone(),
                        arguments: serde_json::to_string(&call.arguments).map_err(|error| {
                            ProviderFault::InvalidResponse {
                                message: error.to_string(),
                            }
                        })?,
                    },
                })
            })
            .collect::<Result<Vec<_>, ProviderFault>>()?;
        Ok(Self {
            role: message.role,
            content: &message.content,
            tool_call_id: message.tool_call_id.as_deref(),
            tool_calls,
        })
    }
}

#[derive(Serialize)]
pub struct OpenAiRequestToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: OpenAiRequestFunction,
}

#[derive(Serialize)]
pub struct OpenAiRequestFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Serialize)]
pub struct OpenAiTool<'a> {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub function: OpenAiFunctionDefinition<'a>,
}

#[derive(Serialize)]
pub struct OpenAiFunctionDefinition<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub parameters: &'a Value,
}
