use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use tokio::sync::{Notify, OwnedSemaphorePermit};
use tokio_util::sync::CancellationToken;

use crate::domain::{
    AgentDepth, AgentId, AgentMessage, AgentState, MAX_INBOX_BYTES, MAX_INBOX_MESSAGES, TeamLimits,
    validate_goal, validate_reason, validate_role, validate_task,
};
use crate::journal::{JournalEvent, ReplayedEvent};

use super::constants::WORKER_ID_PREFIX;
use super::error::CoordinatorError;
use super::snapshot::{AgentSnapshot, MessageDelivery};

pub struct AgentRecord {
    pub id: AgentId,
    pub depth: AgentDepth,
    pub parent: Option<AgentId>,
    pub role: String,
    pub task: String,
    pub state: AgentState,
    pub inbox: VecDeque<MessageDelivery>,
    pub inbox_bytes: usize,
    pub latest_insight: Option<String>,
    pub waiting_on: Option<String>,
    pub activity_revision: u64,
    pub wake: Arc<Notify>,
    pub cancellation: CancellationToken,
    pub worker_permit: Option<OwnedSemaphorePermit>,
}

impl AgentRecord {
    pub fn snapshot(&self) -> AgentSnapshot {
        AgentSnapshot {
            id: self.id.clone(),
            depth: self.depth,
            parent: self.parent.clone(),
            role: self.role.clone(),
            task: self.task.clone(),
            state: self.state,
            unread: self.inbox.len(),
            latest_insight: self.latest_insight.clone(),
            waiting_on: self.waiting_on.clone(),
        }
    }
}

#[derive(Default)]
pub struct TeamState {
    pub agents: HashMap<AgentId, AgentRecord>,
}

pub fn new_record(
    id: AgentId,
    depth: AgentDepth,
    parent: Option<AgentId>,
    role: String,
    task: String,
    worker_permit: Option<OwnedSemaphorePermit>,
) -> AgentRecord {
    AgentRecord {
        id,
        depth,
        parent,
        role,
        task,
        state: AgentState::Running,
        inbox: VecDeque::new(),
        inbox_bytes: 0,
        latest_insight: None,
        waiting_on: None,
        activity_revision: 0,
        wake: Arc::new(Notify::new()),
        cancellation: CancellationToken::new(),
        worker_permit,
    }
}

pub fn collect_descendants(state: &TeamState, root: &AgentId) -> Vec<AgentId> {
    let mut result = Vec::new();
    let mut frontier = vec![root.clone()];
    while let Some(current) = frontier.pop() {
        for (id, agent) in &state.agents {
            if agent.parent.as_ref() == Some(&current) {
                result.push(id.clone());
                frontier.push(id.clone());
            }
        }
    }
    result
}

pub fn signal_main_activity(state: &mut TeamState) -> Option<Arc<Notify>> {
    state.agents.get_mut(&AgentId::main()).map(|main| {
        main.activity_revision = main.activity_revision.saturating_add(1);
        main.wake.clone()
    })
}

pub fn require_main(state: &TeamState, caller: &AgentId) -> Result<(), CoordinatorError> {
    let agent = state
        .agents
        .get(caller)
        .ok_or_else(|| CoordinatorError::UnknownAgent(caller.clone()))?;
    if agent.depth.is_main() {
        Ok(())
    } else {
        Err(CoordinatorError::CallerNotMain(caller.clone()))
    }
}

