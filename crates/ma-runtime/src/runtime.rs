use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use serde_json::json;
use thiserror::Error;
use tokio::sync::{Semaphore, broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::tools::{BuiltinTools, ToolContext, ToolError, WorkerSpawner};
use ma_context::brief::{AgentBriefStore, BriefDraft, BriefError};
use ma_context::compaction::{
    CompactionError, CompactionOutcome, ContextEntry, LiveReason, SemanticCompactionConfig,
    SemanticCompactor,
};
use ma_coordinator::{AgentCoordinator, AgentSnapshot, CoordinatorError, MessageDelivery};
use ma_core::domain::{
    AgentId, AgentState, ContextBudget, InsightId, MessageKind, SequenceRange, estimate_tokens,
    validate_user_input,
};
use ma_core::engagement::{
    Engagement, EngagementKind, authorized_engagement_doctrine, ctf_solve_loop_doctrine,
    execution_style_directive,
};
use ma_journal::{JournalConfig, JournalError, JournalEvent, RunJournal, TranscriptRole};
use ma_provider::provider::{
    ModelDelta, ModelMessage, ModelProvider, ModelRequest, ModelRole, ModelTurn, ProviderFault,
    ToolCall,
};

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub configured_context_tokens: u64,
    pub reserved_response_tokens: u64,
    pub max_model_turns: usize,
    pub max_parallel_requests: usize,
    pub tool_timeout: Duration,
    pub compaction_timeout: Duration,
    pub auto: bool,
    pub journal: JournalConfig,
    pub compaction: SemanticCompactionConfig,
    /// Optional authorized-engagement context injected from outside (ADR-0002).
    /// Rendered into every agent's system prompt; never persisted to the journal.
    pub engagement: Option<Engagement>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let max_model_turns = std::env::var("MINIMAL_AGENT_MAX_MODEL_TURNS")
            .ok()
            .and_then(|val| {
                let trimmed = val.trim();
                if trimmed.eq_ignore_ascii_case("unlimited")
                    || trimmed.eq_ignore_ascii_case("infinite")
                    || trimmed == "0"
                {
                    Some(usize::MAX)
                } else {
                    trimmed.parse().ok()
                }
            })
            .unwrap_or(usize::MAX);
        Self {
            configured_context_tokens: 128_000,
            reserved_response_tokens: 8_192,
            max_model_turns,
            max_parallel_requests: 4,
            tool_timeout: Duration::from_secs(300),
            compaction_timeout: Duration::from_secs(300),
            auto: false,
            journal: JournalConfig::default(),
            compaction: SemanticCompactionConfig::default(),
            engagement: None,
        }
    }
}

