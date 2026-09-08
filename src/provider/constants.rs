use std::time::Duration;

pub const MAX_PROVIDER_ATTEMPTS: usize = 3;
pub const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(250);
pub const MAX_RETRY_DELAY: Duration = Duration::from_secs(5);
pub const MAX_PROVIDER_ERROR_BYTES: usize = 16 * 1024;
pub const MAX_PROVIDER_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MIN_PROVIDER_STREAM_BYTES: usize = 64 * 1024;
pub const MAX_PROVIDER_STREAM_BYTES: usize = 32 * 1024 * 1024;
pub const STREAM_OVERHEAD_MULTIPLIER: usize = 64;

pub const CHAT_COMPLETIONS_ENDPOINT_PATH: &str = "/chat/completions";
pub const STREAM_DONE_SENTINEL: &str = "[DONE]";
pub const TOOL_CHOICE_AUTO: &str = "auto";
pub const FUNCTION_TYPE: &str = "function";

pub const LEAKED_CONTROL_TOKENS: &[&str] = &[
    "<｜DSML｜tool_calls",
    "<｜DSML｜",
    "｜call>",
    "<｜tool_calls｜",
    "<｜tool call begin｜",
    "<｜tool call end｜",
    "<｜begin of sentence｜>",
    "<｜end of sentence｜>",
    "<|im_start|>",
    "<|im_end|>",
];
