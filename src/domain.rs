use std::collections::HashSet;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAIN_AGENT_ID: &str = "main";
pub const MAX_TEAM_SIZE: usize = 10;
/// Maximum agent depth in the bounded team tree: 0 = main, 1 = child,
/// 2 = grandchild (leaf). No agent may be created deeper than this (ADR-0004).
pub const MAX_DEPTH: u8 = 2;
pub const COMPACTION_TRIGGER_PERCENT: u64 = 80;
pub const COMPACTION_TARGET_PERCENT: u64 = 50;
pub const BRIEF_TARGET_PERCENT: u64 = 20;
pub const MESSAGE_TARGET_PERCENT: u64 = 2;
pub const MESSAGE_MAX_TOKENS: u64 = 1_024;
pub const MAX_MESSAGE_BYTES: usize = 4 * 1_024;
pub const MAX_INBOX_MESSAGES: usize = 64;
pub const MAX_INBOX_BYTES: usize = 64 * 1_024;
pub const MAX_ROLE_BYTES: usize = 256;
pub const MAX_TASK_BYTES: usize = 4 * 1_024;
pub const MAX_GOAL_BYTES: usize = 16 * 1_024;
pub const MAX_USER_INPUT_BYTES: usize = 64 * 1_024;
pub const MAX_REASON_BYTES: usize = 4 * 1_024;

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

