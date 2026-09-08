use std::collections::HashSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::domain::{
    AgentId, CompactionCoverage, ContextBudget, DomainError, InsightId, SequenceRange,
    estimate_tokens,
};
use crate::provider::{ModelMessage, ModelProvider, ModelRequest, ModelRole, ProviderFault};

const COMPACTION_SYSTEM_PROMPT: &str = r#"You curate one agent's current brief.
Summarize meaning, never discard a supplied source range, and return JSON only.
Group repetitive attempts by domain or technique. Preserve exact FACT and SUCCESS values with
journal references. Keep HYPOTHESIS and DIRECTION unverified. Record why DEAD_END paths failed.
Do not invent facts, credentials, completion, or progress. Do not copy raw payload lists.
Return a complete replacement brief, not a fragment. A main brief must retain exactly one
minimal-agent:runtime start/end block from current_brief and these headings: Main Agent Brief,
Goal & Constraints, Battlefield, Curated Knowledge, Facts & Successes, Hypotheses & Directions,
Dead Ends, Blockers, Next Moves. A worker brief must retain: Worker Agent Brief, Assignment,
Current State, Attempts by Domain, Curated Knowledge, Facts & Successes, Hypotheses & Directions,
Dead Ends, Integrated Messages, Blockers, Next Move.
Return markdown, source_sha256, covered_ranges, covered_insight_ids, and
superseded_insight_ids. Tools are unavailable during compaction."#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveReason {
    UnreadInbox,
    IncompleteTool,
    PartialOutput,
    RecentExchange,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextEntry {
    pub range: SequenceRange,
    pub content: String,
    pub insight_ids: Vec<InsightId>,
    pub atomic_group: Option<String>,
    pub live_reason: Option<LiveReason>,
}

impl ContextEntry {
    pub fn completed(range: SequenceRange, content: impl Into<String>) -> Self {
        Self {
            range,
            content: content.into(),
            insight_ids: Vec::new(),
            atomic_group: None,
            live_reason: None,
        }
    }

    pub fn with_insight(mut self, insight_id: InsightId) -> Self {
        if !self.insight_ids.contains(&insight_id) {
            self.insight_ids.push(insight_id);
        }
        self
    }

    pub fn atomic(mut self, group: impl Into<String>) -> Self {
        self.atomic_group = Some(group.into());
        self
    }

