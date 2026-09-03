use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use minimal_agent::provider::{
    ModelDelta, ModelMessage, ModelProvider, ModelRequest, ModelRole, OpenAiChatProvider,
    OpenAiConfig, ProviderFault, TokenUsage, ToolCallDelta, assemble_model_deltas,
};

#[test]
fn streaming_deltas_form_one_complete_turn_without_losing_tool_fragments() {
    let turn = assemble_model_deltas(vec![
        ModelDelta::Text("I found ".into()),
        ModelDelta::Text("a lead.".into()),
        ModelDelta::ToolCall(ToolCallDelta {
            index: 0,
            id: Some("call-1".into()),
            name: Some("bash".into()),
            arguments_fragment: "{\"command\":".into(),
        }),
        ModelDelta::ToolCall(ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments_fragment: "\"pwd\"}".into(),
        }),
        ModelDelta::Usage(TokenUsage {
            input_tokens: 100,
            output_tokens: 20,
        }),
        ModelDelta::Finished(Some("tool_calls".into())),
    ])
    .unwrap();
    assert_eq!(turn.text, "I found a lead.");
    assert_eq!(turn.tool_calls.len(), 1);
    assert_eq!(turn.tool_calls[0].id, "call-1");
    assert_eq!(turn.tool_calls[0].name, "bash");
    assert_eq!(
        turn.tool_calls[0].arguments,
        serde_json::json!({"command": "pwd"})
    );
    assert_eq!(turn.usage.unwrap().input_tokens, 100);
    assert_eq!(turn.finish_reason.as_deref(), Some("tool_calls"));
}

#[test]
fn empty_stream_and_malformed_tool_arguments_fail_closed() {
    assert!(matches!(
        assemble_model_deltas(vec![ModelDelta::Finished(Some("stop".into()))]),
        Err(ProviderFault::EmptyCompletion)
    ));
    assert!(matches!(
        assemble_model_deltas(vec![
            ModelDelta::ToolCall(ToolCallDelta {
                index: 0,
                id: Some("call-1".into()),
                name: Some("bash".into()),
                arguments_fragment: "{bad".into(),
            }),
            ModelDelta::Finished(Some("tool_calls".into())),
        ]),
        Err(ProviderFault::MalformedToolCall { .. })
    ));
    assert!(matches!(
        assemble_model_deltas(vec![
            ModelDelta::ToolCall(ToolCallDelta {
                index: 0,
                id: Some("duplicate".into()),
                name: Some("bash".into()),
                arguments_fragment: "{}".into(),
            }),
            ModelDelta::ToolCall(ToolCallDelta {
                index: 1,
                id: Some("duplicate".into()),
                name: Some("workspace".into()),
                arguments_fragment: "{}".into(),
            }),
            ModelDelta::Finished(Some("tool_calls".into())),
        ]),
        Err(ProviderFault::MalformedToolCall { .. })
    ));
    assert!(matches!(
        assemble_model_deltas(vec![
            ModelDelta::Text("truncated".into()),
            ModelDelta::Finished(Some("length".into())),
        ]),
        Err(ProviderFault::InvalidResponse { .. })
    ));
}

#[test]
fn provider_http_faults_remain_distinct_from_context_overflow() {
    assert!(matches!(
        ProviderFault::from_http(429, "rate limited"),
        ProviderFault::RateLimited { .. }
    ));
    assert!(matches!(
        ProviderFault::from_http(402, "credits exhausted"),
        ProviderFault::PaymentRequired { .. }
    ));
    assert!(matches!(
        ProviderFault::from_http(502, "bad gateway"),
        ProviderFault::Upstream { status: 502, .. }
    ));
    assert!(matches!(
        ProviderFault::from_http(400, "maximum context length exceeded"),
        ProviderFault::ContextOverflow { .. }
    ));
    assert!(matches!(
        ProviderFault::from_http(401, "invalid key"),
        ProviderFault::Authentication { .. }
    ));
}

