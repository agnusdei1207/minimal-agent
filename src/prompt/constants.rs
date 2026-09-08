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
