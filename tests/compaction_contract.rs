use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use pentesting::compaction::{
    CompactionError, CompactionOutcome, ContextEntry, LiveReason, SemanticCompactionConfig,
    SemanticCompactor,
};
use pentesting::domain::{AgentId, ContextBudget, InsightId, SequenceRange, estimate_tokens};
use pentesting::provider::{ModelProvider, ModelRequest, ModelTurn, ProviderFault, TokenUsage};

#[derive(Clone)]
enum FakeMode {
    Valid,
    InvalidOnce,
    AlwaysMissingCoverage,
    RateLimited,
}

struct CoveringProvider {
    requests: Mutex<Vec<ModelRequest>>,
    mode: Mutex<FakeMode>,
    calls: AtomicUsize,
}

impl CoveringProvider {
    fn new(mode: FakeMode) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            mode: Mutex::new(mode),
            calls: AtomicUsize::new(0),
        }
    }

    fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().unwrap().clone()
    }

    fn set_mode(&self, mode: FakeMode) {
        *self.mode.lock().unwrap() = mode;
        self.calls.store(0, Ordering::Release);
    }
}

#[async_trait]
impl ModelProvider for CoveringProvider {
    fn context_limit(&self) -> u64 {
        20_000
    }

    async fn complete(
        &self,
        request: ModelRequest,
        _deltas: Option<tokio::sync::mpsc::UnboundedSender<pentesting::provider::ModelDelta>>,
    ) -> Result<ModelTurn, ProviderFault> {
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        self.requests.lock().unwrap().push(request.clone());
        match self.mode.lock().unwrap().clone() {
            FakeMode::RateLimited => {
                return Err(ProviderFault::RateLimited {
                    message: "slow down".into(),
                });
            }
            FakeMode::InvalidOnce if call == 0 => {
                return Ok(turn("{}"));
            }
            _ => {}
        }

        let input: serde_json::Value =
            serde_json::from_str(&request.messages.last().unwrap().content).unwrap();
        let mut ranges = input["required_ranges"].clone();
        let mut insights = input["required_insight_ids"].clone();
        if matches!(*self.mode.lock().unwrap(), FakeMode::AlwaysMissingCoverage) {
            if let Some(values) = ranges.as_array_mut() {
                values.pop();
            }
            if let Some(values) = insights.as_array_mut() {
                values.pop();
            }
        }
        let output = serde_json::json!({
            "markdown": "# Worker Agent Brief\n## Assignment\nContinue\n## Current State\nRunning\n## Attempts by Domain\n- grouped families\n## Curated Knowledge\n### Facts & Successes\nNo durable insight yet\n### Hypotheses & Directions\nNo durable insight yet\n### Dead Ends\nNo durable insight yet\n## Integrated Messages\nNone\n## Blockers\nNone\n## Next Move\nContinue\n",
            "source_sha256": input["source_sha256"],
            "covered_ranges": ranges,
            "covered_insight_ids": insights,
            "superseded_insight_ids": []
        });
        Ok(turn(&output.to_string()))
    }
}

fn turn(text: &str) -> ModelTurn {
    ModelTurn {
        text: text.into(),
        tool_calls: vec![],
        usage: Some(TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        }),
        finish_reason: Some("stop".into()),
    }
}

fn entry(start: u64, end: u64, content: String) -> ContextEntry {
    ContextEntry::completed(SequenceRange::new(start, end).unwrap(), content)
}

fn config() -> SemanticCompactionConfig {
    SemanticCompactionConfig {
        partition_tokens: 600,
        retry_partition_tokens: 300,
        overlap_tokens: 16,
    }
}

fn budget() -> ContextBudget {
    ContextBudget::new(10_000, 20_000, 1_000).unwrap()
}

#[tokio::test]
async fn below_eighty_percent_does_not_call_the_provider() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::Valid));
    let compactor = SemanticCompactor::new(provider.clone(), config());
    let result = compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "small brief",
            vec![entry(1, 1, "small context".into())],
            100,
        )
        .await
        .unwrap();
    assert!(matches!(result, CompactionOutcome::NotNeeded));
    assert!(provider.requests().is_empty());
}

