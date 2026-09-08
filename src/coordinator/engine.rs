use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Notify, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::{
    AgentDepth, AgentId, AgentMessage, AgentState, DomainError, Insight, MessageKind, TeamLimits,
    validate_goal, validate_reason, validate_role, validate_task,
};
use crate::journal::{JournalError, JournalEvent, RunJournal};

use super::constants::MAIN_COORDINATOR_ROLE;
use super::error::CoordinatorError;
use super::record::{
    AgentRecord, TeamState, collect_descendants, ensure_delivery_capacity, fold_replay, new_record,
    next_worker_number, require_main, signal_main_activity,
};
use super::snapshot::{AgentSnapshot, MessageDelivery, SendReceipt};

pub struct CoordinatorInner {
    pub journal: Arc<RunJournal>,
    pub state: Mutex<TeamState>,
    pub worker_permits: Arc<Semaphore>,
    pub next_worker: AtomicU64,
}

#[derive(Clone)]
pub struct AgentCoordinator {
    inner: Arc<CoordinatorInner>,
}

impl AgentCoordinator {
    pub fn new(
        journal: Arc<RunJournal>,
        goal: impl Into<String>,
    ) -> Result<Self, CoordinatorError> {
        let coordinator = Self::empty(journal);
        let main = AgentId::main();
        let role = MAIN_COORDINATOR_ROLE.to_owned();
        let task = goal.into();
        validate_goal(&task, false)?;
        coordinator
            .inner
            .journal
            .append_first_sync(JournalEvent::AgentCreated {
                agent_id: main.clone(),
                role: role.clone(),
                task: task.clone(),
            })
            .map_err(|error| match error {
                JournalError::ExpectedEmpty => CoordinatorError::RunNotEmpty,
                other => CoordinatorError::Journal(other),
            })?;
        coordinator.lock_state()?.agents.insert(
            main.clone(),
            new_record(main, AgentDepth::MAIN, None, role, task, None),
        );
        Ok(coordinator)
    }

    pub fn recover(journal: Arc<RunJournal>) -> Result<Self, CoordinatorError> {
        let replay = journal.replay()?;
        let coordinator = Self::empty(journal);
        let mut state = TeamState::default();
        fold_replay(&mut state, &replay)?;
        if !state.agents.contains_key(&AgentId::main()) {
            return Err(CoordinatorError::MissingMain);
        }

        let active_worker_ids: Vec<_> = state
            .agents
            .values()
            .filter(|agent| !agent.depth.is_main() && !agent.state.is_terminal())
            .map(|agent| agent.id.clone())
            .collect();
        for id in active_worker_ids {
            let permit = coordinator
                .inner
                .worker_permits
                .clone()
                .try_acquire_owned()
                .map_err(|_| CoordinatorError::Domain(DomainError::TeamFull))?;
            if let Some(agent) = state.agents.get_mut(&id) {
                agent.worker_permit = Some(permit);
                if agent.state == AgentState::Recalling {
                    agent.cancellation.cancel();
                }
            }
        }
        let next = next_worker_number(state.agents.keys());
        coordinator.inner.next_worker.store(next, Ordering::Release);
        *coordinator.lock_state()? = state;
        Ok(coordinator)
    }

    fn empty(journal: Arc<RunJournal>) -> Self {
        Self {
            inner: Arc::new(CoordinatorInner {
                journal,
                state: Mutex::new(TeamState::default()),
                worker_permits: Arc::new(Semaphore::new(TeamLimits::default().max_workers())),
                next_worker: AtomicU64::new(1),
            }),
        }
    }

    pub fn journal(&self) -> Arc<RunJournal> {
        self.inner.journal.clone()
    }