pub fn fold_replay(
    state: &mut TeamState,
    replay: &[ReplayedEvent],
) -> Result<(), CoordinatorError> {
    if let Some(first) = replay.first()
        && !matches!(
            &first.event,
            JournalEvent::AgentCreated { agent_id, .. } if agent_id.is_main()
        )
    {
        return Err(CoordinatorError::InvalidReplay(
            "the first journal event must create main".to_owned(),
        ));
    }
    let mut seen_message_ids = HashSet::new();
    for entry in replay {
        match &entry.event {
            JournalEvent::AgentCreated {
                agent_id,
                role,
                task,
            } => {
                validate_role(role)?;
                if agent_id.is_main() {
                    validate_goal(task, false)?;
                } else {
                    validate_task(task)?;
                }
                let (depth, parent) = if agent_id.is_main() {
                    (AgentDepth::MAIN, None)
                } else {
                    (AgentDepth::try_from(1u8)?, Some(AgentId::main()))
                };
                if state.agents.contains_key(agent_id) {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "agent {agent_id} was created more than once"
                    )));
                }
                if !agent_id.is_main() && !state.agents.contains_key(&AgentId::main()) {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "worker {agent_id} was created before main"
                    )));
                }
                if !agent_id.is_main() {
                    TeamLimits::default().validate_total(
                        state
                            .agents
                            .values()
                            .filter(|agent| !agent.state.is_terminal())
                            .count()
                            .saturating_add(1),
                    )?;
                }
                state.agents.insert(
                    agent_id.clone(),
                    new_record(
                        agent_id.clone(),
                        depth,
                        parent,
                        role.clone(),
                        task.clone(),
                        None,
                    ),
                );
            }
            JournalEvent::AgentAssigned {
                agent_id,
                role,
                task,
            } => {
                validate_role(role)?;
                if agent_id.is_main() {
                    validate_goal(task, true)?;
                } else {
                    validate_task(task)?;
                }
                let agent = state.agents.get_mut(agent_id).ok_or_else(|| {
                    CoordinatorError::InvalidReplay(format!(
                        "assignment references unknown agent {agent_id}"
                    ))
                })?;
                if !agent_id.is_main() && agent.state.is_terminal() {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "assignment references inactive worker {agent_id}"
                    )));
                }
                agent.role.clone_from(role);
                agent.task.clone_from(task);
                if !agent_id.is_main() {
                    agent.activity_revision = agent.activity_revision.saturating_add(1);
                }
            }
            JournalEvent::AgentStateChanged {
                agent_id,
                from,
                to,
                reason,
            } => {
                validate_reason(reason)?;
                let agent = state.agents.get_mut(agent_id).ok_or_else(|| {
                    CoordinatorError::InvalidReplay(format!(
                        "state transition references unknown agent {agent_id}"
                    ))
                })?;
                if agent.state != *from {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "state transition for {agent_id} expected {:?}, recorded {:?}",
                        agent.state, from
                    )));
                }
                if agent_id.is_main() && to.is_terminal() {
                    return Err(CoordinatorError::InvalidReplay(
                        "main cannot enter a terminal state".to_owned(),
                    ));
                }
                agent.state.transition_to(*to)?;
                agent.state = *to;
                agent.waiting_on = (*to == AgentState::Waiting).then(|| reason.clone());
            }
            JournalEvent::AgentMessage { message } => {
                message.validate()?;
                if !seen_message_ids.insert(message.id) {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "message {} was recorded more than once",
                        message.id
                    )));
                }
                let sender = state.agents.get(&message.sender).ok_or_else(|| {
                    CoordinatorError::InvalidReplay(format!(
                        "message references unknown sender {}",
                        message.sender
                    ))
                })?;
                if sender.state.is_terminal() {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "message sender {} was inactive",
                        message.sender
                    )));
                }
                if let Some(insight) = &message.insight
                    && let Some(sender) = state.agents.get_mut(&message.sender)
                {
                    sender.latest_insight = Some(insight.text.clone());
                }
                for recipient in &message.audience {
                    let agent = state.agents.get_mut(recipient).ok_or_else(|| {
                        CoordinatorError::InvalidReplay(format!(
                            "message references unknown recipient {recipient}"
                        ))
                    })?;
                    ensure_delivery_capacity(agent, message)?;
                    agent.inbox.push_back(MessageDelivery {
                        sequence: entry.sequence,
                        message: message.clone(),
                    });
                    agent.inbox_bytes = agent.inbox_bytes.saturating_add(message.payload_bytes());
                    agent.activity_revision = agent.activity_revision.saturating_add(1);
                }
            }
            JournalEvent::MessageConsumed {
                recipient,
                message_ids,
            } => {
                let agent = state.agents.get_mut(recipient).ok_or_else(|| {
                    CoordinatorError::InvalidReplay(format!(
                        "message acknowledgement references unknown agent {recipient}"
                    ))
                })?;
                let consumed: HashSet<_> = message_ids.iter().copied().collect();
                if consumed.len() != message_ids.len()
                    || !consumed.iter().all(|id| {
                        agent
                            .inbox
                            .iter()
                            .any(|delivery| delivery.message.id == *id)
                    })
                {
                    return Err(CoordinatorError::InvalidReplay(format!(
                        "message acknowledgement for {recipient} is not present in the inbox"
                    )));
                }
                agent.inbox.retain(|delivery| {
                    if consumed.contains(&delivery.message.id) {
                        agent.inbox_bytes = agent
                            .inbox_bytes
                            .saturating_sub(delivery.message.payload_bytes());
                        false
                    } else {
                        true
                    }
                });
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn ensure_delivery_capacity(
    agent: &AgentRecord,
    message: &AgentMessage,
) -> Result<(), CoordinatorError> {
    if agent.state.is_terminal() {
        return Err(CoordinatorError::InactiveAgent(agent.id.clone()));
    }
    let over_count = agent.inbox.len() >= MAX_INBOX_MESSAGES;
    let over_bytes = agent
        .inbox_bytes
        .checked_add(message.payload_bytes())
        .is_none_or(|bytes| bytes > MAX_INBOX_BYTES);
    if over_count || over_bytes {
        return Err(CoordinatorError::InboxFull(agent.id.clone()));
    }
    Ok(())
}

pub fn next_worker_number<'a>(ids: impl Iterator<Item = &'a AgentId>) -> u64 {
    ids.filter_map(|id| id.as_str().strip_prefix(WORKER_ID_PREFIX))
        .filter_map(|suffix| suffix.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}