#[tokio::test]
async fn every_source_partition_uses_the_llm_in_sequence_and_live_tail_is_exact() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::Valid));
    let compactor = SemanticCompactor::new(provider.clone(), config());
    let entries = vec![
        entry(1, 2, format!("FIRST-MARKER {}", "a".repeat(12_000)))
            .with_insight(InsightId::new("I-first").unwrap()),
        entry(3, 4, format!("MIDDLE-MARKER {}", "b".repeat(12_000)))
            .with_insight(InsightId::new("I-middle").unwrap()),
        entry(5, 6, format!("LAST-MARKER {}", "c".repeat(12_000)))
            .with_insight(InsightId::new("I-last").unwrap()),
        entry(7, 7, "UNREAD-EXACT".into()).protect(LiveReason::UnreadInbox),
        entry(8, 8, "RECENT-EXACT".into()).protect(LiveReason::RecentExchange),
    ];
    let result = compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "existing brief",
            entries,
            100,
        )
        .await
        .unwrap();
    let CompactionOutcome::Compacted(result) = result else {
        panic!("compaction did not run")
    };
    assert_eq!(result.live_tail.len(), 2);
    assert_eq!(result.live_tail[0].content, "UNREAD-EXACT");
    assert_eq!(result.live_tail[1].content, "RECENT-EXACT");
    assert_eq!(result.coverage.covered_insight_ids.len(), 3);
    let requests = provider.requests();
    assert!(requests.iter().all(|request| !request.tools_enabled));
    let partition_inputs: Vec<_> = requests
        .iter()
        .filter_map(|request| {
            let value: serde_json::Value =
                serde_json::from_str(&request.messages.last()?.content).ok()?;
            (value["phase"] == "partition").then_some(value)
        })
        .collect();
    assert!(partition_inputs.len() >= 3);
    let all_source = partition_inputs
        .iter()
        .filter_map(|input| input["source"].as_str())
        .collect::<String>();
    assert!(all_source.contains("FIRST-MARKER"));
    assert!(all_source.contains("MIDDLE-MARKER"));
    assert!(all_source.contains("LAST-MARKER"));
    assert!(!all_source.contains("UNREAD-EXACT"));
    assert!(!all_source.contains("RECENT-EXACT"));
}

#[tokio::test]
async fn tool_call_and_result_atomic_group_stay_together() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::Valid));
    let compactor = SemanticCompactor::new(provider.clone(), config());
    let entries = vec![
        entry(
            1,
            1,
            format!("TOOL-CALL {} TOOL-CALL-END", "a".repeat(12_000)),
        )
        .atomic("call-1"),
        entry(
            2,
            2,
            format!("TOOL-RESULT {} TOOL-RESULT-END", "b".repeat(12_000)),
        )
        .atomic("call-1"),
        entry(3, 3, "RECENT".into()).protect(LiveReason::RecentExchange),
    ];
    compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            entries,
            2_000,
        )
        .await
        .unwrap();
    let requests = provider.requests();
    let sources: Vec<_> = requests
        .iter()
        .filter_map(|request| {
            let value: serde_json::Value =
                serde_json::from_str(&request.messages.last()?.content).ok()?;
            (value["phase"] == "partition")
                .then(|| value["source"].as_str().unwrap_or_default().to_owned())
        })
        .collect();
    assert_eq!(sources.len(), 1);
    assert!(sources[0].contains("TOOL-CALL-END"));
    assert!(sources[0].contains("TOOL-RESULT-END"));
}

#[tokio::test]
async fn latest_tool_exchange_is_preserved_as_one_live_group() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::Valid));
    let compactor = SemanticCompactor::new(provider, config());
    let result = compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            vec![
                entry(1, 1, "old context ".repeat(3_000)),
                entry(2, 2, "assistant tool request".into()).atomic("model-turn-2"),
                entry(3, 4, "tool result".into()).atomic("model-turn-2"),
            ],
            100,
        )
        .await
        .unwrap();
    let CompactionOutcome::Compacted(result) = result else {
        panic!("compaction did not run")
    };

    assert_eq!(result.live_tail.len(), 2);
    assert_eq!(result.live_tail[0].content, "assistant tool request");
    assert_eq!(result.live_tail[1].content, "tool result");
}