    pub fn create_worker(
        &self,
        caller: &AgentId,
        role: impl Into<String>,
        task: impl Into<String>,
    ) -> Result<AgentId, CoordinatorError> {
        let role = role.into();
        let task = task.into();
        validate_role(&role)?;
        validate_task(&task)?;
        let mut state = self.lock_state()?;
        let caller_record = state
            .agents
            .get(caller)
            .ok_or_else(|| CoordinatorError::UnknownAgent(caller.clone()))?;
        TeamLimits::default().validate_spawn(caller_record.depth)?;
        let child_depth = caller_record.depth.child()?;
        let permit = self
            .inner
            .worker_permits
            .clone()
            .try_acquire_owned()
            .map_err(|_| CoordinatorError::Domain(DomainError::TeamFull))?;
        let id = loop {
            let number = self.inner.next_worker.fetch_add(1, Ordering::AcqRel);
            let candidate = AgentId::new(format!("worker-{number:02}"))?;
            if !state.agents.contains_key(&candidate) {
                break candidate;
            }
        };
        self.inner.journal.append_sync(JournalEvent::AgentCreated {
            agent_id: id.clone(),
            role: role.clone(),
            task: task.clone(),
        })?;
        state.agents.insert(
            id.clone(),
            new_record(
                id.clone(),
                child_depth,
                Some(caller.clone()),
                role,
                task,
                Some(permit),
            ),
        );
        Ok(id)
    }

    pub fn reassign(
        &self,
        caller: &AgentId,
        target: &AgentId,
        role: impl Into<String>,
        task: impl Into<String>,
    ) -> Result<(), CoordinatorError> {
        let role = role.into();
        let task = task.into();
        validate_role(&role)?;
        validate_task(&task)?;
        let mut state = self.lock_state()?;
        require_main(&state, caller)?;
        let agent = state
            .agents
            .get_mut(target)
            .ok_or_else(|| CoordinatorError::UnknownAgent(target.clone()))?;
        if agent.depth.is_main() || agent.state.is_terminal() {
            return Err(CoordinatorError::InactiveWorker(target.clone()));
        }
        self.inner
            .journal
            .append_sync(JournalEvent::AgentAssigned {
                agent_id: target.clone(),
                role: role.clone(),
                task: task.clone(),
            })?;
        agent.role = role;
        agent.task = task;
        agent.activity_revision = agent.activity_revision.saturating_add(1);
        agent.wake.notify_one();
        Ok(())
    }

    /// Replaces the durable main objective without enabling autonomous work.
    pub fn set_goal(
        &self,
        caller: &AgentId,
        goal: impl Into<String>,
    ) -> Result<(), CoordinatorError> {
        let goal = goal.into();
        validate_goal(&goal, true)?;
        let mut state = self.lock_state()?;
        require_main(&state, caller)?;
        let main = AgentId::main();
        let agent = state
            .agents
            .get_mut(&main)
            .ok_or(CoordinatorError::MissingMain)?;
        self.inner
            .journal
            .append_sync(JournalEvent::AgentAssigned {
                agent_id: main,
                role: agent.role.clone(),
                task: goal.clone(),
            })?;
        agent.task = goal;
        Ok(())
    }

    pub fn mark_waiting(
        &self,
        id: &AgentId,
        reason: impl Into<String>,
    ) -> Result<(), CoordinatorError> {
        self.set_execution_state(id, AgentState::Waiting, reason.into())
    }

    pub fn mark_running(
        &self,
        id: &AgentId,
        reason: impl Into<String>,
    ) -> Result<(), CoordinatorError> {
        self.set_execution_state(id, AgentState::Running, reason.into())
    }

