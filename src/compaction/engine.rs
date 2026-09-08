use serde_json::json;
use std::sync::Arc;

use crate::domain::{AgentId, ContextBudget, InsightId, SequenceRange, estimate_tokens};
use crate::provider::{ModelMessage, ModelProvider, ModelRequest, ModelRole};

use super::config::SemanticCompactionConfig;
use super::constants::COMPACTION_SYSTEM_PROMPT;
use super::error::{AttemptError, CompactionError};
use super::partition::{
    entry_tokens, make_partitions, source_digest, unique_ids, unique_insights, unique_ranges,
};
use super::types::{
    CompactionOutcome, CompactionResponse, CompactionResult, ContextEntry, LiveReason,
};

pub struct SemanticCompactor {
    provider: Arc<dyn ModelProvider>,
    config: SemanticCompactionConfig,
}

impl SemanticCompactor {
    pub fn new(provider: Arc<dyn ModelProvider>, config: SemanticCompactionConfig) -> Self {
        Self { provider, config }
    }

    pub async fn compact(
        &self,
        agent_id: &AgentId,
        budget: ContextBudget,
        current_brief: &str,
        mut entries: Vec<ContextEntry>,
        fixed_tokens: u64,
    ) -> Result<CompactionOutcome, CompactionError> {
        let config = self.config.validate()?;
        let before_tokens = fixed_tokens
            .saturating_add(estimate_tokens(current_brief))
            .saturating_add(entries.iter().map(entry_tokens).sum::<u64>());
        if !budget.needs_compaction(before_tokens) {
            return Ok(CompactionOutcome::NotNeeded);
        }
        protect_latest_completed_exchange(&mut entries);
        let eligible: Vec<_> = entries
            .iter()
            .filter(|entry| entry.live_reason.is_none())
            .cloned()
            .collect();
        let live_tail: Vec<_> = entries
            .into_iter()
            .filter(|entry| entry.live_reason.is_some())
            .collect();
        if eligible.is_empty() {
            return Err(CompactionError::ContextCompactionBlocked {
                first: "no completed source is eligible for compaction".to_owned(),
                second: "protected live state cannot be discarded".to_owned(),
            });
        }
        let source_sha256 = source_digest(&eligible)?;
        let required_ranges = eligible.iter().map(|entry| entry.range).collect::<Vec<_>>();
        let required_insights = unique_insights(&eligible);

        let first = self
            .run_attempt(
                agent_id,
                current_brief,
                &eligible,
                &source_sha256,
                &required_ranges,
                &required_insights,
                config.partition_tokens,
                config.overlap_tokens,
                budget,
            )
            .await;
        let response = match first {
            Ok(response) => response,
            Err(AttemptError::Provider(error)) => return Err(CompactionError::Provider(error)),
            Err(AttemptError::Invalid(_first_reason)) => {
                match self
                    .run_attempt(
                        agent_id,
                        current_brief,
                        &eligible,
                        &source_sha256,
                        &required_ranges,
                        &required_insights,
                        config.retry_partition_tokens,
                        config.overlap_tokens.min(config.retry_partition_tokens - 1),
                        budget,
                    )
                    .await
                {
                    Ok(response) => response,
                    Err(AttemptError::Provider(error)) => {
                        return Err(CompactionError::Provider(error));
                    }
                    Err(AttemptError::Invalid(_second)) => {
                        // Both semantic attempts produced unusable output (common with
                        // weak models). Do not block forever — fall back to a bounded
                        // mechanical trim (INTENT-0001 §9; risk-management pre-mortem).
                        return Ok(mechanical_trim(&live_tail));
                    }
                }
            }
        };

        let projected_tokens = fixed_tokens
            .saturating_add(estimate_tokens(&response.markdown))
            .saturating_add(live_tail.iter().map(entry_tokens).sum::<u64>());
        if projected_tokens >= before_tokens || projected_tokens > budget.target_input_tokens() {
            // The semantic summary did not shrink the context enough. Rather than
            // block, fall back to a bounded mechanical trim.
            return Ok(mechanical_trim(&live_tail));
        }
        Ok(CompactionOutcome::Compacted(CompactionResult {
            markdown: response.markdown,
            coverage: response.coverage,
            source_sha256,
            source_ranges: required_ranges,
            live_tail,
            projected_tokens,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_attempt(
        &self,
        agent_id: &AgentId,
        current_brief: &str,
        entries: &[ContextEntry],
        source_sha256: &str,
        required_ranges: &[SequenceRange],
        required_insights: &[InsightId],
        partition_tokens: u64,
        overlap_tokens: u64,
        budget: ContextBudget,
    ) -> Result<CompactionResponse, AttemptError> {
        let partitions = make_partitions(entries, partition_tokens, overlap_tokens)
            .map_err(AttemptError::Invalid)?;
        let mut summaries = Vec::with_capacity(partitions.len());
        for partition in partitions {
            let input = json!({
                "phase": "partition",
                "agent_id": agent_id,
                "current_brief": current_brief,
                "source_sha256": source_sha256,
                "required_ranges": partition.ranges,
                "required_insight_ids": partition.insight_ids,
                "source": partition.source,
            });
            let response = self
                .call_provider(input, budget.brief_target_tokens())
                .await?;
            validate_response(
                &response,
                source_sha256,
                &partition.ranges,
                &partition.insight_ids,
                budget,
            )?;
            summaries.push(response);
        }

        while summaries.len() > 1 {
            let mut next = Vec::with_capacity(summaries.len().div_ceil(2));
            let mut iter = summaries.into_iter();
            while let Some(left) = iter.next() {
                let Some(right) = iter.next() else {
                    next.push(left);
                    break;
                };
                let pair_ranges = unique_ranges(
                    left.coverage
                        .covered_ranges
                        .iter()
                        .chain(right.coverage.covered_ranges.iter())
                        .copied(),
                );
                let pair_insights = unique_ids(
                    left.coverage
                        .covered_insight_ids
                        .iter()
                        .chain(left.coverage.superseded_insight_ids.iter())
                        .chain(right.coverage.covered_insight_ids.iter())
                        .chain(right.coverage.superseded_insight_ids.iter())
                        .cloned(),
                );
                let input = json!({
                    "phase": "fold",
                    "agent_id": agent_id,
                    "current_brief": current_brief,
                    "source_sha256": source_sha256,
                    "required_ranges": pair_ranges,
                    "required_insight_ids": pair_insights,
                    "partials": [left, right],
                });
                let response = self
                    .call_provider(input, budget.brief_target_tokens())
                    .await?;
                validate_response(
                    &response,
                    source_sha256,
                    &pair_ranges,
                    &pair_insights,
                    budget,
                )?;
                next.push(response);
            }
            summaries = next;
        }
        let final_response = summaries
            .pop()
            .ok_or_else(|| AttemptError::Invalid("LLM produced no summaries".to_owned()))?;
        validate_response(
            &final_response,
            source_sha256,
            required_ranges,
            required_insights,
            budget,
        )?;
        Ok(final_response)
    }

    async fn call_provider(
        &self,
        input: serde_json::Value,
        max_output_tokens: u64,
    ) -> Result<CompactionResponse, AttemptError> {
        let request = ModelRequest::tools_disabled(
            vec![
                ModelMessage::new(ModelRole::System, COMPACTION_SYSTEM_PROMPT),
                ModelMessage::new(ModelRole::User, input.to_string()),
            ],
            max_output_tokens.max(1),
        );
        let turn = self
            .provider
            .complete(request, None)
            .await
            .map_err(AttemptError::Provider)?;
        if !turn.tool_calls.is_empty() {
            return Err(AttemptError::Invalid(
                "compaction attempted to call a tool".to_owned(),
            ));
        }
        serde_json::from_str(extract_json_object(&turn.text))
            .map_err(|error| AttemptError::Invalid(format!("invalid compaction JSON: {error}")))
    }
}

/// Weaker models sometimes wrap the compaction JSON in markdown code fences
/// (```json … ```) or surrounding prose. Take the span from the first `{` to the
/// last `}` so parsing is robust to that instead of failing at column 1.
pub fn extract_json_object(text: &str) -> &str {
    let trimmed = text.trim();
    match (trimmed.find('{'), trimmed.rfind('}')) {
        (Some(start), Some(end)) if end > start => &trimmed[start..=end],
        _ => trimmed,
    }
}

/// Last-resort mechanical fallback when semantic compaction cannot proceed
/// (unusable model output, or a summary that does not reduce enough). Blocking
/// forever stalls the agent as context keeps growing, so instead keep the
/// protected live tail plus the newest eligible entries that fit under target and
/// drop the older ones from context. The brief is untouched and the run journal
/// still holds every dropped entry, bounding working memory without losing the
/// durable record (INTENT-0001 §9; risk-management pre-mortem: a fail-closed block
/// must have a bounded fallback).
pub fn mechanical_trim(live_tail: &[ContextEntry]) -> CompactionOutcome {
    // Keep EXACTLY the protected live tail — the same set the semantic path keeps —
    // and drop every other eligible entry from context. Keeping arbitrary extra
    // entries could split a tool-call/tool-result atomic group (dropping the call
    // while keeping the result), which the provider rejects ("tool id not found").
    // The run journal still holds every dropped entry, so nothing is lost.
    let kept_ranges = live_tail.iter().map(|entry| entry.range).collect();
    CompactionOutcome::MechanicallyTrimmed { kept_ranges }
}

pub fn protect_latest_completed_exchange(entries: &mut [ContextEntry]) {
    for entry in entries.iter_mut() {
        if entry.live_reason == Some(LiveReason::RecentExchange) {
            entry.live_reason = None;
        }
    }
    let Some(latest) = entries
        .iter()
        .rposition(|entry| entry.live_reason.is_none())
    else {
        return;
    };
    if let Some(group) = entries[latest].atomic_group.clone() {
        for entry in entries.iter_mut().filter(|entry| {
            entry.live_reason.is_none() && entry.atomic_group.as_ref() == Some(&group)
        }) {
            entry.live_reason = Some(LiveReason::RecentExchange);
        }
    } else {
        entries[latest].live_reason = Some(LiveReason::RecentExchange);
    }
}

pub fn validate_response(
    response: &CompactionResponse,
    source_sha256: &str,
    required_ranges: &[SequenceRange],
    required_insights: &[InsightId],
    budget: ContextBudget,
) -> Result<(), AttemptError> {
    if response.markdown.trim().is_empty() {
        return Err(AttemptError::Invalid(
            "compaction returned empty markdown".to_owned(),
        ));
    }
    if response.source_sha256 != source_sha256 {
        return Err(AttemptError::Invalid(
            "compaction source digest is stale".to_owned(),
        ));
    }
    response
        .coverage
        .validate(required_ranges, required_insights)
        .map_err(|_| AttemptError::Invalid("compaction coverage is incomplete".to_owned()))?;
    let tokens = estimate_tokens(&response.markdown);
    if tokens > budget.brief_target_tokens() {
        return Err(AttemptError::Invalid(format!(
            "brief uses {tokens} tokens, above target {}",
            budget.brief_target_tokens()
        )));
    }
    Ok(())
}
