pub const MAIN_AGENT_ID: &str = "main";
pub const MAX_TEAM_SIZE: usize = 10;
/// Maximum agent depth in the bounded team tree: 0 = main, 1 = child,
/// 2 = grandchild (leaf). No agent may be created deeper than this (INTENT-0004).
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
