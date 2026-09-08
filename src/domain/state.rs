use serde::{Deserialize, Serialize};

use super::constants::{MAX_DEPTH, MAX_TEAM_SIZE};
use super::error::DomainError;

/// Position of an agent in the bounded team tree: 0 = main, 1 = child,
/// 2 = grandchild (a leaf). See INTENT-0004. Depths above `MAX_DEPTH` are rejected.
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
