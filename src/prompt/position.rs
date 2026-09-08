use crate::coordinator::AgentSnapshot;
use crate::domain::AgentId;

/// Renders the agent's POSITION block from the live team and reports whether it
/// has children, so the caller can pick the internal- vs leaf-node doctrine
/// (INTENT-0004 §3.2/§3.4).
pub fn render_position(
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
