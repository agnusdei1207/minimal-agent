use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::constants::{
    BRIEF_TARGET_PERCENT, COMPACTION_TARGET_PERCENT, COMPACTION_TRIGGER_PERCENT, MAX_GOAL_BYTES,
    MAX_REASON_BYTES, MAX_ROLE_BYTES, MAX_TASK_BYTES, MAX_USER_INPUT_BYTES, MESSAGE_MAX_TOKENS,
    MESSAGE_TARGET_PERCENT,
};
use super::error::DomainError;
use super::ids::InsightId;

pub fn validate_role(value: &str) -> Result<(), DomainError> {
    validate_bounded_text("role", value, MAX_ROLE_BYTES, false)
}

pub fn validate_task(value: &str) -> Result<(), DomainError> {
    validate_bounded_text("task", value, MAX_TASK_BYTES, false)
}

pub fn validate_goal(value: &str, allow_empty: bool) -> Result<(), DomainError> {
    validate_bounded_text("goal", value, MAX_GOAL_BYTES, allow_empty)
}

pub fn validate_user_input(value: &str) -> Result<(), DomainError> {
    validate_bounded_text("user input", value, MAX_USER_INPUT_BYTES, false)
}

pub fn validate_reason(value: &str) -> Result<(), DomainError> {
    validate_bounded_text("reason", value, MAX_REASON_BYTES, false)
}

/// Conservative tokenizer-independent estimate. ASCII is commonly packed into
/// subword tokens; non-ASCII is charged by UTF-8 byte so multilingual input
/// cannot postpone compaction by being undercounted as one quarter-token.
pub fn estimate_tokens(text: &str) -> u64 {
    let (ascii, non_ascii_bytes) = text.chars().fold((0_u64, 0_u64), |counts, character| {
        if character.is_ascii() {
            (counts.0.saturating_add(1), counts.1)
        } else {
            (
                counts.0,
                counts.1.saturating_add(character.len_utf8() as u64),
            )
        }
    });
    ascii.div_ceil(4).saturating_add(non_ascii_bytes)
}

pub fn validate_bounded_text(
    field: &'static str,
    value: &str,
    max: usize,
    allow_empty: bool,
) -> Result<(), DomainError> {
    if !allow_empty && value.trim().is_empty() {
        return Err(DomainError::EmptyText { field });
    }
    if value.len() > max {
        return Err(DomainError::TextTooLarge {
            field,
            actual: value.len(),
            max,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SequenceRange {
    pub start: u64,
    pub end: u64,
}

impl SequenceRange {
    pub fn new(start: u64, end: u64) -> Result<Self, DomainError> {
        (start > 0 && start <= end)
            .then_some(Self { start, end })
            .ok_or(DomainError::InvalidRange { start, end })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionCoverage {
    pub covered_ranges: Vec<SequenceRange>,
    pub covered_insight_ids: Vec<InsightId>,
    pub superseded_insight_ids: Vec<InsightId>,
}

impl CompactionCoverage {
    pub fn validate(
        &self,
        required_ranges: &[SequenceRange],
        required_insights: &[InsightId],
    ) -> Result<(), DomainError> {
        let ranges_complete = required_ranges
            .iter()
            .all(|required| range_is_covered(*required, &self.covered_ranges));
        let accounted: HashSet<_> = self
            .covered_insight_ids
            .iter()
            .chain(self.superseded_insight_ids.iter())
            .collect();
        let required_insight_set: HashSet<_> = required_insights.iter().collect();
        let insights_complete = required_insights.iter().all(|id| accounted.contains(id));
        let ranges_exact = self
            .covered_ranges
            .iter()
            .all(|covered| range_is_covered(*covered, required_ranges));
        let insights_exact = accounted
            .iter()
            .all(|id| required_insight_set.contains(*id));
        (ranges_complete && insights_complete && ranges_exact && insights_exact)
            .then_some(())
            .ok_or(DomainError::IncompleteCoverage)
    }
}

pub fn range_is_covered(required: SequenceRange, covered: &[SequenceRange]) -> bool {
    let mut ranges = covered.to_vec();
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut next = required.start;
    for range in ranges {
        if range.end < next || range.start > next {
            continue;
        }
        next = range.end.saturating_add(1);
        if next > required.end {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBudget {
    effective_tokens: u64,
}

impl ContextBudget {
    pub fn new(
        configured_max: u64,
        provider_max: u64,
        reserved_response: u64,
    ) -> Result<Self, DomainError> {
        let total = configured_max.min(provider_max);
        let effective_tokens = total
            .checked_sub(reserved_response)
            .filter(|value| *value > 0)
            .ok_or(DomainError::InvalidContextBudget)?;
        Ok(Self { effective_tokens })
    }

    pub fn effective_tokens(self) -> u64 {
        self.effective_tokens
    }

    pub fn needs_compaction(self, projected_input: u64) -> bool {
        u128::from(projected_input) * 100
            >= u128::from(self.effective_tokens) * u128::from(COMPACTION_TRIGGER_PERCENT)
    }

    pub fn target_input_tokens(self) -> u64 {
        self.effective_tokens * COMPACTION_TARGET_PERCENT / 100
    }

    pub fn brief_target_tokens(self) -> u64 {
        self.effective_tokens * BRIEF_TARGET_PERCENT / 100
    }

    pub fn message_target_tokens(self) -> u64 {
        (self.effective_tokens * MESSAGE_TARGET_PERCENT / 100).min(MESSAGE_MAX_TOKENS)
    }
}
