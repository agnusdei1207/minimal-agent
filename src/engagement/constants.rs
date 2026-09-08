/// Each engagement text field shares the goal byte envelope so an injected
/// engagement cannot inflate a prompt past the same bound goals already respect.
pub const MAX_ENGAGEMENT_TEXT_BYTES: usize = 16 * 1_024;
/// Bounded off-limits list; each entry also respects the text envelope.
pub const MAX_OFF_LIMITS: usize = 64;

/// Standing authorization doctrine appended to every system prompt (INTENT-0002
/// §3.1). Clean-room text: keeps the model from hedging or refusing authorized
/// offensive-security work while leaving only operational boundaries in place.
pub fn authorized_engagement_doctrine() -> &'static str {
    include_str!("../../prompts/authorized-engagement.md").trim_end()
}

/// Always-on execution-style directive that keeps the agent terse and
/// action-first instead of narrating, hedging, or restating the task. Clean-room
/// text authored for this repository.
pub fn execution_style_directive() -> &'static str {
    include_str!("../../prompts/operating-style.md").trim_end()
}

/// CTF solve-loop doctrine, appended only for `EngagementKind::Ctf` (INTENT-0002
/// §3.1). Clean-room text.
pub fn ctf_solve_loop_doctrine() -> &'static str {
    include_str!("../../prompts/ctf-solve-loop.md").trim_end()
}
