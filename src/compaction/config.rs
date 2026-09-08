use super::constants::{
    DEFAULT_OVERLAP_TOKENS, DEFAULT_PARTITION_TOKENS, DEFAULT_RETRY_PARTITION_TOKENS,
};
use super::error::CompactionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticCompactionConfig {
    pub partition_tokens: u64,
    pub retry_partition_tokens: u64,
    pub overlap_tokens: u64,
}

impl Default for SemanticCompactionConfig {
    fn default() -> Self {
        Self {
            partition_tokens: DEFAULT_PARTITION_TOKENS,
            retry_partition_tokens: DEFAULT_RETRY_PARTITION_TOKENS,
            overlap_tokens: DEFAULT_OVERLAP_TOKENS,
        }
    }
}

impl SemanticCompactionConfig {
    pub fn validate(self) -> Result<Self, CompactionError> {
        let valid = self.partition_tokens > 0
            && self.retry_partition_tokens > 0
            && self.retry_partition_tokens <= self.partition_tokens
            && self.overlap_tokens < self.retry_partition_tokens;
        valid.then_some(self).ok_or(CompactionError::InvalidConfig)
    }
}
