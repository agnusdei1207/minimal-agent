pub const MAX_SUMMARY_CHARS: usize = 160;

// PROTECTED anti-refusal contract (INTENT-0002 §3.12). Keep it wired here and do
// not weaken it when editing other prompts.
pub const EXECUTION_MANDATE: &str = include_str!("../../prompts/execution-mandate.md");
pub const TRADECRAFT: &str = include_str!("../../prompts/tradecraft.md");
pub const TEAM_CONDUCT: &str = include_str!("../../prompts/team-conduct.md");
pub const COMMUNICATION: &str = include_str!("../../prompts/communication.md");
pub const MAIN_ROLE: &str = include_str!("../../prompts/main-role.md");
pub const WORKER_ROLE: &str = include_str!("../../prompts/worker-role.md");
pub const FAN_OUT: &str = include_str!("../../prompts/fan-out.md");
pub const SELF_MANAGEMENT: &str = include_str!("../../prompts/self-management.md");
pub const TEAM_TREE: &str = include_str!("../../prompts/team-tree.md");
pub const NODE_INTERNAL: &str = include_str!("../../prompts/node-internal.md");
pub const NODE_LEAF: &str = include_str!("../../prompts/node-leaf.md");

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctf_solve_loop_and_doctrines_are_embedded_and_consistent() {
        assert!(!authorized_engagement_doctrine().is_empty());
        assert!(!execution_style_directive().is_empty());
        let ctf = ctf_solve_loop_doctrine();
        assert!(!ctf.is_empty());
        assert!(ctf.contains("Diagnostic Tracer Bullets"));
        assert!(ctf.contains("Silent Wall vs. Live Seam"));
        assert!(ctf.contains("Self-Reflection & Meta-Cognitive Audit"));
        assert!(ctf.contains("Universal Client Compatibility"));
    }
}
