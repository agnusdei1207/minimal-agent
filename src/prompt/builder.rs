use crate::coordinator::AgentSnapshot;
use crate::engagement::{
    Engagement, EngagementKind, authorized_engagement_doctrine, ctf_solve_loop_doctrine,
    execution_style_directive,
};

use super::constants::{
    COMMUNICATION, EXECUTION_MANDATE, FAN_OUT, MAIN_ROLE, NODE_INTERNAL, NODE_LEAF,
    SELF_MANAGEMENT, TEAM_CONDUCT, TEAM_TREE, TRADECRAFT, WORKER_ROLE,
};
use super::position::render_position;

/// Assemble an agent's full system prompt from focused sections joined by a
/// blank line: identity, always-on doctrine, the engagement target block and CTF
/// loop when present, team conduct and communication, the role-specific prompt,
/// then the brief.
pub fn build_system(
    agent: &AgentSnapshot,
    team: &[AgentSnapshot],
    engagement: Option<&Engagement>,
    brief: &str,
) -> String {
    let agent_id = &agent.id;
    // Position in the team tree drives the role prompt (INTENT-0004 §3.2/§3.4).
    let (has_children, position_block) = render_position(agent_id, agent, team);

    let mut sections = vec![
        format!(
            "agent_id: {}\nrole: {}\nassignment: {}",
            agent.id, agent.role, agent.task
        ),
        authorized_engagement_doctrine().to_owned(),
        EXECUTION_MANDATE.trim_end().to_owned(),
        execution_style_directive().to_owned(),
        TRADECRAFT.trim_end().to_owned(),
    ];
    if let Some(engagement) = engagement {
        sections.push(engagement.render_context().trim_end().to_owned());
        if engagement.kind == EngagementKind::Ctf {
            sections.push(ctf_solve_loop_doctrine().to_owned());
        }
    }
    sections.push(TEAM_CONDUCT.trim_end().to_owned());
    sections.push(COMMUNICATION.trim_end().to_owned());
    sections.push(TEAM_TREE.trim_end().to_owned());
    if agent_id.is_main() {
        sections.push(MAIN_ROLE.trim_end().to_owned());
        sections.push(FAN_OUT.trim_end().to_owned());
    } else {
        sections.push(WORKER_ROLE.trim_end().to_owned());
        // A non-main node behaves as an internal node while it has children,
        // otherwise as a leaf (INTENT-0004 §3.2).
        sections.push(
            if has_children {
                NODE_INTERNAL
            } else {
                NODE_LEAF
            }
            .trim_end()
            .to_owned(),
        );
    }
    // Every node maintains its own battlefield note via the `brief` tool
    // (INTENT-0001 §9.1), so the self-management doctrine is always on.
    sections.push(SELF_MANAGEMENT.trim_end().to_owned());
    sections.push(position_block);
    sections.push(format!("CURRENT BRIEF\n{brief}"));
    sections.join("\n\n")
}