    pub fn protect(mut self, reason: LiveReason) -> Self {
        self.live_reason = Some(reason);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticCompactionConfig {
    pub partition_tokens: u64,
    pub retry_partition_tokens: u64,
    pub overlap_tokens: u64,
}

impl Default for SemanticCompactionConfig {
    fn default() -> Self {
        Self {
            partition_tokens: 24_000,
            retry_partition_tokens: 12_000,
            overlap_tokens: 128,
        }
    }
}

impl SemanticCompactionConfig {
    fn validate(self) -> Result<Self, CompactionError> {
        let valid = self.partition_tokens > 0
            && self.retry_partition_tokens > 0
            && self.retry_partition_tokens <= self.partition_tokens
            && self.overlap_tokens < self.retry_partition_tokens;
        valid.then_some(self).ok_or(CompactionError::InvalidConfig)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionResult {
    pub markdown: String,
    pub coverage: CompactionCoverage,
    pub source_sha256: String,
    pub source_ranges: Vec<SequenceRange>,
    pub live_tail: Vec<ContextEntry>,
    pub projected_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactionOutcome {
    NotNeeded,
    Compacted(CompactionResult),
    /// Last-resort fallback when semantic compaction cannot proceed: keep only
    /// these context ranges (the protected live tail — the same kept-set the
    /// semantic path uses, so tool-call/result atomic groups are never split) and
    /// drop the rest from context. The brief is unchanged and the run journal still
    /// holds every dropped entry (INTENT-0001 §9 ledger). Prevents a fail-closed
    /// compaction from stalling the agent forever.
    MechanicallyTrimmed {
        kept_ranges: Vec<SequenceRange>,
    },
}

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
fn extract_json_object(text: &str) -> &str {
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
fn mechanical_trim(live_tail: &[ContextEntry]) -> CompactionOutcome {
    // Keep EXACTLY the protected live tail — the same set the semantic path keeps —
    // and drop every other eligible entry from context. Keeping arbitrary extra
    // entries could split a tool-call/tool-result atomic group (dropping the call
    // while keeping the result), which the provider rejects ("tool id not found").
    // The run journal still holds every dropped entry, so nothing is lost.
    let kept_ranges = live_tail.iter().map(|entry| entry.range).collect();
    CompactionOutcome::MechanicallyTrimmed { kept_ranges }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CompactionResponse {
    markdown: String,
    source_sha256: String,
    #[serde(flatten)]
    coverage: CompactionCoverage,
}

#[derive(Debug)]
enum AttemptError {
    Provider(ProviderFault),
    Invalid(String),
}

impl From<ProviderFault> for AttemptError {
    fn from(value: ProviderFault) -> Self {
        Self::Provider(value)
    }
}

#[derive(Debug)]
struct Partition {
    source: String,
    ranges: Vec<SequenceRange>,
    insight_ids: Vec<InsightId>,
}

#[derive(Debug)]
struct SourceUnit {
    prefix: String,
    body: String,
    ranges: Vec<SequenceRange>,
    insight_ids: Vec<InsightId>,
    atomic: bool,
}

fn make_partitions(
    entries: &[ContextEntry],
    partition_tokens: u64,
    overlap_tokens: u64,
) -> Result<Vec<Partition>, String> {
    if partition_tokens == 0 || overlap_tokens >= partition_tokens {
        return Err("invalid partition bounds".to_owned());
    }
    let units = make_units(entries);
    let mut pieces = Vec::new();
    for unit in units {
        pieces.extend(split_unit(unit, partition_tokens, overlap_tokens)?);
    }
    if pieces.is_empty() {
        return Err("no source partition was produced".to_owned());
    }

    let mut partitions: Vec<Partition> = Vec::new();
    for piece in pieces {
        if let Some(last) = partitions.last_mut()
            && estimate_tokens(&last.source)
                .saturating_add(estimate_tokens(&piece.source))
                .saturating_add(1)
                <= partition_tokens
        {
            last.source.push_str("\n\n");
            last.source.push_str(&piece.source);
            last.ranges = unique_ranges(last.ranges.iter().chain(piece.ranges.iter()).copied());
            last.insight_ids = unique_ids(
                last.insight_ids
                    .iter()
                    .chain(piece.insight_ids.iter())
                    .cloned(),
            );
            continue;
        }
        partitions.push(piece);
    }
    Ok(partitions)
}

fn make_units(entries: &[ContextEntry]) -> Vec<SourceUnit> {
    let mut units = Vec::new();
    let mut index = 0;
    while index < entries.len() {
        let group = entries[index].atomic_group.clone();
        let atomic = group.is_some();
        let mut grouped = vec![entries[index].clone()];
        index += 1;
        if let Some(group_id) = &group {
            while index < entries.len() && entries[index].atomic_group.as_ref() == Some(group_id) {
                grouped.push(entries[index].clone());
                index += 1;
            }
        }
        let member_labels = grouped
            .iter()
            .map(|entry| {
                entry
                    .content
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let prefix = match group {
            Some(group) => format!(
                "ATOMIC GROUP {group}\nMEMBERS: {}\n",
                member_labels.join(" | ")
            ),
            None => format!(
                "SOURCE {}..={}\n",
                grouped[0].range.start, grouped[0].range.end
            ),
        };
        let body = grouped
            .iter()
            .map(|entry| {
                format!(
                    "[sequence {}..={}]\n{}",
                    entry.range.start, entry.range.end, entry.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n---\n");
        units.push(SourceUnit {
            prefix,
            body,
            ranges: unique_ranges(grouped.iter().map(|entry| entry.range)),
            insight_ids: unique_insights(&grouped),
            atomic,
        });
    }
    units
}

fn split_unit(
    unit: SourceUnit,
    max_tokens: u64,
    overlap_tokens: u64,
) -> Result<Vec<Partition>, String> {
    let whole = format!("{}{}", unit.prefix, unit.body);
    let body: Vec<char> = unit.body.chars().collect();
    if unit.atomic || estimate_tokens(&whole) <= max_tokens {
        return Ok(vec![Partition {
            source: whole,
            ranges: unit.ranges,
            insight_ids: unit.insight_ids,
        }]);
    }
    let mut partitions = Vec::new();
    let mut start = 0;
    while start < body.len() {
        let end = largest_fitting_end(&unit.prefix, &body, start, max_tokens).ok_or_else(|| {
            format!("partition token bound {max_tokens} is too small for source metadata")
        })?;
        partitions.push(Partition {
            source: render_part(&unit.prefix, &body, start, end),
            ranges: unit.ranges.clone(),
            insight_ids: unit.insight_ids.clone(),
        });
        if end == body.len() {
            break;
        }
        start = overlap_start(&body, start, end, overlap_tokens)
            .max(start.saturating_add(1))
            .min(end);
    }
    Ok(partitions)
}

fn largest_fitting_end(
    prefix: &str,
    body: &[char],
    start: usize,
    max_tokens: u64,
) -> Option<usize> {
    let mut low = start.saturating_add(1);
    let mut high = body.len();
    let mut best = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        if estimate_tokens(&render_part(prefix, body, start, middle)) <= max_tokens {
            best = Some(middle);
            low = middle.saturating_add(1);
        } else {
            high = middle.saturating_sub(1);
        }
    }
    best
}

fn overlap_start(body: &[char], start: usize, end: usize, overlap_tokens: u64) -> usize {
    if overlap_tokens == 0 {
        return end;
    }
    let mut low = start;
    let mut high = end;
    let mut best = end;
    while low <= high {
        let middle = low + (high - low) / 2;
        let suffix = body[middle..end].iter().collect::<String>();
        if estimate_tokens(&suffix) <= overlap_tokens {
            best = middle;
            if middle == 0 {
                break;
            }
            high = middle - 1;
        } else {
            low = middle.saturating_add(1);
        }
    }
    best
}

fn render_part(prefix: &str, body: &[char], start: usize, end: usize) -> String {
    let slice = body[start..end].iter().collect::<String>();
    format!("{prefix}PART {start}..{end}\n{slice}")
}

fn validate_response(
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

fn protect_latest_completed_exchange(entries: &mut [ContextEntry]) {
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

fn source_digest(entries: &[ContextEntry]) -> Result<String, CompactionError> {
    let bytes = serde_json::to_vec(entries).map_err(|error| CompactionError::Serialization {
        message: error.to_string(),
    })?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn unique_insights(entries: &[ContextEntry]) -> Vec<InsightId> {
    unique_ids(
        entries
            .iter()
            .flat_map(|entry| entry.insight_ids.iter())
            .cloned(),
    )
}

fn unique_ids(ids: impl IntoIterator<Item = InsightId>) -> Vec<InsightId> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn unique_ranges(ranges: impl IntoIterator<Item = SequenceRange>) -> Vec<SequenceRange> {
    let mut seen = HashSet::new();
    ranges
        .into_iter()
        .filter(|range| seen.insert((range.start, range.end)))
        .collect()
}

fn entry_tokens(entry: &ContextEntry) -> u64 {
    estimate_tokens(&entry.content)
}

#[derive(Debug, Error)]
pub enum CompactionError {
    #[error("semantic compaction configuration is invalid")]
    InvalidConfig,
    #[error("semantic compaction is blocked; first: {first}; second: {second}")]
    ContextCompactionBlocked { first: String, second: String },
    #[error(transparent)]
    Provider(ProviderFault),
    #[error("failed to serialize compaction source: {message}")]
    Serialization { message: String },
    #[error("compaction coverage is invalid: {0}")]
    Coverage(#[from] DomainError),
}
