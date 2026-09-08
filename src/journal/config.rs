use super::constants::*;
use super::error::JournalError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalConfig {
    pub segment_bytes: u64,
    pub inline_payload_bytes: usize,
    pub max_run_bytes: u64,
    pub reserved_fault_bytes: u64,
}

impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            segment_bytes: DEFAULT_SEGMENT_BYTES,
            inline_payload_bytes: DEFAULT_INLINE_PAYLOAD_BYTES,
            max_run_bytes: DEFAULT_MAX_RUN_BYTES,
            reserved_fault_bytes: DEFAULT_RESERVED_FAULT_BYTES,
        }
    }
}

impl JournalConfig {
    pub fn validate(self) -> Result<Self, JournalError> {
        let valid = self.segment_bytes > 0
            && self.inline_payload_bytes > 0
            && self.reserved_fault_bytes > 0
            && self.max_run_bytes > self.reserved_fault_bytes;
        valid.then_some(self).ok_or(JournalError::InvalidConfig)
    }
}
