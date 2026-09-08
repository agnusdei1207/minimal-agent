use thiserror::Error;

#[derive(Debug, Error)]
pub enum JournalError {
    #[error("another writer already owns this run journal")]
    WriterActive,
    #[error("journal is not empty")]
    ExpectedEmpty,
    #[error("run storage cap reached")]
    StorageFull,
    #[error("existing run storage is {actual} bytes; configured maximum is {max}")]
    ExistingStorageTooLarge { actual: u64, max: u64 },
    #[error("journal configuration is invalid")]
    InvalidConfig,
    #[error("journal writer lock was poisoned")]
    LockPoisoned,
    #[error("reserved fault space is too small")]
    FaultReserveTooSmall,
    #[error("journal is corrupt at {segment}, line {line}")]
    MiddleCorruption { segment: String, line: usize },
    #[error("journal sequence gap: expected {expected}, found {actual}")]
    SequenceGap { expected: u64, actual: u64 },
    #[error("journal record {sequence} checksum mismatch")]
    ChecksumMismatch { sequence: u64 },
    #[error("blob digest mismatch: expected {expected}, found {actual}")]
    BlobDigestMismatch { expected: String, actual: String },
    #[error("blob length mismatch: expected {expected}, found {actual}")]
    BlobLengthMismatch { expected: u64, actual: u64 },
    #[error("invalid journal segment: {0}")]
    InvalidSegment(String),
    #[error("journal segments are not contiguous")]
    InvalidSegmentSequence,
    #[error("journal replay range is invalid")]
    InvalidReplayRange,
    #[error("journal replay exceeds the {max_bytes}-byte response limit")]
    ReplayLimit { max_bytes: usize },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
