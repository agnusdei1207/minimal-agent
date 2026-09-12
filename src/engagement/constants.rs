/// Each engagement text field shares the goal byte envelope so an injected
/// engagement cannot inflate a prompt past the same bound goals already respect.
pub const MAX_ENGAGEMENT_TEXT_BYTES: usize = 16 * 1_024;
/// Bounded off-limits list; each entry also respects the text envelope.
pub const MAX_OFF_LIMITS: usize = 64;
