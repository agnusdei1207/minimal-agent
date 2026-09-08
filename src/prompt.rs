//! System-prompt assembly and tool-call summarization. Pure functions over
//! already-gathered team/engagement data, kept out of the runtime so the
//! orchestration loop stays about lifecycle rather than prompt text.

use serde_json::Value;

use crate::coordinator::AgentSnapshot;
use crate::domain::AgentId;
use crate::engagement::{
    Engagement, EngagementKind, authorized_engagement_doctrine, ctf_solve_loop_doctrine,
    execution_style_directive,
};
use crate::tools::names as tool_names;

/// Renders the agent's POSITION block from the live team and reports whether it
/// has children, so the caller can pick the internal- vs leaf-node doctrine
/// (INTENT-0004 §3.2/§3.4).
pub(crate) fn render_position(
    agent_id: &AgentId,
    agent: &AgentSnapshot,
    team: &[AgentSnapshot],
) -> (bool, String) {
    let children: Vec<String> = team
        .iter()
        .filter(|snapshot| snapshot.parent.as_ref() == Some(agent_id))
        .map(|snapshot| snapshot.id.to_string())
        .collect();
    let siblings: Vec<String> = team
        .iter()
        .filter(|snapshot| &snapshot.id != agent_id && snapshot.parent == agent.parent)
        .map(|snapshot| snapshot.id.to_string())
        .collect();
    let has_children = !children.is_empty();
    let node_kind = if has_children {
        "internal (you have children — decompose, delegate, and aggregate faithfully)"
    } else {
        "leaf (no children — execute directly and report exact values to your parent)"
    };
    let parent = agent
        .parent
        .as_ref()
        .map_or_else(|| "none (you are the root)".to_owned(), ToString::to_string);
    let join_ids = |items: Vec<String>| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.join(", ")
        }
    };
    let block = format!(
        "POSITION\ndepth: {}\nnode: {}\nparent: {}\nchildren: {}\nsiblings: {}",
        agent.depth.value(),
        node_kind,
        parent,
        join_ids(children),
        join_ids(siblings),
    );
    (has_children, block)
}

/// Assemble an agent's full system prompt from focused sections joined by a
/// blank line: identity, always-on doctrine, the engagement target block and CTF
/// loop when present, team conduct and communication, the role-specific prompt,
/// then the brief.
pub(crate) fn build_system(
    agent: &AgentSnapshot,
    team: &[AgentSnapshot],
    engagement: Option<&Engagement>,
    brief: &str,
) -> String {
    // PROTECTED anti-refusal contract (INTENT-0002 §3.12). Keep it wired here and do
    // not weaken it when editing other prompts.
    const EXECUTION_MANDATE: &str = include_str!("../prompts/execution-mandate.md");
    const TRADECRAFT: &str = include_str!("../prompts/tradecraft.md");
    const TEAM_CONDUCT: &str = include_str!("../prompts/team-conduct.md");
    const COMMUNICATION: &str = include_str!("../prompts/communication.md");
    const MAIN_ROLE: &str = include_str!("../prompts/main-role.md");
    const WORKER_ROLE: &str = include_str!("../prompts/worker-role.md");
    const FAN_OUT: &str = include_str!("../prompts/fan-out.md");
    const SELF_MANAGEMENT: &str = include_str!("../prompts/self-management.md");
    const TEAM_TREE: &str = include_str!("../prompts/team-tree.md");
    const NODE_INTERNAL: &str = include_str!("../prompts/node-internal.md");
    const NODE_LEAF: &str = include_str!("../prompts/node-leaf.md");

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

/// Short, bounded summary of a tool call's target for transcript rendering.
/// Never includes secret material and is capped so a large argument cannot
/// inflate a transcript line.
pub(crate) fn tool_call_summary(name: &str, arguments: &Value) -> Option<String> {
    const MAX_SUMMARY_CHARS: usize = 160;
    let raw = match name {
        tool_names::BASH => arguments.get("command")?.as_str()?.to_owned(),
        tool_names::TMUX => arguments.get("args")?.as_str()?.to_owned(),
        tool_names::WORKSPACE => {
            let op = arguments.get("op").and_then(Value::as_str)?;
            let path = arguments.get("path").and_then(Value::as_str)?;
            format!("{op} {path}")
        }
        tool_names::JOURNAL => {
            let start = arguments.get("start").and_then(Value::as_i64)?;
            let end = arguments.get("end").and_then(Value::as_i64)?;
            format!("{start}..{end}")
        }
        tool_names::TEAM | tool_names::REPORT => {
            arguments.get("op").and_then(Value::as_str)?.to_owned()
        }
        _ => return None,
    };
    let summary = raw.trim();
    if summary.is_empty() {
        return None;
    }
    let condensed = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(truncate_chars(&condensed, MAX_SUMMARY_CHARS))
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let truncated = value
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}
