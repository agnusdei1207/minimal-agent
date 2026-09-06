use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{RwLock, mpsc};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelMessage {
    pub role: ModelRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ModelMessage {
    pub fn new(role: ModelRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallDelta {
    pub index: usize,
    pub id: Option<String>,
    pub name: Option<String>,
    pub arguments_fragment: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelDelta {
    Reasoning(String),
    Text(String),
    ToolCall(ToolCallDelta),
    Usage(TokenUsage),
    Finished(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelTurn {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ModelRequest {
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ToolDefinition>,
    pub tools_enabled: bool,
    pub max_output_tokens: u64,
    pub temperature: Option<f32>,
}

impl ModelRequest {
    pub fn tools_disabled(messages: Vec<ModelMessage>, max_output_tokens: u64) -> Self {
        Self {
            messages,
            tools: Vec::new(),
            tools_enabled: false,
            max_output_tokens,
            temperature: None,
        }
    }
}

#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn model_id(&self) -> &str;
    fn context_limit(&self) -> u64;

    async fn complete(
        &self,
        request: ModelRequest,
        deltas: Option<mpsc::UnboundedSender<ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault>;
}

pub struct ProviderSlot {
    current: RwLock<Option<Arc<dyn ModelProvider>>>,
    active_model: RwLock<Option<String>>,
    configured: AtomicBool,
    context_limit: u64,
}

impl ProviderSlot {
    pub fn unconfigured(context_limit: u64) -> Self {
        Self {
            current: RwLock::new(None),
            active_model: RwLock::new(None),
            configured: AtomicBool::new(false),
            context_limit,
        }
    }

    pub fn is_configured(&self) -> bool {
        self.configured.load(Ordering::Acquire)
    }

    pub fn runtime_context_limit(&self) -> u64 {
        self.context_limit
    }

    pub async fn active_model(&self) -> Option<String> {
        self.active_model.read().await.clone()
    }

    pub async fn replace(&self, provider: Arc<dyn ModelProvider>, model: String) {
        *self.current.write().await = Some(provider);
        *self.active_model.write().await = Some(model);
        self.configured.store(true, Ordering::Release);
    }
}

#[async_trait]
impl ModelProvider for ProviderSlot {
    fn model_id(&self) -> &str {
        "runtime-selected"
    }

    fn context_limit(&self) -> u64 {
        self.context_limit
    }

    async fn complete(
        &self,
        request: ModelRequest,
        deltas: Option<mpsc::UnboundedSender<ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        let provider =
            self.current
                .read()
                .await
                .clone()
                .ok_or_else(|| ProviderFault::Configuration {
                    message: "no model configured; run /model to choose one".to_owned(),
                })?;
        provider.complete(request, deltas).await
    }
}

#[derive(Clone)]
pub struct OpenAiConfig {
    pub base_url: Url,
    pub api_key: String,
    pub model: String,
    pub context_tokens: u64,
    pub timeout: Duration,
    pub headers: HashMap<String, String>,
}

impl OpenAiConfig {
    pub fn from_env() -> Result<Self, ProviderFault> {
        let api_key =
            std::env::var("OPENAI_API_KEY").map_err(|_| ProviderFault::Configuration {
                message: "set OPENAI_API_KEY or run /model".to_owned(),
            })?;
        let model = std::env::var("OPENAI_MODEL").map_err(|_| ProviderFault::Configuration {
            message: "set OPENAI_MODEL or run /model".to_owned(),
        })?;
        let base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned())
            .parse::<Url>()
            .map_err(|error| ProviderFault::Configuration {
                message: format!("invalid provider base URL: {error}"),
            })?;
        let context_tokens =
            parse_context_tokens(std::env::var("OPENAI_CONTEXT_TOKENS").ok().as_deref())?;
        let timeout = parse_provider_timeout();
        Ok(Self {
            base_url,
            api_key,
            model,
            context_tokens,
            timeout,
            headers: HashMap::new(),
        })
    }

    fn endpoint(&self) -> Result<Url, ProviderFault> {
        let base = self.base_url.as_str().trim_end_matches('/');
        let endpoint = if base.ends_with("/chat/completions") {
            base.to_owned()
        } else {
            format!("{base}/chat/completions")
        };
        endpoint
            .parse()
            .map_err(|error| ProviderFault::Configuration {
                message: format!("invalid chat endpoint: {error}"),
            })
    }
}

fn parse_context_tokens(value: Option<&str>) -> Result<u64, ProviderFault> {
    let Some(value) = value else {
        return Ok(128_000);
    };
    value
        .parse::<u64>()
        .ok()
        .filter(|tokens| *tokens > 0)
        .ok_or_else(|| ProviderFault::Configuration {
            message: "OPENAI_CONTEXT_TOKENS must be a positive integer".to_owned(),
        })
}

fn parse_provider_timeout() -> Duration {
    std::env::var("MINIMAL_AGENT_PROVIDER_TIMEOUT")
        .or_else(|_| std::env::var("OPENAI_TIMEOUT"))
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(300))
}

pub struct OpenAiChatProvider {
    config: OpenAiConfig,
    client: reqwest::Client,
}

const MAX_PROVIDER_ATTEMPTS: usize = 3;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_PROVIDER_ERROR_BYTES: usize = 16 * 1024;
const MAX_PROVIDER_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const MIN_PROVIDER_STREAM_BYTES: usize = 64 * 1024;
const MAX_PROVIDER_STREAM_BYTES: usize = 32 * 1024 * 1024;
const STREAM_OVERHEAD_MULTIPLIER: usize = 64;

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
            .user_agent(concat!("minimal-agent/", env!("CARGO_PKG_VERSION")))
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
        let body = OpenAiRequest::from_model_request(&self.config.model, request)
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

        let max_output_bytes = usize::try_from(request.max_output_tokens)
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
            if event.data.trim() == "[DONE]" {
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
    fn model_id(&self) -> &str {
        &self.config.model
    }

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

fn delta_payload_bytes(delta: &ModelDelta) -> usize {
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

struct ProviderAttemptFault {
    fault: ProviderFault,
    retry_after: Option<Duration>,
    output_started: bool,
}

impl ProviderAttemptFault {
    fn retryable(fault: ProviderFault) -> Self {
        Self {
            fault,
            retry_after: None,
            output_started: false,
        }
    }

    fn terminal(fault: ProviderFault) -> Self {
        Self {
            fault,
            retry_after: None,
            output_started: true,
        }
    }

    fn can_retry(&self) -> bool {
        !self.output_started && is_retryable_fault(&self.fault)
    }
}

fn is_retryable_fault(fault: &ProviderFault) -> bool {
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

fn retry_delay(attempt: usize, retry_after: Option<Duration>) -> Duration {
    let exponent = attempt.saturating_sub(1).min(4) as u32;
    let backoff = INITIAL_RETRY_DELAY.saturating_mul(1_u32 << exponent);
    backoff
        .max(retry_after.unwrap_or_default())
        .min(MAX_RETRY_DELAY)
}

fn parse_retry_after(value: &HeaderValue) -> Option<Duration> {
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
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ModelAccumulator {
    reasoning: String,
    text: String,
    tool_calls: BTreeMap<usize, PartialToolCall>,
    usage: Option<TokenUsage>,
    finish_reason: Option<String>,
}

impl ModelAccumulator {
    fn push(&mut self, delta: ModelDelta) {
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

    fn finish(mut self) -> Result<ModelTurn, ProviderFault> {
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

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiTool<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<&'static str>,
    stream: bool,
    /// Ask OpenRouter-style providers to stream the model's reasoning so the
    /// root node's "thinking" is visible live in the transcript. Models with no
    /// reasoning channel ignore it, so it is safe to send unconditionally.
    include_reasoning: bool,
    max_completion_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

impl<'a> OpenAiRequest<'a> {
    fn from_model_request(
        model: &'a str,
        request: &'a ModelRequest,
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
                    kind: "function",
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
        let tool_choice = (!tools.is_empty()).then_some("auto");
        Ok(Self {
            model,
            messages,
            tools,
            tool_choice,
            stream: true,
            include_reasoning: true,
            max_completion_tokens: request.max_output_tokens,
            temperature: request.temperature,
        })
    }
}

#[derive(Serialize)]
struct OpenAiMessage<'a> {
    role: ModelRole,
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OpenAiRequestToolCall>,
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
                    kind: "function",
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
struct OpenAiRequestToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAiRequestFunction,
}

#[derive(Serialize)]
struct OpenAiRequestFunction {
    name: String,
    arguments: String,
}

#[derive(Serialize)]
struct OpenAiTool<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAiFunctionDefinition<'a>,
}

#[derive(Serialize)]
struct OpenAiFunctionDefinition<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a Value,
}

#[derive(Deserialize)]
struct OpenAiChunk {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

impl OpenAiChunk {
    fn into_deltas(self) -> Vec<ModelDelta> {
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

fn sanitize_model_content(text: &str) -> String {
    if !text.contains("<｜") && !text.contains("<|") {
        return text.to_owned();
    }
    let mut cleaned = text.to_owned();
    for token in [
        "<｜DSML｜tool_calls",
        "<｜DSML｜",
        "｜call>",
        "<｜tool_calls｜",
        "<｜tool call begin｜",
        "<｜tool call end｜",
        "<｜begin of sentence｜>",
        "<｜end of sentence｜>",
        "<|im_start|>",
        "<|im_end|>",
    ] {
        if cleaned.contains(token) {
            eprintln!("[provider] stripping leaked control token from content: {token}");
            cleaned = cleaned.replace(token, "");
        }
    }
    cleaned
}

#[derive(Deserialize)]
struct OpenAiChoice {
    #[serde(default)]
    delta: OpenAiResponseDelta,
    finish_reason: Option<String>,
}

#[derive(Default, Deserialize)]
struct OpenAiResponseDelta {
    #[serde(alias = "reasoning_content")]
    reasoning: Option<String>,
    #[serde(default)]
    reasoning_details: Vec<OpenAiReasoningDetail>,
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiResponseToolCall>,
}

#[derive(Deserialize)]
struct OpenAiResponseToolCall {
    index: usize,
    id: Option<String>,
    function: Option<OpenAiResponseFunction>,
}

#[derive(Deserialize)]
struct OpenAiResponseFunction {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderFault {
    #[error("provider rate limited the request: {message}")]
    RateLimited { message: String },
    #[error("provider payment is required: {message}")]
    PaymentRequired { message: String },
    #[error("provider authentication failed: {message}")]
    Authentication { message: String },
    #[error("model context overflowed: {message}")]
    ContextOverflow { message: String },
    #[error("provider upstream failed with HTTP {status}: {message}")]
    Upstream { status: u16, message: String },
    #[error("provider returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("provider transport failed: {message}")]
    Transport { message: String },
    #[error("provider stream failed: {message}")]
    Stream { message: String },
    #[error("provider response was invalid: {message}")]
    InvalidResponse { message: String },
    #[error("provider returned no assistant content or tool call")]
    EmptyCompletion,
    #[error("tool call {index} was malformed: {message}")]
    MalformedToolCall { index: usize, message: String },
    #[error("provider configuration is invalid: {message}")]
    Configuration { message: String },
}

impl ProviderFault {
    pub fn from_http(status: u16, message: impl Into<String>) -> Self {
        let message = message.into();
        let normalized = message.to_ascii_lowercase();
        match status {
            429 => Self::RateLimited { message },
            402 => Self::PaymentRequired { message },
            401 | 403 => Self::Authentication { message },
            400 | 413
                if normalized.contains("context")
                    && (normalized.contains("length")
                        || normalized.contains("token")
                        || normalized.contains("overflow")) =>
            {
                Self::ContextOverflow { message }
            }
            500..=599 => Self::Upstream { status, message },
            _ => Self::Http { status, message },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn provider_slot_starts_unconfigured_and_can_be_replaced_without_restart() {
        let slot = ProviderSlot::unconfigured(128_000);
        assert!(!slot.is_configured());
        assert!(matches!(
            slot.complete(ModelRequest::tools_disabled(Vec::new(), 32), None)
                .await,
            Err(ProviderFault::Configuration { .. })
        ));

        let configured = OpenAiChatProvider::new(OpenAiConfig {
            base_url: Url::parse("https://example.test/v1").unwrap(),
            api_key: "secret".to_owned(),
            model: "model-a".to_owned(),
            context_tokens: 64_000,
            timeout: Duration::from_secs(1),
            headers: HashMap::new(),
        })
        .unwrap();
        slot.replace(Arc::new(configured), "model-a".to_owned())
            .await;

        assert!(slot.is_configured());
        assert_eq!(slot.active_model().await.as_deref(), Some("model-a"));
    }

    #[test]
    fn explicit_context_limit_is_never_silently_defaulted() {
        assert_eq!(parse_context_tokens(None).unwrap(), 128_000);
        assert_eq!(parse_context_tokens(Some("8192")).unwrap(), 8_192);
        assert!(matches!(
            parse_context_tokens(Some("not-a-number")),
            Err(ProviderFault::Configuration { .. })
        ));
        assert!(matches!(
            parse_context_tokens(Some("0")),
            Err(ProviderFault::Configuration { .. })
        ));
    }

    #[test]
    fn openai_stream_preserves_provider_reasoning_for_live_display() {
        let chunk: OpenAiChunk = serde_json::from_value(serde_json::json!({
            "choices": [{
                "delta": {"reasoning_content": "checking the request"},
                "finish_reason": null
            }]
        }))
        .unwrap();

        assert_eq!(
            chunk.into_deltas(),
            vec![ModelDelta::Reasoning("checking the request".to_owned())]
        );
    }

    #[test]
    fn openai_stream_reads_structured_reasoning_text_for_live_display() {
        let chunk: OpenAiChunk = serde_json::from_value(serde_json::json!({
            "choices": [{
                "delta": {
                    "reasoning_details": [{
                        "type": "reasoning.text",
                        "text": "checking the tool result"
                    }]
                },
                "finish_reason": null
            }]
        }))
        .unwrap();

        assert_eq!(
            chunk.into_deltas(),
            vec![ModelDelta::Reasoning("checking the tool result".to_owned())]
        );
    }

    #[test]
    fn openai_request_explicitly_enables_automatic_tool_selection() {
        let request = ModelRequest {
            messages: vec![ModelMessage::new(ModelRole::User, "inspect the workspace")],
            tools: vec![ToolDefinition {
                name: "bash".to_owned(),
                description: "Run a command".to_owned(),
                parameters: serde_json::json!({"type":"object"}),
            }],
            tools_enabled: true,
            max_output_tokens: 256,
            temperature: None,
        };

        let body = OpenAiRequest::from_model_request("test-model", &request).unwrap();
        let json = serde_json::to_value(body).unwrap();

        assert_eq!(json["tool_choice"], "auto");
    }

    #[test]
    fn reasoning_only_stream_promotes_thoughts_to_text() {
        let turn = assemble_model_deltas(vec![
            ModelDelta::Reasoning("thinking step 1...".to_owned()),
            ModelDelta::Reasoning(" thinking step 2...".to_owned()),
            ModelDelta::Finished(Some("stop".to_owned())),
        ])
        .unwrap();

        assert_eq!(turn.text, "thinking step 1... thinking step 2...");
        assert!(turn.tool_calls.is_empty());
    }
}

#[derive(Deserialize)]
struct OpenAiReasoningDetail {
    text: Option<String>,
    summary: Option<String>,
}

impl OpenAiReasoningDetail {
    fn visible_text(self) -> Option<String> {
        self.text
            .or(self.summary)
            .filter(|content| !content.is_empty())
    }
}