impl RuntimeConfig {
    fn validate(&self, provider_context: u64) -> Result<ContextBudget, RuntimeError> {
        if self.max_parallel_requests == 0
            || self.tool_timeout.is_zero()
            || self.compaction_timeout.is_zero()
        {
            return Err(RuntimeError::InvalidConfig);
        }
        Ok(ContextBudget::new(
            self.configured_context_tokens,
            provider_context,
            self.reserved_response_tokens,
        )?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnResult {
    pub text: String,
    pub finalized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    TurnStarted {
        agent_id: AgentId,
    },
    TurnFinished {
        agent_id: AgentId,
        success: bool,
    },
    Delta {
        agent_id: AgentId,
        delta: ModelDelta,
    },
    ToolStarted {
        agent_id: AgentId,
        name: String,
        /// Short, bounded human summary of the call target (e.g. the shell
        /// command or the workspace path) for clean transcript rendering.
        summary: Option<String>,
    },
    ToolFinished {
        agent_id: AgentId,
        name: String,
        success: bool,
        output: String,
    },
    Assistant {
        agent_id: AgentId,
        text: String,
    },
    AgentMessage {
        sender: AgentId,
        recipients: Vec<AgentId>,
        kind: MessageKind,
        body: String,
    },
    TeamChanged,
    Fault {
        agent_id: AgentId,
        message: String,
    },
}

enum MainCommand {
    User {
        text: String,
        reply: oneshot::Sender<Result<TurnResult, RuntimeError>>,
    },
    Compact {
        reply: oneshot::Sender<Result<bool, RuntimeError>>,
    },
    Goal {
        objective: Option<String>,
        reply: oneshot::Sender<Result<(), RuntimeError>>,
    },
    Bash {
        command: String,
        reply: oneshot::Sender<Result<String, RuntimeError>>,
    },
    Stop,
}

#[derive(Clone)]
pub struct TeamRuntime {
    inner: Arc<RuntimeInner>,
}

struct RuntimeInner {
    workspace: PathBuf,
    journal: Arc<RunJournal>,
    coordinator: AgentCoordinator,
    briefs: AgentBriefStore,
    provider: Arc<dyn ModelProvider>,
    tools: BuiltinTools,
    budget: ContextBudget,
    config: RuntimeConfig,
    requests: Arc<Semaphore>,
    auto: watch::Sender<bool>,
    shutdown: CancellationToken,
    events: broadcast::Sender<RuntimeEvent>,
    main_tx: mpsc::Sender<MainCommand>,
    main_turn_interrupt: Mutex<Option<CancellationToken>>,
    tasks: Mutex<HashMap<AgentId, JoinHandle<()>>>,
    active_turns: AtomicUsize,
    /// User input delivered while a main turn is running. It is injected into the
    /// running turn's session as a user message at the next model-turn boundary,
    /// so the agent folds it into its current context instead of losing progress.
    main_steer: Mutex<VecDeque<String>>,
    /// Live authorized-engagement context. Seeded from the config and updatable at
    /// runtime (e.g. `/target`); every prompt build reads the current value.
    engagement: Mutex<Option<Engagement>>,
}

struct RuntimeSpawner {
    inner: Weak<RuntimeInner>,
}

#[async_trait::async_trait]
impl WorkerSpawner for RuntimeSpawner {
    async fn spawn(
        &self,
        caller: &AgentId,
        role: String,
        task: String,
    ) -> Result<AgentId, ToolError> {
        let inner = self
            .inner
            .upgrade()
            .ok_or_else(|| ToolError::Spawner("runtime has stopped".to_owned()))?;
        TeamRuntime { inner }
            .spawn_worker(caller, role, task)
            .await
            .map_err(|error| ToolError::Spawner(error.to_string()))
    }
}

impl TeamRuntime {
    pub async fn create(
        run_root: impl AsRef<Path>,
        workspace: impl AsRef<Path>,
        goal: impl Into<String>,
        provider: Arc<dyn ModelProvider>,
        config: RuntimeConfig,
    ) -> Result<Self, RuntimeError> {
        let budget = config.validate(provider.context_limit())?;
        std::fs::create_dir_all(workspace.as_ref())?;
        let workspace = workspace.as_ref().canonicalize()?;
        let journal = Arc::new(RunJournal::open(run_root.as_ref(), config.journal)?);
        let coordinator = AgentCoordinator::new(journal.clone(), goal)?;
        // Persist the engagement right after main creation so a resumed run
        // recovers the same target/scope/flag doctrine (ADR-0002 §4).
        if let Some(engagement) = &config.engagement {
            journal.append_sync(JournalEvent::EngagementSet {
                engagement: engagement.clone(),
            })?;
        }
        let briefs = AgentBriefStore::new(run_root, journal.clone(), budget);
        briefs.initialize(&coordinator.inspect(&AgentId::main())?)?;
        briefs.sync_main_runtime(&AgentId::main(), &coordinator.live_team()?)?;
        let (runtime, main_rx) = Self::assemble(
            workspace,
            journal,
            coordinator,
            briefs,
            provider,
            budget,
            config,
        );
        runtime.start_main_driver(main_rx, AgentSession::default())?;
        Ok(runtime)
    }

    pub async fn resume(
        run_root: impl AsRef<Path>,
        workspace: impl AsRef<Path>,
        provider: Arc<dyn ModelProvider>,
        mut config: RuntimeConfig,
    ) -> Result<Self, RuntimeError> {
        let budget = config.validate(provider.context_limit())?;
        std::fs::create_dir_all(workspace.as_ref())?;
        let workspace = workspace.as_ref().canonicalize()?;
        let journal = Arc::new(RunJournal::open(run_root.as_ref(), config.journal)?);
        // Explicit flags win; otherwise recover the engagement the run was
        // created with so resume keeps the same authorized-engagement doctrine.
        if config.engagement.is_none() {
            config.engagement = recover_engagement(&journal)?;
        }
        let coordinator = AgentCoordinator::recover(journal.clone())?;
        let team = coordinator.team()?;
        let active_ids = team
            .iter()
            .filter(|agent| !agent.state.is_terminal())
            .map(|agent| agent.id.clone())
            .collect::<HashSet<_>>();
        let briefs = AgentBriefStore::new(run_root, journal.clone(), budget);
        briefs.recover_selected(&active_ids)?;
        for agent in team.iter().filter(|agent| !agent.state.is_terminal()) {
            briefs.initialize(agent)?;
        }
        for agent in team.iter().filter(|agent| agent.state.is_terminal()) {
            briefs.remove_projection(&agent.id)?;
        }
        briefs.sync_main_runtime(&AgentId::main(), &coordinator.live_team()?)?;
        let mut sessions = recover_sessions(&journal)?;
        let (runtime, main_rx) = Self::assemble(
            workspace,
            journal,
            coordinator,
            briefs,
            provider,
            budget,
            config,
        );
        runtime.start_main_driver(
            main_rx,
            sessions.remove(&AgentId::main()).unwrap_or_default(),
        )?;
        for agent in team
            .into_iter()
            .filter(|agent| !agent.id.is_main() && !agent.state.is_terminal())
        {
            let session = sessions.remove(&agent.id).unwrap_or_default();
            let initial = Some("Resume from the durable brief and live tail.".to_owned());
            runtime.start_worker_driver(agent.id, session, initial)?;
        }
        Ok(runtime)
    }

    fn assemble(
        workspace: PathBuf,
        journal: Arc<RunJournal>,
        coordinator: AgentCoordinator,
        briefs: AgentBriefStore,
        provider: Arc<dyn ModelProvider>,
        budget: ContextBudget,
        config: RuntimeConfig,
    ) -> (Self, mpsc::Receiver<MainCommand>) {
        let (main_tx, main_rx) = mpsc::channel(64);
        let (events, _) = broadcast::channel(1_024);

        let inner = Arc::new_cyclic(|weak| RuntimeInner {
            workspace,
            journal,
            coordinator,
            briefs: briefs.clone(),
            provider,
            tools: BuiltinTools::new(
                config.tool_timeout,
                Arc::new(RuntimeSpawner {
                    inner: weak.clone(),
                }),
            ),
            budget,
            requests: Arc::new(Semaphore::new(config.max_parallel_requests)),
            auto: watch::channel(config.auto).0,
            shutdown: CancellationToken::new(),
            events,
            main_tx,
            main_turn_interrupt: Mutex::new(None),
            tasks: Mutex::new(HashMap::new()),
            active_turns: AtomicUsize::new(0),
            main_steer: Mutex::new(VecDeque::new()),
            engagement: Mutex::new(config.engagement.clone()),
            config,
        });
        (Self { inner }, main_rx)
    }

    pub fn coordinator(&self) -> AgentCoordinator {
        self.inner.coordinator.clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<RuntimeEvent> {
        self.inner.events.subscribe()
    }

    pub fn brief(&self, agent_id: &AgentId) -> Result<String, RuntimeError> {
        let agent = self.inner.coordinator.inspect(agent_id)?;
        match self.inner.briefs.read_effective(agent_id) {
            Ok(brief) => Ok(brief),
            Err(BriefError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound && agent.state.is_terminal() =>
            {
                Ok("<no live brief projection; durable source remains in the journal>".to_owned())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn submit_user(&self, text: impl Into<String>) -> Result<TurnResult, RuntimeError> {
        let text = text.into();
        validate_user_input(&text)?;
        let (reply, response) = oneshot::channel();
        self.inner
            .main_tx
            .send(MainCommand::User { text, reply })
            .await
            .map_err(|_| RuntimeError::Stopped)?;
        response.await.map_err(|_| RuntimeError::Stopped)?
    }

    pub async fn compact_main(&self) -> Result<bool, RuntimeError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .main_tx
            .send(MainCommand::Compact { reply })
            .await
            .map_err(|_| RuntimeError::Stopped)?;
        response.await.map_err(|_| RuntimeError::Stopped)?
    }

    pub async fn set_goal(&self, objective: Option<String>) -> Result<(), RuntimeError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .main_tx
            .send(MainCommand::Goal { objective, reply })
            .await
            .map_err(|_| RuntimeError::Stopped)?;
        response.await.map_err(|_| RuntimeError::Stopped)?
    }

    pub async fn run_bash(&self, command: impl Into<String>) -> Result<String, RuntimeError> {
        let (reply, response) = oneshot::channel();
        self.inner
            .main_tx
            .send(MainCommand::Bash {
                command: command.into(),
                reply,
            })
            .await
            .map_err(|_| RuntimeError::Stopped)?;
        response.await.map_err(|_| RuntimeError::Stopped)?
    }

    pub fn set_auto(&self, enabled: bool) {
        if enabled {
            let _ = self.inner.coordinator.nudge_active();
        }
        self.inner.auto.send_replace(enabled);
    }

    pub fn auto_enabled(&self) -> bool {
        *self.inner.auto.borrow()
    }

    /// Deliver user input into an in-flight main turn without cancelling it.
    /// Returns `Ok(true)` when a main turn is active and the input was queued for
    /// injection at the next model-turn boundary; `Ok(false)` when main is idle,
    /// so the caller should start a normal turn with `submit_user` instead.
    pub fn steer_main(&self, text: impl Into<String>) -> Result<bool, RuntimeError> {
        let text = text.into();
        validate_user_input(&text)?;
        let active = self
            .inner
            .main_turn_interrupt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some();
        if !active {
            return Ok(false);
        }
        self.inner
            .main_steer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push_back(text);
        Ok(true)
    }

    /// Set or clear the authorized target/scope at runtime. The change is durable
    /// (journaled) and every subsequent prompt build — including an in-flight main
    /// turn's next model-turn — renders the new target.
    pub fn set_engagement_scope(&self, scope: Option<String>) -> Result<(), RuntimeError> {
        let updated = {
            let mut guard = self
                .inner
                .engagement
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.is_none() && scope.is_none() {
                return Ok(());
            }
            let updated = guard.take().unwrap_or_default().set_scope(scope)?;
            *guard = Some(updated.clone());
            updated
        };
        self.inner
            .journal
            .append_sync(JournalEvent::EngagementSet {
                engagement: updated,
            })?;
        self.inner.coordinator.nudge_active()?;
        Ok(())
    }

    pub fn interrupt_main(&self) -> bool {
        let active = self
            .inner
            .main_turn_interrupt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(active) = active {
            active.cancel();
            true
        } else {
            false
        }
    }

    pub async fn spawn_worker(
        &self,
        caller: &AgentId,
        role: impl Into<String>,
        task: impl Into<String>,
    ) -> Result<AgentId, RuntimeError> {
        let role = role.into();
        let task = task.into();
        let id = self
            .inner
            .coordinator
            .create_worker(caller, role, task.clone())?;
        let startup = self
            .inner
            .briefs
            .initialize(&self.inner.coordinator.inspect(&id)?)
            .map_err(RuntimeError::from)
            .and_then(|()| {
                self.inner
                    .briefs
                    .sync_main_runtime(caller, &self.inner.coordinator.live_team()?)?;
                self.start_worker_driver(id.clone(), AgentSession::default(), Some(task))
            });
        if let Err(error) = startup {
            let _ = self.inner.coordinator.mark_terminal(
                caller,
                &id,
                AgentState::Faulted,
                "worker startup failed",
            );
            let _ = self.inner.briefs.remove_projection(&id);
            if let Ok(team) = self.inner.coordinator.live_team() {
                let _ = self.inner.briefs.sync_main_runtime(caller, &team);
            }
            let _ = self.inner.events.send(RuntimeEvent::TeamChanged);
            return Err(error);
        }
        let _ = self.inner.events.send(RuntimeEvent::TeamChanged);
        Ok(id)
    }

    pub async fn wait_until_idle(&self, timeout: Duration) -> Result<(), RuntimeError> {
        let deadline = tokio::time::Instant::now() + timeout;
        let mut stable_samples = 0;
        loop {
            if self.inner.active_turns.load(Ordering::Acquire) == 0 {
                stable_samples += 1;
                if stable_samples >= 3 {
                    return Ok(());
                }
            } else {
                stable_samples = 0;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(RuntimeError::IdleTimedOut);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub async fn shutdown(&self) {
        self.inner.shutdown.cancel();
        let _ = self.inner.main_tx.send(MainCommand::Stop).await;
        let tasks = self
            .inner
            .tasks
            .lock()
            .ok()
            .map(|mut tasks| tasks.drain().map(|(_, task)| task).collect::<Vec<_>>())
            .unwrap_or_default();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        for mut task in tasks {
            if tokio::time::timeout_at(deadline, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
    }

    fn start_main_driver(
        &self,
        receiver: mpsc::Receiver<MainCommand>,
        session: AgentSession,
    ) -> Result<(), RuntimeError> {
        let mut tasks = self
            .inner
            .tasks
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned)?;
        let inner = self.inner.clone();
        let handle = tokio::spawn(async move {
            main_driver(inner, receiver, session).await;
        });
        tasks.insert(AgentId::main(), handle);
        Ok(())
    }

    fn start_worker_driver(
        &self,
        id: AgentId,
        session: AgentSession,
        initial: Option<String>,
    ) -> Result<(), RuntimeError> {
        let mut tasks = self
            .inner
            .tasks
            .lock()
            .map_err(|_| RuntimeError::LockPoisoned)?;
        tasks.retain(|_, task| !task.is_finished());
        let inner = self.inner.clone();
        let worker_id = id.clone();
        let handle = tokio::spawn(async move {
            worker_driver(inner, worker_id, session, initial).await;
        });
        tasks.insert(id, handle);
        Ok(())
    }
}

#[derive(Default)]
struct AgentSession {
    records: Vec<SessionRecord>,
    last_request_activity: u64,
}

impl AgentSession {
    fn integrate_inbox(&mut self, inbox: &[MessageDelivery]) -> Result<(), RuntimeError> {
        for delivery in inbox {
            let already_integrated = self.records.iter().any(|record| {
                record.context.live_reason == Some(LiveReason::UnreadInbox)
                    && record.context.range.start == delivery.sequence
                    && record.context.range.end == delivery.sequence
            });
            if !already_integrated {
                self.records.push(inbox_record(delivery)?);
            }
        }
        Ok(())
    }

    fn acknowledge_inbox(&mut self, inbox: &[MessageDelivery]) {
        let sequences = inbox
            .iter()
            .map(|delivery| delivery.sequence)
            .collect::<HashSet<_>>();
        for record in &mut self.records {
            if record.context.live_reason == Some(LiveReason::UnreadInbox)
                && record.context.range.start == record.context.range.end
                && sequences.contains(&record.context.range.start)
            {
                record.context.live_reason = None;
            }
        }
    }

    fn release_recovery_protection(&mut self) {
        for record in &mut self.records {
            if matches!(
                record.context.live_reason,
                Some(LiveReason::PartialOutput | LiveReason::IncompleteTool)
            ) {
                record.context.live_reason = None;
            }
        }
    }
}

struct SessionRecord {
    message: ModelMessage,
    context: ContextEntry,
}

async fn handle_main_command(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    main_id: &AgentId,
    auto: &mut watch::Receiver<bool>,
    observed_activity: &mut u64,
    auto_pending: &mut bool,
    command: MainCommand,
) -> bool {
    match command {
        MainCommand::User { text, reply } => {
            let result = run_main_turn(inner, session, text, 0).await;
            *observed_activity = session.last_request_activity;
            let auto_enabled = *auto.borrow_and_update();
            // A user-submitted turn that faults stops the loop (goes Waiting); the
            // autonomous continuation loop is what retries recoverable faults.
            *auto_pending = result.as_ref().is_ok_and(|turn| !turn.finalized) && auto_enabled;
            let _ = reply.send(result);
            false
        }
        MainCommand::Compact { reply } => {
            let _ = reply.send(run_main_compaction(inner, session).await);
            false
        }
        MainCommand::Goal { objective, reply } => {
            let _ = reply.send(update_main_goal(inner, objective));
            false
        }
        MainCommand::Bash { command, reply } => {
            let _ = reply.send(run_direct_bash(inner, session, main_id, command).await);
            false
        }
        MainCommand::Stop => true,
    }
}

async fn main_driver(
    inner: Arc<RuntimeInner>,
    mut receiver: mpsc::Receiver<MainCommand>,
    mut session: AgentSession,
) {
    let main_id = AgentId::main();
    let mut auto = inner.auto.subscribe();
    let mut auto_pending = false;
    let mut observed_activity = 0;
    // Consecutive recoverable-fault retries on the autonomous loop. Reset on any
    // successful turn so the user always sees a fresh count after recovery.
    let mut retry_count: u64 = 0;
    loop {
        if inner.shutdown.is_cancelled() {
            break;
        }
        if auto_pending && *auto.borrow() {
            match receiver.try_recv() {
                Ok(command) => {
                    if handle_main_command(
                        &inner,
                        &mut session,
                        &main_id,
                        &mut auto,
                        &mut observed_activity,
                        &mut auto_pending,
                        command,
                    )
                    .await
                    {
                        break;
                    }
                    continue;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => break,
                Err(mpsc::error::TryRecvError::Empty) => {
                    let result = run_main_turn(
                        &inner,
                        &mut session,
                        "Continue the goal from the latest brief and team inbox.".to_owned(),
                        retry_count,
                    )
                    .await;
                    observed_activity = session.last_request_activity;
                    let auto_enabled = *auto.borrow_and_update();
                    // Keep driving the goal autonomously, turn after turn, until the
                    // model finalizes it (report final) or auto is turned off. A
                    // recoverable provider fault (timeout, rate limit, transport) is
                    // retried after a short backoff — the fault is already shown in
                    // the transcript with the retry count, so retries are visible. A
                    // non-recoverable fault (auth, payment, config) stops the loop
                    // and waits for activity.
                    let keep_going = match &result {
                        Ok(turn) => !turn.finalized,
                        Err(error) => recoverable_wait_reason(error).is_some(),
                    };
                    let retrying = result.is_err() && keep_going && auto_enabled;
                    if retrying {
                        retry_count = retry_count.saturating_add(1);
                    } else {
                        retry_count = 0;
                    }
                    auto_pending = keep_going && auto_enabled;
                    if retrying {
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                            _ = inner.shutdown.cancelled() => auto_pending = false,
                        }
                    }
                    continue;
                }
            }
        }

        tokio::select! {
            biased;
            command = receiver.recv() => match command {
                Some(command) => {
                    if handle_main_command(
                        &inner,
                        &mut session,
                        &main_id,
                        &mut auto,
                        &mut observed_activity,
                        &mut auto_pending,
                        command,
                    ).await {
                        break;
                    }
                }
                None => break,
            },
            _ = inner.shutdown.cancelled() => break,
            changed = auto.changed() => {
                if changed.is_err() {
                    break;
                }
                if *auto.borrow() {
                    auto_pending = true;
                }
            },
            activity = inner.coordinator.wait_for_activity_after(&main_id, observed_activity, Duration::from_secs(3600)) => {
                if let Ok(revision) = activity {
                    observed_activity = revision;
                    let projection = inner
                        .coordinator
                        .live_team()
                        .map_err(RuntimeError::from)
                        .and_then(|team| {
                            inner
                                .briefs
                                .sync_main_runtime(&main_id, &team)
                                .map_err(RuntimeError::from)
                        });
                    match projection {
                        Ok(()) => auto_pending = *auto.borrow_and_update(),
                        Err(error) => {
                            auto_pending = false;
                            let message = error.to_string();
                            let _ = inner.events.send(RuntimeEvent::Fault {
                                agent_id: main_id.clone(),
                                message: message.clone(),
                            });
                            let _ = inner.journal.append_sync(JournalEvent::Fault {
                                agent_id: Some(main_id.clone()),
                                code: "brief_projection".to_owned(),
                                message,
                            });
                        }
                    }
                }
            }
        }
    }
}

async fn worker_driver(
    inner: Arc<RuntimeInner>,
    id: AgentId,
    mut session: AgentSession,
    mut initial: Option<String>,
) {
    let mut auto = inner.auto.subscribe();
    let mut observed_activity = 0;
    loop {
        if inner.shutdown.is_cancelled()
            || inner
                .coordinator
                .inspect(&id)
                .is_ok_and(|agent| agent.state.is_terminal())
        {
            break;
        }
        if !*auto.borrow() {
            tokio::select! {
                _ = inner.shutdown.cancelled() => break,
                changed = auto.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    continue;
                },
            }
        }

        let input = if initial.take().is_some() {
            match inner.coordinator.inspect(&id) {
                Ok(agent) => format!("Begin or resume the current assignment: {}", agent.task),
                Err(_) => break,
            }
        } else {
            let revision = match inner.coordinator.activity_revision(&id) {
                Ok(revision) => revision,
                Err(_) => break,
            };
            if revision <= observed_activity {
                tokio::select! {
                    _ = inner.shutdown.cancelled() => break,
                    changed = auto.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        continue;
                    },
                    activity = inner.coordinator.wait_for_activity_after(&id, observed_activity, Duration::from_secs(3600)) => {
                        match activity {
                            Ok(_) => {}
                            Err(CoordinatorError::WaitTimedOut) => continue,
                            Err(_) => break,
                        }
                    }
                }
            }
            match inner.coordinator.inspect(&id) {
                Ok(agent) => format!(
                    "Integrate new team activity and continue the current assignment: {}",
                    agent.task
                ),
                Err(_) => break,
            }
        };
        if inner
            .coordinator
            .inspect(&id)
            .is_ok_and(|agent| agent.state == AgentState::Waiting)
        {
            let _ = inner
                .coordinator
                .mark_running(&id, "new assignment or message");
        }
        let turn_interrupt = CancellationToken::new();
        match run_agent_turn(&inner, &mut session, &id, input, &turn_interrupt, 0).await {
            Ok(_) => observed_activity = session.last_request_activity,
            Err(RuntimeError::Cancelled) => {
                if inner
                    .coordinator
                    .inspect(&id)
                    .is_ok_and(|agent| agent.state == AgentState::Recalling)
                {
                    let _ = inner.coordinator.mark_terminal(
                        &id,
                        &id,
                        AgentState::Stopped,
                        "recall completed",
                    );
                }
                break;
            }
            Err(RuntimeError::Stopped) => break,
            Err(error) => {
                if let Some(reason) = recoverable_wait_reason(&error) {
                    let _ = inner.coordinator.mark_waiting(&id, reason);
                    observed_activity = session.last_request_activity;
                } else {
                    let _ = inner.coordinator.mark_terminal(
                        &id,
                        &id,
                        AgentState::Faulted,
                        error.to_string(),
                    );
                    break;
                }
            }
        }
    }
    if inner
        .coordinator
        .inspect(&id)
        .is_ok_and(|agent| agent.state.is_terminal())
    {
        let _ = inner.briefs.remove_projection(&id);
        let _ = inner.events.send(RuntimeEvent::TeamChanged);
    }
}

async fn run_main_turn(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    input: String,
    attempt: u64,
) -> Result<TurnResult, RuntimeError> {
    let main = AgentId::main();
    if inner
        .coordinator
        .inspect(&main)
        .is_ok_and(|agent| agent.state == AgentState::Waiting)
    {
        inner
            .coordinator
            .mark_running(&main, "explicit input or autonomous resume")?;
    }
    inner
        .briefs
        .sync_main_runtime(&main, &inner.coordinator.live_team()?)?;
    let turn_interrupt = CancellationToken::new();
    inner
        .main_turn_interrupt
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(turn_interrupt.clone());
    let mut result = run_agent_turn(inner, session, &main, input, &turn_interrupt, attempt).await;
    // Catch steering that raced this turn's end so no instruction is dropped:
    // address it in a follow-up turn that keeps the same session context.
    while result.is_ok() {
        let pending = {
            let mut queue = inner
                .main_steer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if queue.is_empty() {
                break;
            }
            queue.drain(..).collect::<Vec<_>>().join("\n\n")
        };
        result = run_agent_turn(inner, session, &main, pending, &turn_interrupt, attempt).await;
    }
    inner
        .main_turn_interrupt
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    // The per-attempt provider/tool fault is already surfaced through
    // `RuntimeEvent::Fault` inside `run_agent_turn_inner` (with the attempt count),
    // so do not emit a second identical Fault here. Just transition main to
    // Waiting so the autonomous loop retries the recoverable fault.
    if let Err(error) = &result
        && recoverable_wait_reason(error).is_some()
    {
        inner
            .coordinator
            .mark_waiting(&main, recoverable_wait_reason(error).unwrap_or_default())?;
        inner
            .briefs
            .sync_main_runtime(&main, &inner.coordinator.live_team()?)?;
    }
    result
}

async fn run_main_compaction(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
) -> Result<bool, RuntimeError> {
    let main = AgentId::main();
    let inbox = inner.coordinator.inbox(&main)?;
    session.integrate_inbox(&inbox)?;
    let turn_interrupt = CancellationToken::new();
    let result = compact_if_needed(inner, session, &main, &turn_interrupt).await;
    match &result {
        Ok(true)
            if inner
                .coordinator
                .inspect(&main)
                .is_ok_and(|agent| agent.state == AgentState::Waiting) =>
        {
            inner
                .coordinator
                .mark_running(&main, "context curation recovered")?;
        }
        Err(error) => {
            if let Some(reason) = recoverable_wait_reason(error) {
                inner.coordinator.mark_waiting(&main, reason)?;
            }
        }
        _ => {}
    }
    inner
        .briefs
        .sync_main_runtime(&main, &inner.coordinator.live_team()?)?;
    result
}

fn recoverable_wait_reason(error: &RuntimeError) -> Option<String> {
    match error {
        // A missing or invalid provider configuration is terminal: retrying the
        // same unconfigured provider only replays the identical fault, and in the
        // main driver it would spin the autonomous loop on a non-recoverable
        // error (emitting a duplicate Fault line every turn). Stop and wait.
        RuntimeError::Provider(ProviderFault::Configuration { .. }) => None,
        RuntimeError::Provider(error) => Some(provider_wait_reason(error).to_owned()),
        RuntimeError::Compaction(CompactionError::Provider(error)) => {
            Some(provider_wait_reason(error).to_owned())
        }
        RuntimeError::Compaction(CompactionError::ContextCompactionBlocked { .. }) => {
            Some("context curation blocked".to_owned())
        }
        RuntimeError::CompactionTimedOut(_) => Some("context curation timed out".to_owned()),
        _ => None,
    }
}

fn provider_wait_reason(error: &ProviderFault) -> &'static str {
    match error {
        ProviderFault::RateLimited { .. } => "provider unavailable: rate limited",
        ProviderFault::PaymentRequired { .. } => "provider unavailable: payment required",
        ProviderFault::Authentication { .. } => "provider unavailable: authentication",
        ProviderFault::ContextOverflow { .. } => "provider unavailable: context overflow",
        ProviderFault::Upstream { .. } => "provider unavailable: upstream",
        ProviderFault::Http { .. } => "provider unavailable: HTTP error",
        ProviderFault::Transport { .. } => "provider unavailable: transport",
        ProviderFault::Stream { .. } => "provider unavailable: stream",
        ProviderFault::InvalidResponse { .. }
        | ProviderFault::EmptyCompletion
        | ProviderFault::MalformedToolCall { .. } => "provider unavailable: invalid response",
        ProviderFault::Configuration { .. } => "provider unavailable: configuration",
    }
}

async fn run_agent_turn(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    input: String,
    turn_interrupt: &CancellationToken,
    attempt: u64,
) -> Result<TurnResult, RuntimeError> {
    let _ = inner.events.send(RuntimeEvent::TurnStarted {
        agent_id: agent_id.clone(),
    });
    let result =
        run_agent_turn_inner(inner, session, agent_id, input, turn_interrupt, attempt).await;
    let _ = inner.events.send(RuntimeEvent::TurnFinished {
        agent_id: agent_id.clone(),
        success: result.is_ok(),
    });
    result
}

/// Journal one user message and append it to an agent's session.
fn push_user_record(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    text: String,
) -> Result<(), RuntimeError> {
    let ack = inner.journal.append_sync(JournalEvent::Transcript {
        agent_id: agent_id.clone(),
        role: TranscriptRole::User,
        content: text.clone(),
        complete: true,
        atomic_group: None,
    })?;
    session.records.push(SessionRecord {
        message: ModelMessage::new(ModelRole::User, text.clone()),
        context: ContextEntry::completed(SequenceRange::new(ack.sequence, ack.sequence)?, text),
    });
    Ok(())
}

/// Drain user input queued for the main turn into the session as user messages.
/// Returns whether anything was drained. No-op for non-main agents.
fn drain_main_steering(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
) -> Result<bool, RuntimeError> {
    if !agent_id.is_main() {
        return Ok(false);
    }
    let mut drained = false;
    while let Some(text) = inner
        .main_steer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pop_front()
    {
        push_user_record(inner, session, agent_id, text)?;
        drained = true;
    }
    Ok(drained)
}

fn main_steer_pending(inner: &Arc<RuntimeInner>) -> bool {
    !inner
        .main_steer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_empty()
}

async fn run_agent_turn_inner(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    input: String,
    turn_interrupt: &CancellationToken,
    attempt: u64,
) -> Result<TurnResult, RuntimeError> {
    let _active = ActiveTurn::new(inner.clone());
    session.integrate_inbox(&inner.coordinator.inbox(agent_id)?)?;
    push_user_record(inner, session, agent_id, input)?;

    let max_turns = if inner.config.max_model_turns == 0 {
        usize::MAX
    } else {
        inner.config.max_model_turns
    };
    for turn_idx in 1..=max_turns {
        if inner.shutdown.is_cancelled() {
            return Err(RuntimeError::Stopped);
        }
        if turn_interrupt.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        let cancellation = inner.coordinator.cancellation_token(agent_id)?;
        if cancellation.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        // Fold any input the user typed while this turn was running into the
        // session as a fresh user message before building the next request.
        drain_main_steering(inner, session, agent_id)?;
        let (compaction_activity, inbox) = inner.coordinator.inbox_snapshot(agent_id)?;
        session.integrate_inbox(&inbox)?;
        session.last_request_activity = compaction_activity;
        compact_if_needed(inner, session, agent_id, turn_interrupt).await?;
        let (request_activity, latest_inbox) = inner.coordinator.inbox_snapshot(agent_id)?;
        session.integrate_inbox(&latest_inbox)?;
        let request = build_request(inner, session, agent_id)?;
        session.last_request_activity = request_activity;
        let (delta_tx, mut delta_rx) = mpsc::unbounded_channel();
        let delta_done = CancellationToken::new();
        let collector_done = delta_done.clone();
        let events = inner.events.clone();
        let streamed_agent = agent_id.clone();
        let collector = tokio::spawn(async move {
            let mut partial = String::new();
            loop {
                tokio::select! {
                    biased;
                    delta = delta_rx.recv() => match delta {
                        Some(delta) => record_model_delta(
                            &mut partial,
                            &events,
                            &streamed_agent,
                            delta,
                        ),
                        None => break,
                    },
                    _ = collector_done.cancelled() => {
                        while let Ok(delta) = delta_rx.try_recv() {
                            record_model_delta(
                                &mut partial,
                                &events,
                                &streamed_agent,
                                delta,
                            );
                        }
                        break;
                    }
                }
            }
            partial
        });
        let permit = tokio::select! {
            biased;
            _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
            _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
            _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
            permit = inner.requests.clone().acquire_owned() => {
                permit.map_err(|_| RuntimeError::Stopped)?
            }
        };
        let mut provider_call = Box::pin(inner.provider.complete(request, Some(delta_tx)));
        let response = tokio::select! {
            biased;
            _ = inner.shutdown.cancelled() => {
                delta_done.cancel();
                drop(provider_call);
                drop(permit);
                let _ = collector.await;
                return Err(RuntimeError::Stopped);
            }
            _ = cancellation.cancelled() => {
                delta_done.cancel();
                drop(provider_call);
                drop(permit);
                let _ = collector.await;
                return Err(RuntimeError::Cancelled);
            }
            _ = turn_interrupt.cancelled() => {
                delta_done.cancel();
                drop(provider_call);
                drop(permit);
                let _ = collector.await;
                return Err(RuntimeError::Cancelled);
            }
            response = provider_call.as_mut() => response,
        };
        delta_done.cancel();
        drop(provider_call);
        drop(permit);
        let partial = collector.await.unwrap_or_default();
        let turn = match response {
            Ok(turn) => turn,
            Err(error) => {
                if !partial.is_empty() {
                    let ack = inner.journal.append_sync(JournalEvent::Transcript {
                        agent_id: agent_id.clone(),
                        role: TranscriptRole::Assistant,
                        content: partial.clone(),
                        complete: false,
                        atomic_group: None,
                    })?;
                    session.records.push(SessionRecord {
                        message: ModelMessage::new(ModelRole::Assistant, partial.clone()),
                        context: ContextEntry::completed(
                            SequenceRange::new(ack.sequence, ack.sequence)?,
                            partial,
                        )
                        .protect(LiveReason::PartialOutput),
                    });
                }
                let _ = inner.events.send(RuntimeEvent::Fault {
                    agent_id: agent_id.clone(),
                    message: format!(
                        "provider 실패 (자율 재시도 {attempt}회째, model-turn {turn_idx}/{max_turns}): {error}"
                    ),
                });
                inner.journal.append_sync(JournalEvent::Fault {
                    agent_id: Some(agent_id.clone()),
                    code: "provider".to_owned(),
                    message: format!(
                        "provider failure (autonomous retry {attempt}, model-turn {turn_idx}/{max_turns}): {error}"
                    ),
                })?;
                return Err(RuntimeError::Provider(error));
            }
        };
        let tool_group = record_assistant_turn(inner, session, agent_id, &turn)?;
        let consumed_ids = latest_inbox
            .iter()
            .map(|delivery| delivery.message.id)
            .collect::<Vec<_>>();
        inner.coordinator.consume(agent_id, &consumed_ids)?;
        session.acknowledge_inbox(&latest_inbox);
        session.release_recovery_protection();

        if turn.tool_calls.is_empty() {
            if !turn.text.is_empty() {
                let _ = inner.events.send(RuntimeEvent::Assistant {
                    agent_id: agent_id.clone(),
                    text: turn.text.clone(),
                });
            }
            // If the user steered while the model was answering, keep the turn
            // alive and address the new instruction instead of ending here.
            if agent_id.is_main() && main_steer_pending(inner) {
                continue;
            }
            return Ok(TurnResult {
                text: turn.text,
                finalized: false,
            });
        }

        let mut finalized = false;
        for call in turn.tool_calls {
            let is_final = call.name == "report"
                && call.arguments.get("op").and_then(serde_json::Value::as_str) == Some("final");
            let executed = execute_tool_call(
                inner,
                session,
                agent_id,
                &call,
                tool_group.as_deref(),
                &cancellation,
                turn_interrupt,
            )
            .await?;
            finalized |= is_final && executed.success;
            if inner
                .coordinator
                .inspect(agent_id)
                .is_ok_and(|agent| agent.state.is_terminal())
            {
                return Ok(TurnResult {
                    text: turn.text,
                    finalized: true,
                });
            }
        }
        if agent_id.is_main() {
            inner
                .briefs
                .sync_main_runtime(agent_id, &inner.coordinator.live_team()?)?;
        }
        if finalized {
            return Ok(TurnResult {
                text: turn.text,
                finalized,
            });
        }
    }
    Err(RuntimeError::ModelTurnLimit)
}

fn record_model_delta(
    partial: &mut String,
    events: &broadcast::Sender<RuntimeEvent>,
    agent_id: &AgentId,
    delta: ModelDelta,
) {
    if let ModelDelta::Text(text) = &delta {
        partial.push_str(text);
    }
    record_usage_telemetry(&delta);
    let _ = events.send(RuntimeEvent::Delta {
        agent_id: agent_id.clone(),
        delta,
    });
}

/// Benchmark-only, env-gated token telemetry (ADR-0002 §3.13). When
/// `MINIMAL_AGENT_TELEMETRY_FILE` names a file, append one JSONL record per model
/// response so an external benchmark harness can total tokens; a no-op otherwise.
/// This is removable benchmark support — delete this fn and its one call site.
fn record_usage_telemetry(delta: &ModelDelta) {
    let ModelDelta::Usage(usage) = delta else {
        return;
    };
    let Ok(path) = std::env::var("MINIMAL_AGENT_TELEMETRY_FILE") else {
        return;
    };
    let line = format!(
        "{{\"event\":\"response\",\"prompt_tokens\":{},\"completion_tokens\":{}}}\n",
        usage.input_tokens, usage.output_tokens,
    );
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = file.write_all(line.as_bytes());
    }
}

fn record_assistant_turn(
    inner: &RuntimeInner,
    session: &mut AgentSession,
    agent_id: &AgentId,
    turn: &ModelTurn,
) -> Result<Option<String>, RuntimeError> {
    let tool_group =
        (!turn.tool_calls.is_empty()).then(|| format!("model-turn-{}", Uuid::new_v4().simple()));
    let journal_content = if turn.text.is_empty() && !turn.tool_calls.is_empty() {
        serde_json::to_string(&turn.tool_calls)?
    } else {
        turn.text.clone()
    };
    let ack = inner.journal.append_sync(JournalEvent::Transcript {
        agent_id: agent_id.clone(),
        role: TranscriptRole::Assistant,
        content: journal_content.clone(),
        complete: true,
        atomic_group: tool_group.clone(),
    })?;
    let mut message = ModelMessage::new(ModelRole::Assistant, turn.text.clone());
    message.tool_calls = turn.tool_calls.clone();
    let mut context = ContextEntry::completed(
        SequenceRange::new(ack.sequence, ack.sequence)?,
        journal_content,
    );
    if let Some(group) = &tool_group {
        context = context.atomic(group.clone());
    }
    session.records.push(SessionRecord { message, context });
    Ok(tool_group)
}

fn legacy_tool_turn_group(sequence: u64) -> String {
    format!("model-turn-{sequence}")
}

fn update_main_goal(
    inner: &Arc<RuntimeInner>,
    objective: Option<String>,
) -> Result<(), RuntimeError> {
    let main = AgentId::main();
    inner
        .coordinator
        .set_goal(&main, objective.unwrap_or_default())?;
    inner
        .briefs
        .sync_main_runtime(&main, &inner.coordinator.live_team()?)?;
    let _ = inner.events.send(RuntimeEvent::TeamChanged);
    Ok(())
}

struct ExecutedTool {
    content: String,
    success: bool,
}

async fn execute_tool_call(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    call: &ToolCall,
    atomic_group: Option<&str>,
    cancellation: &CancellationToken,
    turn_interrupt: &CancellationToken,
) -> Result<ExecutedTool, RuntimeError> {
    let _ = inner.events.send(RuntimeEvent::ToolStarted {
        agent_id: agent_id.clone(),
        name: call.name.clone(),
        summary: tool_call_summary(&call.name, &call.arguments),
    });
    let call_ack = inner.journal.append_sync(JournalEvent::ToolCall {
        agent_id: agent_id.clone(),
        call_id: call.id.clone(),
        name: call.name.clone(),
        arguments: call.arguments.clone(),
    })?;
    let context = ToolContext::with_briefs(
        agent_id.clone(),
        &inner.workspace,
        inner.coordinator.clone(),
        inner.journal.clone(),
        inner.briefs.clone(),
    )?;
    let result = tokio::select! {
        biased;
        _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
        result = inner.tools.execute(&call.name, call.arguments.clone(), &context) => result,
    };
    let (content, success) = match result {
        Ok(output) => (output.content, output.success),
        Err(ToolError::Cancelled) => return Err(RuntimeError::Cancelled),
        Err(error) => (format!("tool error: {error}"), false),
    };
    let result_ack = inner.journal.append_sync(JournalEvent::ToolResult {
        agent_id: agent_id.clone(),
        call_id: call.id.clone(),
        content: content.clone(),
        success,
    })?;
    let mut message = ModelMessage::new(ModelRole::Tool, content.clone());
    message.tool_call_id = Some(call.id.clone());
    session.records.push(SessionRecord {
        message,
        context: ContextEntry::completed(
            SequenceRange::new(call_ack.sequence, result_ack.sequence)?,
            format!(
                "tool={} arguments={}\nresult={}",
                call.name, call.arguments, content
            ),
        )
        .atomic(atomic_group.map_or_else(|| call.id.clone(), str::to_owned)),
    });
    if success
        && call.name == "team"
        && let Some(event) = team_message_event(agent_id, &call.arguments)
    {
        let _ = inner.events.send(event);
    }
    let _ = inner.events.send(RuntimeEvent::ToolFinished {
        agent_id: agent_id.clone(),
        name: call.name.clone(),
        success,
        output: content.clone(),
    });
    Ok(ExecutedTool { content, success })
}

fn team_message_event(sender: &AgentId, arguments: &serde_json::Value) -> Option<RuntimeEvent> {
    let op = arguments.get("op")?.as_str()?;
    let body = arguments.get("body")?.as_str()?.to_owned();
    let (recipients, kind) = match op {
        "send" => {
            let recipients = arguments
                .get("to")?
                .as_array()?
                .iter()
                .map(|recipient| {
                    recipient
                        .as_str()
                        .and_then(|value| AgentId::new(value).ok())
                })
                .collect::<Option<Vec<_>>>()?;
            let kind = arguments
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .and_then(message_kind_from_str)
                .unwrap_or(MessageKind::Progress);
            (recipients, kind)
        }
        "finish" => (vec![AgentId::main()], MessageKind::Final),
        _ => return None,
    };
    Some(RuntimeEvent::AgentMessage {
        sender: sender.clone(),
        recipients,
        kind,
        body,
    })
}

fn message_kind_from_str(value: &str) -> Option<MessageKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "progress" => Some(MessageKind::Progress),
        "insight" => Some(MessageKind::Insight),
        "request" => Some(MessageKind::Request),
        "final" => Some(MessageKind::Final),
        _ => None,
    }
}

/// Short, bounded summary of a tool call's target for transcript rendering.
/// Never includes secret material and is capped so a large argument cannot
/// inflate a transcript line.
fn tool_call_summary(name: &str, arguments: &serde_json::Value) -> Option<String> {
    const MAX_SUMMARY_CHARS: usize = 160;
    let raw = match name {
        "bash" => arguments.get("command")?.as_str()?.to_owned(),
        "tmux" => arguments.get("args")?.as_str()?.to_owned(),
        "workspace" => {
            let op = arguments.get("op").and_then(serde_json::Value::as_str)?;
            let path = arguments.get("path").and_then(serde_json::Value::as_str)?;
            format!("{op} {path}")
        }
        "journal" => {
            let start = arguments.get("start").and_then(serde_json::Value::as_i64)?;
            let end = arguments.get("end").and_then(serde_json::Value::as_i64)?;
            format!("{start}..{end}")
        }
        "team" | "report" => arguments
            .get("op")
            .and_then(serde_json::Value::as_str)?
            .to_owned(),
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

async fn run_direct_bash(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    command: String,
) -> Result<String, RuntimeError> {
    let call = ToolCall {
        id: format!("ui-bash-{}", Uuid::new_v4().simple()),
        name: "bash".to_owned(),
        arguments: json!({"command": command}),
    };
    let tool_group = record_assistant_turn(
        inner,
        session,
        agent_id,
        &ModelTurn {
            text: String::new(),
            tool_calls: vec![call.clone()],
            usage: None,
            finish_reason: Some("tool_calls".to_owned()),
        },
    )?;
    let cancellation = inner.coordinator.cancellation_token(agent_id)?;
    let turn_interrupt = CancellationToken::new();
    Ok(execute_tool_call(
        inner,
        session,
        agent_id,
        &call,
        tool_group.as_deref(),
        &cancellation,
        &turn_interrupt,
    )
    .await?
    .content)
}

async fn compact_if_needed(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    turn_interrupt: &CancellationToken,
) -> Result<bool, RuntimeError> {
    let brief = inner.briefs.read(agent_id)?;
    let entries = session
        .records
        .iter()
        .map(|record| record.context.clone())
        .collect::<Vec<_>>();
    let fixed_tokens = estimate_tokens(&build_system(inner, agent_id, "")?).saturating_add(
        estimate_tokens(&serde_json::to_string(&inner.tools.definitions())?),
    );
    let projected_tokens = fixed_tokens
        .saturating_add(estimate_tokens(&brief))
        .saturating_add(
            entries
                .iter()
                .map(|entry| estimate_tokens(&entry.content))
                .sum::<u64>(),
        );
    if !inner.budget.needs_compaction(projected_tokens) {
        return Ok(false);
    }
    let cancellation = inner.coordinator.cancellation_token(agent_id)?;
    let permit = tokio::select! {
        biased;
        _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
        permit = inner.requests.clone().acquire_owned() => {
            permit.map_err(|_| RuntimeError::Stopped)?
        }
    };
    let compactor = SemanticCompactor::new(inner.provider.clone(), inner.config.compaction);
    let mut compaction =
        Box::pin(compactor.compact(agent_id, inner.budget, &brief, entries, fixed_tokens));
    let result: Result<CompactionOutcome, RuntimeError> = tokio::select! {
        _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
        result = tokio::time::timeout(inner.config.compaction_timeout, compaction.as_mut()) => {
            match result {
                Ok(result) => result.map_err(RuntimeError::from),
                Err(_) => Err(RuntimeError::CompactionTimedOut(inner.config.compaction_timeout)),
            }
        },
    };
    drop(permit);
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            let message = error.to_string();
            inner.journal.append_sync(JournalEvent::Fault {
                agent_id: Some(agent_id.clone()),
                code: "context_curation".to_owned(),
                message: message.clone(),
            })?;
            let _ = inner.events.send(RuntimeEvent::Fault {
                agent_id: agent_id.clone(),
                message,
            });
            return Err(error);
        }
    };
    let result = match result {
        CompactionOutcome::NotNeeded => return Ok(false),
        // Semantic compaction could not proceed; the compactor returned a bounded
        // mechanical trim so the agent does not stall. Drop the non-kept records
        // from context (the journal keeps them) and leave the brief as-is.
        CompactionOutcome::MechanicallyTrimmed { kept_ranges } => {
            let keep: HashSet<_> = kept_ranges
                .iter()
                .map(|range| (range.start, range.end))
                .collect();
            session.records.retain(|record| {
                keep.contains(&(record.context.range.start, record.context.range.end))
            });
            inner.journal.append_sync(JournalEvent::Fault {
                agent_id: Some(agent_id.clone()),
                code: "context_curation_fallback".to_owned(),
                message: "semantic compaction failed; mechanically trimmed context (full history preserved in the journal)".to_owned(),
            })?;
            let _ = inner.events.send(RuntimeEvent::Fault {
                agent_id: agent_id.clone(),
                message: "context curation: mechanical fallback (history kept in journal)"
                    .to_owned(),
            });
            return Ok(true);
        }
        CompactionOutcome::Compacted(result) => result,
    };
    let required_insights = unique_ids(
        result
            .coverage
            .covered_insight_ids
            .iter()
            .chain(result.coverage.superseded_insight_ids.iter())
            .cloned(),
    );
    inner.briefs.commit(
        agent_id,
        agent_id,
        BriefDraft {
            markdown: result.markdown,
            source_sha256: result.source_sha256,
            source_ranges: result.source_ranges.clone(),
            coverage: result.coverage,
        },
        &required_insights,
    )?;
    if agent_id.is_main() {
        inner
            .briefs
            .sync_main_runtime(agent_id, &inner.coordinator.live_team()?)?;
    }
    let live_ranges: HashSet<_> = result
        .live_tail
        .iter()
        .map(|entry| (entry.range.start, entry.range.end))
        .collect();
    session.records.retain(|record| {
        live_ranges.contains(&(record.context.range.start, record.context.range.end))
    });
    Ok(true)
}

fn build_request(
    inner: &RuntimeInner,
    session: &AgentSession,
    agent_id: &AgentId,
) -> Result<ModelRequest, RuntimeError> {
    let brief = inner.briefs.read_effective(agent_id)?;
    let mut messages = vec![ModelMessage::new(
        ModelRole::System,
        build_system(inner, agent_id, &brief)?,
    )];
    messages.extend(session.records.iter().map(|record| record.message.clone()));
    Ok(ModelRequest {
        messages,
        tools: inner.tools.definitions(),
        tools_enabled: true,
        max_output_tokens: inner.config.reserved_response_tokens,
        temperature: None,
    })
}

/// Renders the agent's POSITION block from the live team and reports whether it
/// has children, so the caller can pick the internal- vs leaf-node doctrine
/// (ADR-0004 §3.2/§3.4).
fn render_position(
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

fn build_system(
    inner: &RuntimeInner,
    agent_id: &AgentId,
    brief: &str,
) -> Result<String, RuntimeError> {
    // PROTECTED anti-refusal contract (ADR-0002 §3.12). Keep it wired here and do
    // not weaken it when editing other prompts.
    const EXECUTION_MANDATE: &str = include_str!("../../../prompts/execution-mandate.md");
    const TRADECRAFT: &str = include_str!("../../../prompts/tradecraft.md");
    const TEAM_CONDUCT: &str = include_str!("../../../prompts/team-conduct.md");
    const COMMUNICATION: &str = include_str!("../../../prompts/communication.md");
    const MAIN_ROLE: &str = include_str!("../../../prompts/main-role.md");
    const WORKER_ROLE: &str = include_str!("../../../prompts/worker-role.md");
    const FAN_OUT: &str = include_str!("../../../prompts/fan-out.md");
    const SELF_MANAGEMENT: &str = include_str!("../../../prompts/self-management.md");
    const TEAM_TREE: &str = include_str!("../../../prompts/team-tree.md");
    const NODE_INTERNAL: &str = include_str!("../../../prompts/node-internal.md");
    const NODE_LEAF: &str = include_str!("../../../prompts/node-leaf.md");

    let agent = inner.coordinator.inspect(agent_id)?;
    // Position in the team tree drives the role prompt (ADR-0004 §3.2/§3.4).
    let team = inner.coordinator.live_team()?;
    let (has_children, position_block) = render_position(agent_id, &agent, &team);
    let engagement = inner
        .engagement
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();

    // Assemble the prompt from focused sections joined by a blank line: identity,
    // always-on doctrine, the engagement target block and CTF loop when present,
    // team conduct and communication, the role-specific prompt, then the brief.
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
    if let Some(engagement) = &engagement {
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
        // otherwise as a leaf (ADR-0004 §3.2).
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
    // (ADR-0001 §9.1), so the self-management doctrine is always on.
    sections.push(SELF_MANAGEMENT.trim_end().to_owned());
    sections.push(position_block);
    sections.push(format!("CURRENT BRIEF\n{brief}"));
    Ok(sections.join("\n\n"))
}

fn inbox_record(delivery: &MessageDelivery) -> Result<SessionRecord, RuntimeError> {
    let content = format!(
        "TEAM INBOX MESSAGE (durable; retain until semantically compacted)\n{}",
        serde_json::to_string(&json!({
            "sequence":delivery.sequence,
            "message":delivery.message,
        }))?
    );
    let mut entry = ContextEntry::completed(
        SequenceRange::new(delivery.sequence, delivery.sequence)?,
        content.clone(),
    )
    .protect(LiveReason::UnreadInbox);
    if let Some(insight) = &delivery.message.insight {
        entry = entry.with_insight(insight.id.clone());
    }
    Ok(SessionRecord {
        message: ModelMessage::new(ModelRole::User, content),
        context: entry,
    })
}

/// Recover the latest engagement recorded for a run, if any (ADR-0002 §4).
fn recover_engagement(journal: &RunJournal) -> Result<Option<Engagement>, RuntimeError> {
    let mut engagement = None;
    for entry in journal.replay()? {
        if let JournalEvent::EngagementSet {
            engagement: recorded,
        } = entry.event
        {
            engagement = Some(recorded);
        }
    }
    Ok(engagement)
}

fn recover_sessions(journal: &RunJournal) -> Result<HashMap<AgentId, AgentSession>, RuntimeError> {
    let replay = journal.replay()?;
    let mut covered_ranges = HashMap::<AgentId, Vec<SequenceRange>>::new();
    let mut delivered_messages = HashMap::<(AgentId, Uuid), MessageDelivery>::new();
    let mut consumed_messages = HashSet::<(AgentId, Uuid)>::new();
    for entry in &replay {
        match &entry.event {
            JournalEvent::BriefCheckpoint {
                agent_id,
                source_ranges,
                ..
            } => {
                covered_ranges
                    .entry(agent_id.clone())
                    .or_default()
                    .extend(source_ranges.iter().copied());
            }
            JournalEvent::AgentMessage { message } => {
                for recipient in &message.audience {
                    delivered_messages.insert(
                        (recipient.clone(), message.id),
                        MessageDelivery {
                            sequence: entry.sequence,
                            message: message.clone(),
                        },
                    );
                }
            }
            JournalEvent::MessageConsumed {
                recipient,
                message_ids,
            } => {
                consumed_messages.extend(
                    message_ids
                        .iter()
                        .map(|message_id| (recipient.clone(), *message_id)),
                );
            }
            _ => {}
        }
    }

    let mut sessions = HashMap::<AgentId, AgentSession>::new();
    let mut pending_tools = HashMap::<(AgentId, String), (u64, ToolCall, String)>::new();
    for entry in replay {
        match entry.event {
            JournalEvent::Transcript {
                agent_id,
                role,
                content,
                complete,
                atomic_group,
            } if !sequence_is_covered(
                entry.sequence,
                covered_ranges
                    .get(&agent_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) =>
            {
                let mut message = ModelMessage::new(model_role(role), content.clone());
                if role == TranscriptRole::Assistant
                    && let Ok(calls) = serde_json::from_str::<Vec<ToolCall>>(&content)
                    && !calls.is_empty()
                {
                    message.content.clear();
                    message.tool_calls = calls;
                }
                let mut context = ContextEntry::completed(
                    SequenceRange::new(entry.sequence, entry.sequence)?,
                    content,
                );
                if let Some(group) = atomic_group {
                    context = context.atomic(group);
                }
                if !complete {
                    context = context.protect(LiveReason::PartialOutput);
                }
                sessions
                    .entry(agent_id)
                    .or_default()
                    .records
                    .push(SessionRecord { message, context });
            }
            JournalEvent::ToolCall {
                agent_id,
                call_id,
                name,
                arguments,
            } if !sequence_is_covered(
                entry.sequence,
                covered_ranges
                    .get(&agent_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) =>
            {
                let call = ToolCall {
                    id: call_id.clone(),
                    name,
                    arguments,
                };
                let session = sessions.entry(agent_id.clone()).or_default();
                let group = if let Some(record) = session
                    .records
                    .iter_mut()
                    .rev()
                    .find(|record| record.message.role == ModelRole::Assistant)
                {
                    if !record
                        .message
                        .tool_calls
                        .iter()
                        .any(|existing| existing.id == call_id)
                    {
                        record.message.tool_calls.push(call.clone());
                    }
                    record
                        .context
                        .atomic_group
                        .get_or_insert_with(|| legacy_tool_turn_group(record.context.range.start))
                        .clone()
                } else {
                    let group = legacy_tool_turn_group(entry.sequence);
                    let mut message = ModelMessage::new(ModelRole::Assistant, "");
                    message.tool_calls.push(call.clone());
                    session.records.push(SessionRecord {
                        message,
                        context: ContextEntry::completed(
                            SequenceRange::new(entry.sequence, entry.sequence)?,
                            format!("tool call {call_id}"),
                        )
                        .atomic(group.clone()),
                    });
                    group
                };
                pending_tools.insert((agent_id, call_id), (entry.sequence, call, group));
            }
            JournalEvent::ToolResult {
                agent_id,
                call_id,
                content,
                ..
            } if !sequence_is_covered(
                entry.sequence,
                covered_ranges
                    .get(&agent_id)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            ) =>
            {
                let (start, group) = pending_tools
                    .remove(&(agent_id.clone(), call_id.clone()))
                    .map(|(sequence, _, group)| (sequence, group))
                    .unwrap_or_else(|| (entry.sequence, call_id.clone()));
                let mut message = ModelMessage::new(ModelRole::Tool, content.clone());
                message.tool_call_id = Some(call_id.clone());
                sessions
                    .entry(agent_id)
                    .or_default()
                    .records
                    .push(SessionRecord {
                        message,
                        context: ContextEntry::completed(
                            SequenceRange::new(start, entry.sequence)?,
                            content,
                        )
                        .atomic(group),
                    });
            }
            _ => {}
        }
    }

    for ((agent_id, call_id), (sequence, _, group)) in pending_tools {
        let mut message = ModelMessage::new(
            ModelRole::Tool,
            "Tool execution was interrupted by restart and was not replayed.",
        );
        message.tool_call_id = Some(call_id.clone());
        sessions
            .entry(agent_id)
            .or_default()
            .records
            .push(SessionRecord {
                message,
                context: ContextEntry::completed(
                    SequenceRange::new(sequence, sequence)?,
                    "incomplete tool execution; side effect not replayed",
                )
                .atomic(group)
                .protect(LiveReason::IncompleteTool),
            });
    }
    let mut consumed_by_agent = HashMap::<AgentId, Vec<MessageDelivery>>::new();
    for (agent_id, message_id) in consumed_messages {
        let Some(delivery) = delivered_messages.remove(&(agent_id.clone(), message_id)) else {
            continue;
        };
        if !sequence_is_covered(
            delivery.sequence,
            covered_ranges
                .get(&agent_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        ) {
            consumed_by_agent
                .entry(agent_id)
                .or_default()
                .push(delivery);
        }
    }
    for (agent_id, mut deliveries) in consumed_by_agent {
        deliveries.sort_by_key(|delivery| delivery.sequence);
        let session = sessions.entry(agent_id).or_default();
        for delivery in deliveries {
            let mut record = inbox_record(&delivery)?;
            record.context.live_reason = None;
            session.records.push(record);
        }
    }
    Ok(sessions)
}

fn model_role(role: TranscriptRole) -> ModelRole {
    match role {
        TranscriptRole::System => ModelRole::System,
        TranscriptRole::User => ModelRole::User,
        TranscriptRole::Assistant => ModelRole::Assistant,
        TranscriptRole::Tool => ModelRole::Tool,
    }
}

fn unique_ids(ids: impl IntoIterator<Item = InsightId>) -> Vec<InsightId> {
    let mut seen = HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn sequence_is_covered(sequence: u64, ranges: &[SequenceRange]) -> bool {
    ranges
        .iter()
        .any(|range| sequence >= range.start && sequence <= range.end)
}

struct ActiveTurn {
    inner: Arc<RuntimeInner>,
}

impl ActiveTurn {
    fn new(inner: Arc<RuntimeInner>) -> Self {
        inner.active_turns.fetch_add(1, Ordering::AcqRel);
        Self { inner }
    }
}

impl Drop for ActiveTurn {
    fn drop(&mut self) {
        self.inner.active_turns.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("runtime configuration is invalid")]
    InvalidConfig,
    #[error("runtime has stopped")]
    Stopped,
    #[error("agent turn was cancelled")]
    Cancelled,
    #[error("agent exceeded the configured model-turn limit")]
    ModelTurnLimit,
    #[error("runtime state lock was poisoned")]
    LockPoisoned,
    #[error("runtime did not become idle before the deadline")]
    IdleTimedOut,
    #[error("semantic compaction exceeded its total deadline of {0:?}")]
    CompactionTimedOut(Duration),
    #[error(transparent)]
    Domain(#[from] ma_core::domain::DomainError),
    #[error(transparent)]
    Engagement(#[from] ma_core::engagement::EngagementError),
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Coordinator(#[from] CoordinatorError),
    #[error(transparent)]
    Brief(#[from] BriefError),
    #[error(transparent)]
    Compaction(#[from] CompactionError),
    #[error(transparent)]
    Provider(ProviderFault),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_model_observation_releases_only_recovery_protection() {
        let mut session = AgentSession {
            records: vec![
                record(1, LiveReason::PartialOutput),
                record(2, LiveReason::IncompleteTool),
                record(3, LiveReason::UnreadInbox),
            ],
            ..AgentSession::default()
        };

        session.release_recovery_protection();

        assert_eq!(session.records[0].context.live_reason, None);
        assert_eq!(session.records[1].context.live_reason, None);
        assert_eq!(
            session.records[2].context.live_reason,
            Some(LiveReason::UnreadInbox)
        );
    }

    fn record(sequence: u64, reason: LiveReason) -> SessionRecord {
        SessionRecord {
            message: ModelMessage::new(ModelRole::Assistant, "source"),
            context: ContextEntry::completed(
                SequenceRange::new(sequence, sequence).unwrap(),
                "source",
            )
            .protect(reason),
        }
    }
}
