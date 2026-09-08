use crate::coordinator::AgentSnapshot;
use crate::domain::AgentState;

use super::constants::{RUNTIME_END, RUNTIME_START};
use super::error::BriefError;
use super::validate::validate_runtime_block;

pub fn render_main_template(team: &[AgentSnapshot]) -> String {
    // Seed the battlefield root with the actual objective so the note is
    // meaningful from turn zero, even before the model writes to it. An empty
    // placeholder after real work reads as broken; the goal read as the root of
    // the arc tree does not.
    let goal_root = team
        .iter()
        .find(|agent| agent.id.is_main())
        .map(|agent| agent.task.trim())
        .filter(|task| !task.is_empty())
        .unwrap_or("Goal");
    format!(
        "# Main Agent Brief\n{}\n\
## Goal & Constraints\nNo goal refinement yet\n\
## Battlefield\n{goal_root}\n└─ No active arc yet\n\
## Curated Knowledge\n\
### Facts & Successes\nNo durable insight yet\n\
### Hypotheses & Directions\nNo durable insight yet\n\
### Dead Ends\nNo durable insight yet\n\
## Blockers\nNone\n\
## Next Moves\nAssign the first useful task\n",
        render_runtime_block(team)
    )
}

pub fn render_worker_template(agent: &AgentSnapshot) -> String {
    format!(
        "# Worker Agent Brief\n\
## Assignment\n{}\n\
## Current State\n{}\n\
## Attempts by Domain\nNo attempts yet\n\
## Curated Knowledge\n\
### Facts & Successes\nNo durable insight yet\n\
### Hypotheses & Directions\nNo durable insight yet\n\
### Dead Ends\nNo durable insight yet\n\
## Integrated Messages\nNone\n\
## Blockers\nNone\n\
## Next Move\nStart the assigned task\n",
        agent.task,
        state_name(agent.state)
    )
}

pub fn render_runtime_block(team: &[AgentSnapshot]) -> String {
    let mut team = team.to_vec();
    team.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    let mut output = String::from(RUNTIME_START);
    output.push_str(
        "\n## Team\n| ID | Role | Current Task | State | Latest Insight | Waiting On |\n",
    );
    output.push_str("|---|---|---|---|---|---|\n");
    for agent in team {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            agent.id,
            table_cell(&agent.role),
            table_cell(&agent.task),
            state_name(agent.state),
            table_cell(agent.latest_insight.as_deref().unwrap_or("")),
            table_cell(agent.waiting_on.as_deref().unwrap_or("")),
        ));
    }
    output.push_str(RUNTIME_END);
    output
}

pub fn replace_runtime_block(current: &str, runtime: &str) -> Result<String, BriefError> {
    validate_runtime_block(current)?;
    let start = current
        .find(RUNTIME_START)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    let relative_end = current[start..]
        .find(RUNTIME_END)
        .ok_or(BriefError::MissingRuntimeBlock)?;
    let end = start + relative_end + RUNTIME_END.len();
    let mut output = String::with_capacity(current.len() + runtime.len());
    output.push_str(&current[..start]);
    output.push_str(runtime);
    output.push_str(&current[end..]);
    Ok(output)
}

pub fn state_name(state: AgentState) -> &'static str {
    match state {
        AgentState::Running => "RUNNING",
        AgentState::Waiting => "WAITING",
        AgentState::Recalling => "RECALLING",
        AgentState::Finished => "FINISHED",
        AgentState::Stopped => "STOPPED",
        AgentState::Faulted => "FAULTED",
    }
}

pub fn table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_owned()
}