fn validate_bounded_text(
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
        valid
            .then_some(Self(value.clone()))
            .ok_or(DomainError::InvalidIdentifier(value))
    }

    pub fn main() -> Self {
        Self(MAIN_AGENT_ID.to_owned())
    }

    pub fn is_main(&self) -> bool {
        self.0 == MAIN_AGENT_ID
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for AgentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Position of an agent in the bounded team tree: 0 = main, 1 = child,
/// 2 = grandchild (a leaf). See ADR-0004. Depths above `MAX_DEPTH` are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentDepth(u8);

impl AgentDepth {
    /// The root of the tree (main).
    pub const MAIN: Self = Self(0);

    pub fn value(self) -> u8 {
        self.0
    }

    /// Only the root has no parent.
    pub fn is_main(self) -> bool {
        self.0 == 0
    }

    /// A node may create children only while below the maximum depth; a
    /// grandchild (depth 2) is a leaf and cannot spawn.
    pub fn can_spawn(self) -> bool {
        self.0 < MAX_DEPTH
    }

    /// The depth of a child of this node, or an error at the leaf.
    pub fn child(self) -> Result<Self, DomainError> {
        if self.can_spawn() {
            Ok(Self(self.0 + 1))
        } else {
            Err(DomainError::MaxDepthReached)
        }
    }
}

impl TryFrom<u8> for AgentDepth {
    type Error = DomainError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value <= MAX_DEPTH {
            Ok(Self(value))
        } else {
            Err(DomainError::InvalidDepth(value))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Running,
    Waiting,
    Recalling,
    Finished,
    Stopped,
    Faulted,
}

impl AgentState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Finished | Self::Stopped | Self::Faulted)
    }

    pub fn transition_to(self, next: Self) -> Result<(), DomainError> {
        let valid = self == next
            || matches!(
                (self, next),
                (
                    Self::Running,
                    Self::Waiting
                        | Self::Recalling
                        | Self::Finished
                        | Self::Stopped
                        | Self::Faulted
                ) | (
                    Self::Waiting,
                    Self::Running
                        | Self::Recalling
                        | Self::Finished
                        | Self::Stopped
                        | Self::Faulted
                ) | (
                    Self::Recalling,
                    Self::Finished | Self::Stopped | Self::Faulted
                )
            );
        valid
            .then_some(())
            .ok_or(DomainError::InvalidStateTransition {
                from: self,
                to: next,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamLimits {
    max_team_size: usize,
}

impl Default for TeamLimits {
    fn default() -> Self {
        Self {
            max_team_size: MAX_TEAM_SIZE,
        }
    }
}

impl TeamLimits {
    pub fn max_team_size(self) -> usize {
        self.max_team_size
    }

    pub fn max_workers(self) -> usize {
        self.max_team_size - 1
    }

    pub fn validate_total(self, total: usize) -> Result<(), DomainError> {
        (total <= self.max_team_size)
            .then_some(())
            .ok_or(DomainError::TeamFull)
    }

    pub fn validate_spawn(self, caller: AgentDepth) -> Result<(), DomainError> {
        if caller.can_spawn() {
            Ok(())
        } else {
            Err(DomainError::MaxDepthReached)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Progress,
    Insight,
    Request,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InsightLabel {
    Fact,
    Hypothesis,
    Direction,
    Success,
    DeadEnd,
    Blocker,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct InsightId(String);

impl InsightId {
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let value = value.into();
        let valid = !value.trim().is_empty() && value.len() <= 128;
        valid
            .then_some(Self(value.clone()))
            .ok_or(DomainError::InvalidIdentifier(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for InsightId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Insight {
    pub id: InsightId,
    pub label: InsightLabel,
    pub text: String,
}

impl Insight {
    pub fn new(id: InsightId, label: InsightLabel, text: impl Into<String>) -> Self {
        Self {
            id,
            label,
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: Uuid,
    pub sender: AgentId,
    pub audience: Vec<AgentId>,
    pub kind: MessageKind,
    pub body: String,
    pub insight: Option<Insight>,
}

impl AgentMessage {
    pub fn new(
        sender: AgentId,
        mut audience: Vec<AgentId>,
        kind: MessageKind,
        body: impl Into<String>,
        insight: Option<Insight>,
    ) -> Result<Self, DomainError> {
        let body = body.into();
        // Neighbor routing (parent/children/siblings) is enforced by the
        // coordinator, which holds the tree; the domain keeps only shape limits
        // (non-empty, deduplicated, bounded). See ADR-0004 §3.4.
        let mut seen = HashSet::new();
        audience.retain(|recipient| seen.insert(recipient.clone()));
        let message = Self {
            id: Uuid::new_v4(),
            sender,
            audience,
            kind,
            body,
            insight,
        };
        message.validate()?;
        Ok(message)
    }

    pub fn payload_bytes(&self) -> usize {
        self.body.len()
            + self
                .insight
                .as_ref()
                .map_or(0, |insight| insight.text.len())
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if self.body.trim().is_empty() {
            return Err(DomainError::EmptyMessage);
        }
        let payload_bytes = self.payload_bytes();
        if payload_bytes > MAX_MESSAGE_BYTES {
            return Err(DomainError::MessageTooLarge {
                actual: payload_bytes,
                max: MAX_MESSAGE_BYTES,
            });
        }
        if self.kind == MessageKind::Insight && self.insight.is_none() {
            return Err(DomainError::MissingInsight);
        }
        if self.audience.len() > MAX_TEAM_SIZE {
            return Err(DomainError::AudienceTooLarge { max: MAX_TEAM_SIZE });
        }
        if self.audience.is_empty() {
            return Err(DomainError::EmptyAudience);
        }
        let unique = self.audience.iter().collect::<HashSet<_>>();
        if unique.len() != self.audience.len() {
            return Err(DomainError::DuplicateAudience);
        }
        Ok(())
    }
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

fn range_is_covered(required: SequenceRange, covered: &[SequenceRange]) -> bool {
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

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("agent depth {0} exceeds the maximum team-tree depth")]
    InvalidDepth(u8),
    #[error("agent identifier is invalid: {0}")]
    InvalidIdentifier(String),
    #[error("the active team already has ten agents")]
    TeamFull,
    #[error("a leaf agent at the maximum depth cannot spawn children")]
    MaxDepthReached,
    #[error("invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: AgentState, to: AgentState },
    #[error("message body cannot be empty")]
    EmptyMessage,
    #[error("message audience cannot be empty")]
    EmptyAudience,
    #[error("message recipients must be the sender's parent, children, or siblings")]
    NonNeighborAudience,
    #[error("message audience cannot contain duplicates")]
    DuplicateAudience,
    #[error("message audience exceeds the team limit of {max}")]
    AudienceTooLarge { max: usize },
    #[error("message payload is {actual} bytes; maximum is {max}")]
    MessageTooLarge { actual: usize, max: usize },
    #[error("{field} cannot be empty")]
    EmptyText { field: &'static str },
    #[error("{field} is {actual} bytes; maximum is {max}")]
    TextTooLarge {
        field: &'static str,
        actual: usize,
        max: usize,
    },
    #[error("insight messages require a typed insight")]
    MissingInsight,
    #[error("invalid sequence range {start}..={end}")]
    InvalidRange { start: u64, end: u64 },
    #[error("compaction coverage omits source data")]
    IncompleteCoverage,
    #[error("context budget must leave room after the response reserve")]
    InvalidContextBudget,
}
