use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use super::constants::{MAX_MESSAGE_BYTES, MAX_TEAM_SIZE};
use super::error::DomainError;
use super::ids::{AgentId, InsightId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Progress,
    Insight,
    Request,
    Final,
}

impl MessageKind {
    /// Parse a message kind (case-folded; callers trim if they need to). Single
    /// source of truth for the `team send` tool and the runtime event mapper.
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "progress" => Some(Self::Progress),
            "insight" => Some(Self::Insight),
            "request" => Some(Self::Request),
            "final" => Some(Self::Final),
            _ => None,
        }
    }
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

impl InsightLabel {
    /// Canonical `SCREAMING_SNAKE_CASE` label token. Single source of truth for
    /// the `team send` tool and the brief-knowledge validator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fact => "FACT",
            Self::Hypothesis => "HYPOTHESIS",
            Self::Direction => "DIRECTION",
            Self::Success => "SUCCESS",
            Self::DeadEnd => "DEAD_END",
            Self::Blocker => "BLOCKER",
        }
    }

    /// Parse a label token (case-folded; callers trim if they need to).
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_uppercase().as_str() {
            "FACT" => Some(Self::Fact),
            "HYPOTHESIS" => Some(Self::Hypothesis),
            "DIRECTION" => Some(Self::Direction),
            "SUCCESS" => Some(Self::Success),
            "DEAD_END" => Some(Self::DeadEnd),
            "BLOCKER" => Some(Self::Blocker),
            _ => None,
        }
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
