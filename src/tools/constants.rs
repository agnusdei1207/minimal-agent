pub const MAX_TOOL_RESULT_BYTES: usize = 128 * 1024;
/// Context-friendly bound for a tool result fed to the model. Larger outputs are
/// truncated to a head + tail with an elision marker so one big output cannot
/// bloat the context (INTENT-0002 §3.15). Well below `MAX_TOOL_RESULT_BYTES`.
pub const MAX_TOOL_CONTEXT_BYTES: usize = 16 * 1024;
pub const MAX_TOOL_ARGUMENT_BYTES: usize = MAX_TOOL_RESULT_BYTES;
pub const MAX_BASH_STREAM_BYTES: usize = MAX_TOOL_RESULT_BYTES;
pub const MAX_WORKSPACE_READ_BYTES: u64 = MAX_TOOL_RESULT_BYTES as u64;
pub const MAX_WORKSPACE_LIST_ENTRIES: usize = 10_000;
pub const MAX_JOURNAL_REPLAY_BYTES: usize = MAX_TOOL_RESULT_BYTES;

pub const DEFAULT_BASH_TIMEOUT_SECS: u64 = 300;
pub const MAX_BASH_TIMEOUT_SECS: u64 = 3600;
pub const DEFAULT_WAIT_TIMEOUT_MS: u64 = 60_000;
pub const MAX_JOURNAL_REPLAY_EVENT_LIMIT: u64 = 1_000;
pub const READ_BUFFER_SIZE: usize = 8 * 1024;
pub const ELISION_MARKER: &str = "\n... [truncated] ...\n";
