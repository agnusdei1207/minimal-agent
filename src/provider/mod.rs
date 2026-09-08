pub mod client;
pub mod config;
pub mod constants;
pub mod fault;
pub mod slot;
pub mod stream;
pub mod traits;
pub mod types;

pub use client::OpenAiChatProvider;
pub use config::OpenAiConfig;
pub use constants::*;
pub use fault::ProviderFault;
pub use slot::ProviderSlot;
pub use stream::{
    ModelAccumulator, assemble_model_deltas, has_control_token, sanitize_model_content,
};
pub use traits::ModelProvider;
pub use types::{
    ModelDelta, ModelMessage, ModelRequest, ModelRole, ModelTurn, TokenUsage, ToolCall,
    ToolCallDelta, ToolDefinition,
};

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;
    use url::Url;

    use super::client::OpenAiChatProvider;
    use super::client::OpenAiRequest;
    use super::config::OpenAiConfig;
    use super::fault::ProviderFault;
    use super::slot::ProviderSlot;
    use super::stream::{OpenAiChunk, assemble_model_deltas};
    use super::traits::ModelProvider;
    use super::types::{ModelDelta, ModelMessage, ModelRequest, ModelRole, ToolDefinition};

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
            max_output_tokens: None,
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

        let body = OpenAiRequest::from_model_request("test-model", &request, None).unwrap();
        let json = serde_json::to_value(body).unwrap();

        assert_eq!(json["tool_choice"], "auto");
        assert_eq!(json["max_completion_tokens"], 256);

        let overridden =
            OpenAiRequest::from_model_request("test-model", &request, Some(1024)).unwrap();
        let json_overridden = serde_json::to_value(overridden).unwrap();
        assert_eq!(json_overridden["max_completion_tokens"], 1024);
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
