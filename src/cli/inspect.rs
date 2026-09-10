use anyhow::Context;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

use pentesting::brief::AgentBriefStore;
use pentesting::coordinator::{AgentCoordinator, AgentSnapshot};
use pentesting::domain::{AgentId, ContextBudget};
use pentesting::journal::{JournalConfig, RunJournal};

pub fn inspect(run_root: &Path, selected_agent: Option<&str>) -> anyhow::Result<()> {
    let journal = Arc::new(
        RunJournal::open(run_root, JournalConfig::default())
            .with_context(|| format!("open run {}", run_root.display()))?,
    );
    let coordinator = AgentCoordinator::recover(journal.clone())?;
    let briefs = AgentBriefStore::new(
        run_root,
        journal,
        ContextBudget::new(128_000, 128_000, 8_192)?,
    );
    let team = coordinator.team()?;
    if let Some(agent) = selected_agent {
        let agent = AgentId::new(agent)?;
        let snapshot = coordinator.inspect(&agent)?;
        let brief = inspect_brief(&briefs, &agent)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"agent":agent_json(&snapshot),"brief":brief}))?
        );
    } else {
        let agents = team
            .iter()
            .map(|agent| -> anyhow::Result<_> {
                let brief = inspect_brief(&briefs, &agent.id)?;
                Ok(json!({"agent":agent_json(agent),"brief":brief}))
            })
            .collect::<Result<Vec<_>, _>>()?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"agents":agents}))?
        );
    }
    Ok(())
}

pub fn inspect_brief(store: &AgentBriefStore, agent: &AgentId) -> anyhow::Result<String> {
    if !store.path(agent).exists() {
        return Ok("<no live brief projection; durable source remains in the journal>".to_owned());
    }
    Ok(store.read(agent)?)
}

pub fn agent_json(agent: &AgentSnapshot) -> serde_json::Value {
    json!({
        "id":agent.id,
        "role":agent.role,
        "task":agent.task,
        "state":agent.state,
        "unread":agent.unread,
        "latest_insight":agent.latest_insight,
        "waiting_on":agent.waiting_on,
    })
}
