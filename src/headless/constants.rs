use std::time::Duration;

pub const HEADLESS_MAX_WALL: Duration = Duration::from_secs(1800);
/// Idle-settle window before a headless run re-nudges, with and without auto.
pub const HEADLESS_IDLE_AUTO: Duration = Duration::from_secs(20);
pub const HEADLESS_IDLE_MANUAL: Duration = Duration::from_secs(2);
/// Max autonomous re-nudges of a waiting main agent in a headless run.
pub const MAX_HEADLESS_RETRIES: usize = 3;
/// Backoff before re-nudging a waiting main agent in a headless run.
pub const HEADLESS_RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// Bytes of a tool result / assistant snippet shown on one debug log line.
pub const LOG_SNIPPET_BYTES: usize = 400;
/// Bytes of a team-message body shown on one debug log line.
pub const LOG_MESSAGE_SNIPPET_BYTES: usize = 200;
/// Maximum bytes scanned for tool results when discovering evidence.
pub const MAX_TOOL_EVENT_BYTES: usize = 8 * 1024 * 1024;
