use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use thiserror::Error;
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::{
    AgentDepth, AgentId, AgentMessage, AgentState, DomainError, Insight, MAX_INBOX_BYTES,
    MAX_INBOX_MESSAGES, MessageKind, TeamLimits, validate_goal, validate_reason, validate_role,
    validate_task,
};
use crate::journal::{JournalError, JournalEvent, ReplayedEvent, RunJournal};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub id: AgentId,
    pub depth: AgentDepth,
    /// The direct parent in the team tree; `None` only for main (ADR-0004).
    pub parent: Option<AgentId>,
    pub role: String,
    pub task: String,
    pub state: AgentState,
    pub unread: usize,
    pub latest_insight: Option<String>,
    pub waiting_on: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageDelivery {
    pub sequence: u64,
    pub message: AgentMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendReceipt {
    pub sequence: u64,
    pub message_id: Uuid,
    pub audience: Vec<AgentId>,
}

struct AgentRecord {
    id: AgentId,
    depth: AgentDepth,
    parent: Option<AgentId>,
    role: String,
    task: String,
    state: AgentState,
    inbox: VecDeque<MessageDelivery>,
    inbox_bytes: usize,
    latest_insight: Option<String>,
    waiting_on: Option<String>,
    activity_revision: u64,
    wake: Arc<Notify>,
    cancellation: CancellationToken,
    worker_permit: Option<OwnedSemaphorePermit>,
}

impl AgentRecord {
    fn snapshot(&self) -> AgentSnapshot {
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
struct TeamState {
    agents: HashMap<AgentId, AgentRecord>,
}

struct CoordinatorInner {
    journal: Arc<RunJournal>,
    state: Mutex<TeamState>,
    worker_permits: Arc<Semaphore>,
    next_worker: AtomicU64,
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
        let role = "main coordinator".to_owned();
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

        // Restart restores the live team from the journal (ADR-0001 durable
        // resume). Deep grandchild structure is NOT reconstructed — recovered
        // non-main agents fold back as direct children of main (fold_replay), which
        // keeps recovery light without a tree-reconstruction machine. Whether a
        // restart should instead revive ONLY the root (ADR-0004 §6 recovery
        // method A) is a pending decision that conflicts with this ADR-0001 resume.
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
        // Recalling an internal node recalls its whole subtree so descendants stop
        // promptly rather than running on until the parent tears down (ADR-0004 §7).
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
        // Cascade (ADR-0004 §7): terminating a node terminates its whole subtree so
        // no grandchild is orphaned. This also reclaims the subtree of a node that
        // died unexpectedly (a faulted node reaches here with a terminal state).
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

    /// Cascades a state change down the whole subtree rooted at `root` (excluding
    /// `root` itself): each descendant is transitioned to `to`, its transition
    /// journaled, its work cancelled, and its permit released when `to` is
    /// terminal. Descendants already terminal, or already at `to`, are skipped.
    /// Returns their wake handles so the caller can notify after unlocking the
    /// state (ADR-0004 §7 subtree teardown).
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
        // Neighbor-only routing (ADR-0004 §3.4): a sender may address only its
        // direct parent, its direct children, and its siblings (same parent).
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
        let ids: HashSet<_> = message_ids.iter().copied().collect();
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
        let consumed: HashSet<_> = present.into_iter().collect();
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

    /// Returns the addressable team used by hot-path UI and brief projections.
    /// Terminal agents remain available through `inspect` and durable replay,
    /// but cannot make the current-team context grow forever.
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

    /// Signals an explicit autonomous resume without fabricating a message.
    /// The monotonic revision prevents a wake notification from being lost
    /// between driver state transitions.
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

fn new_record(
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

/// All transitive descendants of `root` in the team tree, found by following
/// parent pointers. Used to cascade a teardown down a subtree (ADR-0004 §7).
fn collect_descendants(state: &TeamState, root: &AgentId) -> Vec<AgentId> {
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

fn signal_main_activity(state: &mut TeamState) -> Option<Arc<Notify>> {
    state.agents.get_mut(&AgentId::main()).map(|main| {
        main.activity_revision = main.activity_revision.saturating_add(1);
        main.wake.clone()
    })
}

fn require_main(state: &TeamState, caller: &AgentId) -> Result<(), CoordinatorError> {
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

fn fold_replay(state: &mut TeamState, replay: &[ReplayedEvent]) -> Result<(), CoordinatorError> {
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
                // The journal does not record depth/parent (ADR-0004 §6: recovery
                // stays lightweight). A recovered non-main agent is folded back as
                // a direct child of main; deep-subtree structure is not
                // reconstructed — the root re-delegates what remains.
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

fn ensure_delivery_capacity(
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

fn next_worker_number<'a>(ids: impl Iterator<Item = &'a AgentId>) -> u64 {
    ids.filter_map(|id| id.as_str().strip_prefix("worker-"))
        .filter_map(|suffix| suffix.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

#[derive(Debug, Error)]
pub enum CoordinatorError {
    #[error(transparent)]
    Domain(#[from] DomainError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error("unknown agent: {0}")]
    UnknownAgent(AgentId),
    #[error("worker is inactive: {0}")]
    InactiveWorker(AgentId),
    #[error("agent is inactive: {0}")]
    InactiveAgent(AgentId),
    #[error("agent inbox is full: {0}")]
    InboxFull(AgentId),
    #[error("state {0:?} is not terminal")]
    NotTerminal(AgentState),
    #[error("main is the stable team root and cannot be marked terminal")]
    CannotTerminateMain,
    #[error("only main may perform this operation: {0}")]
    CallerNotMain(AgentId),
    #[error("state {0:?} is not a running/waiting execution state")]
    InvalidExecutionState(AgentState),
    #[error("coordinator state lock was poisoned")]
    LockPoisoned,
    #[error("run journal does not contain a main agent")]
    MissingMain,
    #[error("a new run requires an empty journal")]
    RunNotEmpty,
    #[error("waiting for an agent message timed out")]
    WaitTimedOut,
    #[error("journal replay violates coordinator invariants: {0}")]
    InvalidReplay(String),
}