#[tokio::test]
async fn each_compaction_moves_recent_exchange_protection_to_the_newest_entry() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::Valid));
    let compactor = SemanticCompactor::new(provider, config());
    let result = compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            vec![
                entry(1, 1, "previously recent ".repeat(3_000)).protect(LiveReason::RecentExchange),
                entry(2, 2, "newest exchange".into()),
            ],
            100,
        )
        .await
        .unwrap();
    let CompactionOutcome::Compacted(result) = result else {
        panic!("compaction did not run")
    };

    assert_eq!(result.live_tail.len(), 1);
    assert_eq!(result.live_tail[0].content, "newest exchange");
    assert_eq!(
        result.source_ranges,
        vec![SequenceRange::new(1, 1).unwrap()]
    );
}

#[tokio::test]
async fn multilingual_source_is_partitioned_by_the_shared_token_estimator() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::Valid));
    let compactor = SemanticCompactor::new(provider.clone(), config());
    compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            vec![
                entry(1, 1, "가".repeat(2_000)),
                entry(2, 2, "RECENT".into()).protect(LiveReason::RecentExchange),
            ],
            2_000,
        )
        .await
        .unwrap();

    let sources = provider
        .requests()
        .iter()
        .filter_map(|request| {
            let input: serde_json::Value =
                serde_json::from_str(&request.messages.last()?.content).ok()?;
            if input["phase"] != "partition" {
                return None;
            }
            input["source"].as_str().map(str::to_owned)
        })
        .collect::<Vec<_>>();
    assert!(sources.len() > 1);
    assert!(
        sources
            .iter()
            .all(|source| estimate_tokens(source) <= config().partition_tokens)
    );
}

#[tokio::test]
async fn invalid_output_retries_once_with_smaller_partitions() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::InvalidOnce));
    let compactor = SemanticCompactor::new(provider.clone(), config());
    let result = compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            vec![
                entry(1, 1, "x".repeat(30_000)).with_insight(InsightId::new("I-1").unwrap()),
                entry(2, 2, "RECENT".into()).protect(LiveReason::RecentExchange),
            ],
            100,
        )
        .await
        .unwrap();
    assert!(matches!(result, CompactionOutcome::Compacted(_)));
    assert!(provider.requests().len() > 1);
}

#[tokio::test]
async fn two_invalid_attempts_fall_back_mechanically_without_a_permanent_latch() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::AlwaysMissingCoverage));
    let compactor = SemanticCompactor::new(provider.clone(), config());
    let entries = vec![
        entry(1, 2, "x".repeat(30_000)).with_insight(InsightId::new("I-1").unwrap()),
        entry(3, 3, "RECENT".into()).protect(LiveReason::RecentExchange),
    ];
    // Two unusable semantic attempts must NOT block forever; they fall back to a
    // bounded mechanical trim that always keeps the protected live tail.
    match compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            entries.clone(),
            100,
        )
        .await
        .unwrap()
    {
        CompactionOutcome::MechanicallyTrimmed { kept_ranges } => {
            // Keeps ONLY the protected live tail (3,3) — never an eligible entry
            // (1,2). Keeping an eligible entry could split a tool-call/result atomic
            // group and orphan a tool result (provider "tool id not found").
            assert!(
                kept_ranges
                    .iter()
                    .any(|range| range.start == 3 && range.end == 3)
            );
            assert!(
                !kept_ranges
                    .iter()
                    .any(|range| range.start == 1 && range.end == 2)
            );
        }
        other => panic!("expected a mechanical trim, got {other:?}"),
    }
    provider.set_mode(FakeMode::Valid);
    assert!(matches!(
        compactor
            .compact(
                &AgentId::new("worker-01").unwrap(),
                budget(),
                "brief",
                entries,
                100,
            )
            .await
            .unwrap(),
        CompactionOutcome::Compacted(_)
    ));
}

#[tokio::test]
async fn rate_limit_is_not_misclassified_as_compaction_failure() {
    let provider = Arc::new(CoveringProvider::new(FakeMode::RateLimited));
    let compactor = SemanticCompactor::new(provider, config());
    let error = compactor
        .compact(
            &AgentId::new("worker-01").unwrap(),
            budget(),
            "brief",
            vec![
                entry(1, 1, "x".repeat(30_000)),
                entry(2, 2, "RECENT".into()).protect(LiveReason::RecentExchange),
            ],
            100,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        CompactionError::Provider(ProviderFault::RateLimited { .. })
    ));
}
