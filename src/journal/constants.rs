pub const DEFAULT_SEGMENT_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_INLINE_PAYLOAD_BYTES: usize = 16 * 1024;
pub const DEFAULT_MAX_RUN_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_RESERVED_FAULT_BYTES: u64 = 4 * 1024;

pub const JOURNAL_DIR_NAME: &str = "journal";
pub const BLOBS_DIR_NAME: &str = "blobs";
pub const WRITER_LOCK_NAME: &str = "writer.lock";
pub const SEGMENT_EXTENSION: &str = "jsonl";
pub const FAULT_CODE_STORAGE_CAP: &str = "storage_cap";
pub const FAULT_MSG_STORAGE_CAP: &str =
    "run storage cap reached; journal stopped without deleting source";