    fn set_execution_state(
        &self,
        id: &AgentId,
        next: AgentState,
        reason: String,
    ) -> Result<(), CoordinatorError> {
        validate_reason(&reason)?;
        if !matches!(next, AgentState::Running | AgentState::Waiting) {
            return Err(CoordinatorError::InvalidExecutionState(next));
        }
        let mut state = self.lock_state()?;
        {
            let agent = state
                .agents
                .get_mut(id)
                .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))?;
            if agent.state == next {
                let waiting_on = (next == AgentState::Waiting).then_some(reason);
                if agent.waiting_on == waiting_on {
                    return Ok(());
                }
                self.inner
                    .journal
                    .append_sync(JournalEvent::AgentStateChanged {
                        agent_id: id.clone(),
                        from: agent.state,
                        to: next,
                        reason: waiting_on.clone().unwrap_or_default(),
                    })?;
                agent.waiting_on = waiting_on;
            } else {
                agent.state.transition_to(next)?;
                self.inner
                    .journal
                    .append_sync(JournalEvent::AgentStateChanged {
                        agent_id: id.clone(),
                        from: agent.state,
                        to: next,
                        reason: reason.clone(),
                    })?;
                agent.state = next;
                agent.waiting_on = (next == AgentState::Waiting).then_some(reason);
            }
        }
        let main_wake = (!id.is_main())
            .then(|| signal_main_activity(&mut state))
            .flatten();
        drop(state);
        if let Some(wake) = main_wake {
            wake.notify_one();
        }
        Ok(())
    }

    pub fn recall(
        &self,
        caller: &AgentId,
        target: &AgentId,
        reason: impl Into<String>,
    ) -> Result<(), CoordinatorError> {
        let reason = reason.into();
        validate_reason(&reason)?;
        let mut state = self.lock_state()?;
        require_main(&state, caller)?;
        let agent = state
            .agents
            .get_mut(target)
            .ok_or_else(|| CoordinatorError::UnknownAgent(target.clone()))?;
        if agent.depth.is_main() {
            return Err(CoordinatorError::InactiveWorker(target.clone()));
        }
        agent.state.transition_to(AgentState::Recalling)?;
        self.inner
            .journal
            .append_sync(JournalEvent::AgentStateChanged {
                agent_id: target.clone(),
                from: agent.state,
                to: AgentState::Recalling,
                reason,
            })?;
        agent.state = AgentState::Recalling;
        agent.cancellation.cancel();
        let target_wake = agent.wake.clone();
        let cascade_wakes = self.cascade_subtree(
            &mut state,
            target,
            AgentState::Recalling,
            &format!("ancestor {target} recalled"),
        )?;
        drop(state);
        target_wake.notify_one();
        for wake in cascade_wakes {
            wake.notify_one();
        }
        Ok(())
    }

    pub fn mark_terminal(
        &self,
        actor: &AgentId,
        target: &AgentId,
        terminal: AgentState,
        reason: impl Into<String>,
    ) -> Result<(), CoordinatorError> {
        let reason = reason.into();
        validate_reason(&reason)?;
        if !terminal.is_terminal() {
            return Err(CoordinatorError::NotTerminal(terminal));
        }
        if target.is_main() {
            return Err(CoordinatorError::CannotTerminateMain);
        }
        let mut state = self.lock_state()?;
        if actor != target {
            require_main(&state, actor)?;
        }
        let target_wake = {
            let agent = state
                .agents
                .get_mut(target)
                .ok_or_else(|| CoordinatorError::UnknownAgent(target.clone()))?;
            agent.state.transition_to(terminal)?;
            self.inner
                .journal
                .append_sync(JournalEvent::AgentStateChanged {
                    agent_id: target.clone(),
                    from: agent.state,
                    to: terminal,
                    reason,
                })?;
            agent.state = terminal;
            agent.worker_permit.take();
            if terminal != AgentState::Finished {
                agent.cancellation.cancel();
            }
            agent.wake.clone()
        };
        let cascade_wakes = self.cascade_subtree(
            &mut state,
            target,
            AgentState::Stopped,
            &format!("ancestor {target} terminated"),
        )?;
        let main_wake = signal_main_activity(&mut state);
        drop(state);
        target_wake.notify_one();
        for wake in cascade_wakes {
            wake.notify_one();
        }
        if let Some(wake) = main_wake {
            wake.notify_one();
        }
        Ok(())
    }

    fn cascade_subtree(
        &self,
        state: &mut TeamState,
        root: &AgentId,
        to: AgentState,
        reason: &str,
    ) -> Result<Vec<Arc<Notify>>, CoordinatorError> {
        let mut wakes = Vec::new();
        for id in collect_descendants(state, root) {
            let Some(agent) = state.agents.get_mut(&id) else {
                continue;
            };
            if agent.state.is_terminal() || agent.state == to {
                continue;
            }
            let from = agent.state;
            if agent.state.transition_to(to).is_err() {
                continue;
            }
            self.inner
                .journal
                .append_sync(JournalEvent::AgentStateChanged {
                    agent_id: id.clone(),
                    from,
                    to,
                    reason: reason.to_owned(),
                })?;
            agent.state = to;
            if to.is_terminal() {
                agent.worker_permit.take();
            }
            agent.cancellation.cancel();
            wakes.push(agent.wake.clone());
        }
        Ok(wakes)
    }

    pub fn send(
        &self,
        sender: AgentId,
        audience: Vec<AgentId>,
        kind: MessageKind,
        body: impl Into<String>,
        insight: Option<Insight>,
    ) -> Result<SendReceipt, CoordinatorError> {
        let message = AgentMessage::new(sender.clone(), audience, kind, body, insight)?;
        let mut state = self.lock_state()?;
        let sender_record = state
            .agents
            .get(&sender)
            .ok_or_else(|| CoordinatorError::UnknownAgent(sender.clone()))?;
        if sender_record.state.is_terminal() {
            return Err(CoordinatorError::InactiveAgent(sender));
        }
        let sender_parent = sender_record.parent.clone();
        for recipient in &message.audience {
            let agent = state
                .agents
                .get(recipient)
                .ok_or_else(|| CoordinatorError::UnknownAgent(recipient.clone()))?;
            let is_parent = sender_parent.as_ref() == Some(recipient);
            let is_child = agent.parent.as_ref() == Some(&sender);
            let is_sibling =
                recipient != &sender && agent.parent.is_some() && agent.parent == sender_parent;
            if !(is_parent || is_child || is_sibling) {
                return Err(CoordinatorError::Domain(DomainError::NonNeighborAudience));
            }
            ensure_delivery_capacity(agent, &message)?;
        }
        let ack = self.inner.journal.append_sync(JournalEvent::AgentMessage {
            message: message.clone(),
        })?;
        if let Some(insight) = &message.insight
            && let Some(sender) = state.agents.get_mut(&message.sender)
        {
            sender.latest_insight = Some(insight.text.clone());
        }
        let mut wakes = Vec::new();
        for recipient in &message.audience {
            let agent = state
                .agents
                .get_mut(recipient)
                .ok_or_else(|| CoordinatorError::UnknownAgent(recipient.clone()))?;
            if !agent
                .inbox
                .iter()
                .any(|delivery| delivery.message.id == message.id)
            {
                agent.inbox.push_back(MessageDelivery {
                    sequence: ack.sequence,
                    message: message.clone(),
                });
                agent.inbox_bytes = agent.inbox_bytes.saturating_add(message.payload_bytes());
                agent.activity_revision = agent.activity_revision.saturating_add(1);
            }
            wakes.push(agent.wake.clone());
        }
        drop(state);
        for wake in wakes {
            wake.notify_one();
        }
        Ok(SendReceipt {
            sequence: ack.sequence,
            message_id: message.id,
            audience: message.audience,
        })
    }

    pub fn inbox(&self, id: &AgentId) -> Result<Vec<MessageDelivery>, CoordinatorError> {
        self.inbox_snapshot(id).map(|(_, inbox)| inbox)
    }

    pub fn inbox_snapshot(
        &self,
        id: &AgentId,
    ) -> Result<(u64, Vec<MessageDelivery>), CoordinatorError> {
        let state = self.lock_state()?;
        let agent = state
            .agents
            .get(id)
            .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))?;
        Ok((
            agent.activity_revision,
            agent.inbox.iter().cloned().collect(),
        ))
    }

    pub fn consume(&self, id: &AgentId, message_ids: &[Uuid]) -> Result<(), CoordinatorError> {
        let ids: std::collections::HashSet<_> = message_ids.iter().copied().collect();
        let mut state = self.lock_state()?;
        let agent = state
            .agents
            .get_mut(id)
            .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))?;
        let present: Vec<_> = agent
            .inbox
            .iter()
            .filter(|delivery| ids.contains(&delivery.message.id))
            .map(|delivery| delivery.message.id)
            .collect();
        if present.is_empty() {
            return Ok(());
        }
        self.inner
            .journal
            .append_sync(JournalEvent::MessageConsumed {
                recipient: id.clone(),
                message_ids: present.clone(),
            })?;
        let consumed: std::collections::HashSet<_> = present.into_iter().collect();
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
        Ok(())
    }

    pub async fn wait_for_messages(
        &self,
        id: &AgentId,
        timeout: Duration,
    ) -> Result<Vec<MessageDelivery>, CoordinatorError> {
        let wake = {
            let state = self.lock_state()?;
            state
                .agents
                .get(id)
                .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))?
                .wake
                .clone()
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = wake.notified();
            let inbox = self.inbox(id)?;
            if !inbox.is_empty() {
                return Ok(inbox);
            }
            tokio::time::timeout_at(deadline, notified)
                .await
                .map_err(|_| CoordinatorError::WaitTimedOut)?;
        }
    }

    pub fn activity_revision(&self, id: &AgentId) -> Result<u64, CoordinatorError> {
        self.lock_state()?
            .agents
            .get(id)
            .map(|agent| agent.activity_revision)
            .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))
    }

    pub async fn wait_for_activity_after(
        &self,
        id: &AgentId,
        observed: u64,
        timeout: Duration,
    ) -> Result<u64, CoordinatorError> {
        let wake = {
            let state = self.lock_state()?;
            state
                .agents
                .get(id)
                .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))?
                .wake
                .clone()
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = wake.notified();
            let revision = self.activity_revision(id)?;
            if revision > observed {
                return Ok(revision);
            }
            tokio::time::timeout_at(deadline, notified)
                .await
                .map_err(|_| CoordinatorError::WaitTimedOut)?;
        }
    }

    pub fn inspect(&self, id: &AgentId) -> Result<AgentSnapshot, CoordinatorError> {
        self.lock_state()?
            .agents
            .get(id)
            .map(AgentRecord::snapshot)
            .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))
    }

    pub fn team(&self) -> Result<Vec<AgentSnapshot>, CoordinatorError> {
        let mut team: Vec<_> = self
            .lock_state()?
            .agents
            .values()
            .map(AgentRecord::snapshot)
            .collect();
        team.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        Ok(team)
    }

    pub fn live_team(&self) -> Result<Vec<AgentSnapshot>, CoordinatorError> {
        let mut team: Vec<_> = self
            .lock_state()?
            .agents
            .values()
            .filter(|agent| !agent.state.is_terminal())
            .map(AgentRecord::snapshot)
            .collect();
        team.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        Ok(team)
    }

    pub fn nudge_active(&self) -> Result<(), CoordinatorError> {
        let wakes = {
            let mut state = self.lock_state()?;
            state
                .agents
                .values_mut()
                .filter(|agent| !agent.state.is_terminal())
                .map(|agent| {
                    agent.activity_revision = agent.activity_revision.saturating_add(1);
                    agent.wake.clone()
                })
                .collect::<Vec<_>>()
        };
        for wake in wakes {
            wake.notify_one();
        }
        Ok(())
    }

    pub fn active_team_size(&self) -> usize {
        self.lock_state()
            .map(|state| {
                state
                    .agents
                    .values()
                    .filter(|agent| !agent.state.is_terminal())
                    .count()
            })
            .unwrap_or_default()
    }

    pub fn cancellation_token(&self, id: &AgentId) -> Result<CancellationToken, CoordinatorError> {
        self.lock_state()?
            .agents
            .get(id)
            .map(|agent| agent.cancellation.clone())
            .ok_or_else(|| CoordinatorError::UnknownAgent(id.clone()))
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, TeamState>, CoordinatorError> {
        self.inner
            .state
            .lock()
            .map_err(|_| CoordinatorError::LockPoisoned)
    }
}
