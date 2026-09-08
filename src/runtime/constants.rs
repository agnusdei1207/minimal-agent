use std::time::Duration;

pub const ACTIVITY_WAIT: Duration = Duration::from_secs(3600);
pub const MAIN_COMMAND_CAPACITY: usize = 64;
pub const EVENT_CHANNEL_CAPACITY: usize = 1_024;
pub const IDLE_STABLE_SAMPLES: usize = 3;
pub const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(20);
pub const AUTONOMOUS_RETRY_BACKOFF: Duration = Duration::from_secs(2);
pub const SHUTDOWN_JOIN_DEADLINE: Duration = Duration::from_secs(2);

pub const DEFAULT_MAX_PARALLEL_REQUESTS: usize = 4;
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_COMPACTION_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_CONFIGURED_CONTEXT_TOKENS: u64 = 128_000;
pub const DEFAULT_RESERVED_RESPONSE_TOKENS: u64 = 32_768;