#[tokio::test]
async fn transient_http_failures_retry_inside_one_logical_provider_call() {
    let (base_url, requests, server) = scripted_http_server(vec![
        http_response(429, "Retry-After: 0\r\n", "rate limited"),
        http_response(502, "", "bad gateway"),
        sse_response("recovered"),
    ]);
    let provider = OpenAiChatProvider::new(OpenAiConfig {
        base_url: base_url.parse().unwrap(),
        api_key: "test-key".into(),
        model: "test-model".into(),
        context_tokens: 8_192,
        timeout: Duration::from_secs(3),
        headers: HashMap::new(),
    })
    .unwrap();

    let turn = provider
        .complete(
            ModelRequest::tools_disabled(vec![ModelMessage::new(ModelRole::User, "hello")], 256),
            None,
        )
        .await
        .unwrap();

    server.join().unwrap();
    assert_eq!(turn.text, "recovered");
    assert_eq!(requests.load(Ordering::Acquire), 3);
}

#[tokio::test]
async fn transient_retry_is_bounded_and_permanent_faults_fail_immediately() {
    let (base_url, requests, server) = scripted_http_server(vec![
        http_response(503, "Retry-After: 0\r\n", "unavailable one"),
        http_response(503, "Retry-After: 0\r\n", "unavailable two"),
        http_response(503, "Retry-After: 0\r\n", "unavailable three"),
    ]);
    let provider = test_provider(&base_url);
    assert!(matches!(
        provider.complete(test_request(), None).await,
        Err(ProviderFault::Upstream { status: 503, .. })
    ));
    server.join().unwrap();
    assert_eq!(requests.load(Ordering::Acquire), 3);

    let (base_url, requests, server) =
        scripted_http_server(vec![http_response(402, "", "credits exhausted")]);
    let provider = test_provider(&base_url);
    assert!(matches!(
        provider.complete(test_request(), None).await,
        Err(ProviderFault::PaymentRequired { .. })
    ));
    server.join().unwrap();
    assert_eq!(requests.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn partial_stream_is_not_accepted_or_retried_after_output_started() {
    let (base_url, requests, server) =
        scripted_http_server(vec![partial_sse_response("partial answer")]);
    let provider = test_provider(&base_url);
    let (delta_tx, mut delta_rx) = tokio::sync::mpsc::unbounded_channel();

    assert!(matches!(
        provider.complete(test_request(), Some(delta_tx)).await,
        Err(ProviderFault::Stream { .. })
    ));
    server.join().unwrap();
    assert_eq!(requests.load(Ordering::Acquire), 1);
    assert!(matches!(
        delta_rx.try_recv(),
        Ok(ModelDelta::Text(text)) if text == "partial answer"
    ));
}

#[tokio::test]
async fn provider_error_body_is_bounded_before_it_becomes_a_fault() {
    let (base_url, requests, server) =
        scripted_http_server(vec![http_response(402, "", &"x".repeat(256 * 1024))]);
    let provider = test_provider(&base_url);

    let error = provider.complete(test_request(), None).await.unwrap_err();
    server.join().unwrap();
    let ProviderFault::PaymentRequired { message } = error else {
        panic!("expected payment fault");
    };
    assert!(message.len() <= 16 * 1024 + 128);
    assert!(message.ends_with("[truncated]"));
    assert_eq!(requests.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn provider_output_cannot_exceed_the_requested_response_envelope() {
    let (base_url, requests, server) = scripted_http_server(vec![sse_response(&"x".repeat(5_000))]);
    let provider = test_provider(&base_url);

    assert!(matches!(
        provider.complete(test_request(), None).await,
        Err(ProviderFault::InvalidResponse { .. })
    ));
    server.join().unwrap();
    assert_eq!(requests.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn raw_sse_framing_is_bounded_before_event_parsing() {
    let (base_url, requests, server) =
        scripted_http_server(vec![raw_sse_response(&"x".repeat(70 * 1024))]);
    let provider = test_provider(&base_url);
    let request =
        ModelRequest::tools_disabled(vec![ModelMessage::new(ModelRole::User, "hello")], 1);

    let error = provider.complete(request, None).await.unwrap_err();
    server.join().unwrap();
    let ProviderFault::InvalidResponse { message } = error else {
        panic!("expected invalid response fault");
    };
    assert!(message.contains("stream envelope"));
    assert_eq!(requests.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn close_delimited_sse_is_bounded_while_streaming_without_content_length() {
    let (base_url, requests, server) =
        scripted_http_server(vec![close_delimited_raw_sse_response(
            &"x".repeat(70 * 1024),
        )]);
    let provider = test_provider(&base_url);
    let request =
        ModelRequest::tools_disabled(vec![ModelMessage::new(ModelRole::User, "hello")], 1);

    let error = provider.complete(request, None).await.unwrap_err();
    server.join().unwrap();
    let ProviderFault::InvalidResponse { message } = error else {
        panic!("expected invalid response fault");
    };
    assert!(message.contains("stream envelope"));
    assert_eq!(requests.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn retries_share_one_logical_call_deadline() {
    let (base_url, requests, server) = scripted_http_server(vec![http_response(
        503,
        "Retry-After: 5\r\n",
        "temporarily unavailable",
    )]);
    let provider = OpenAiChatProvider::new(OpenAiConfig {
        base_url: base_url.parse().unwrap(),
        api_key: "test-key".into(),
        model: "test-model".into(),
        context_tokens: 8_192,
        timeout: Duration::from_millis(100),
        headers: HashMap::new(),
    })
    .unwrap();
    let started = Instant::now();

    assert!(matches!(
        provider.complete(test_request(), None).await,
        Err(ProviderFault::Transport { .. })
    ));
    assert!(started.elapsed() < Duration::from_millis(400));
    server.join().unwrap();
    assert_eq!(requests.load(Ordering::Acquire), 1);
}

fn test_provider(base_url: &str) -> OpenAiChatProvider {
    OpenAiChatProvider::new(OpenAiConfig {
        base_url: base_url.parse().unwrap(),
        api_key: "test-key".into(),
        model: "test-model".into(),
        context_tokens: 8_192,
        timeout: Duration::from_secs(3),
        headers: HashMap::new(),
    })
    .unwrap()
}

fn test_request() -> ModelRequest {
    ModelRequest::tools_disabled(vec![ModelMessage::new(ModelRole::User, "hello")], 256)
}

fn scripted_http_server(
    responses: Vec<String>,
) -> (String, Arc<AtomicUsize>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let request_count = requests.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        while request_count.load(Ordering::Acquire) < responses.len() && Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut request = [0_u8; 16 * 1024];
                    let _ = stream.read(&mut request);
                    let index = request_count.fetch_add(1, Ordering::AcqRel);
                    stream.write_all(responses[index].as_bytes()).unwrap();
                    stream.flush().unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("test server failed: {error}"),
            }
        }
    });
    (format!("http://{address}/v1"), requests, server)
}

fn http_response(status: u16, headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} test\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n{body}",
        body.len()
    )
}

fn sse_response(text: &str) -> String {
    let body = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}},\"finish_reason\":null}}]}}\n\ndata: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"stop\"}}]}}\n\ndata: [DONE]\n\n",
        serde_json::to_string(text).unwrap()
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn partial_sse_response(text: &str) -> String {
    let body = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"content\":{}}},\"finish_reason\":null}}]}}\n\n",
        serde_json::to_string(text).unwrap()
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn raw_sse_response(data: &str) -> String {
    let body = format!("data: {data}\n\n");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn close_delimited_raw_sse_response(data: &str) -> String {
    let body = format!("data: {data}\n\n");
    format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n{body}")
}
