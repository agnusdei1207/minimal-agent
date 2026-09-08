use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{Semaphore, broadcast, mpsc, oneshot, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::brief::{AgentBriefStore, BriefError};
use crate::coordinator::AgentCoordinator;
use crate::domain::{AgentId, AgentState, ContextBudget, validate_user_input};
use crate::engagement::Engagement;
use crate::journal::{JournalEvent, RunJournal};
use crate::provider::ModelProvider;
use crate::tools::BuiltinTools;

use super::config::RuntimeConfig;
use super::constants::{
    EVENT_CHANNEL_CAPACITY, IDLE_POLL_INTERVAL, IDLE_STABLE_SAMPLES, MAIN_COMMAND_CAPACITY,
    SHUTDOWN_JOIN_DEADLINE,
};
use super::error::RuntimeError;
use super::events::{RuntimeEvent, TurnResult};
use super::recovery::{recover_engagement, recover_sessions};
use super::session::AgentSession;
use super::spawner::RuntimeSpawner;
use super::worker::{WorkerExit, main_driver, worker_driver};

pub enum MainCommand {
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
    pub(crate) inner: Arc<RuntimeInner>,
}

pub struct RuntimeInner {
    pub(crate) workspace: PathBuf,
    pub(crate) journal: Arc<RunJournal>,
    pub(crate) coordinator: AgentCoordinator,
    pub(crate) briefs: AgentBriefStore,
    pub(crate) provider: Arc<dyn ModelProvider>,
    pub(crate) tools: BuiltinTools,
    pub(crate) budget: ContextBudget,
    pub(crate) config: RuntimeConfig,
    pub(crate) requests: Arc<Semaphore>,
    pub(crate) auto: watch::Sender<bool>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) events: broadcast::Sender<RuntimeEvent>,
    pub(crate) main_tx: mpsc::Sender<MainCommand>,
    pub(crate) main_turn_interrupt: Mutex<Option<CancellationToken>>,
    pub(crate) tasks: Mutex<HashMap<AgentId, JoinHandle<()>>>,
    pub(crate) active_turns: AtomicUsize,
    /// User input delivered while a main turn is running. It is injected into the
    /// running turn's session as a user message at the next model-turn boundary,
    /// so the agent folds it into its current context instead of losing progress.
    pub(crate) main_steer: Mutex<VecDeque<String>>,
    /// Live authorized-engagement context. Seeded from the config and updatable at
    /// runtime (e.g. `/target`); every prompt build reads the current value.
    pub(crate) engagement: Mutex<Option<Engagement>>,
}

impl RuntimeInner {
    // Runtime-owned projection: the caller's semantic brief authority is unrelated.
    pub fn sync_main_brief(&self) -> Result<(), RuntimeError> {
        self.briefs
            .sync_main_runtime(&AgentId::main(), &self.coordinator.live_team()?)?;
        Ok(())
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
        // recovers the same target/scope/flag doctrine (INTENT-0002 §4).
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
        let (main_tx, main_rx) = mpsc::channel(MAIN_COMMAND_CAPACITY);
        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

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
        {
            let mut guard = self
                .inner
                .engagement
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if guard.is_none() && scope.is_none() {
                return Ok(());
            }
            let updated = guard.clone().unwrap_or_default().set_scope(scope)?;
            self.inner
                .journal
                .append_sync(JournalEvent::EngagementSet {
                    engagement: updated.clone(),
                })?;
            *guard = Some(updated);
        }
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
                self.inner.sync_main_brief()?;
                self.start_worker_driver(id.clone(), AgentSession::default(), Some(task))
            });
        if let Err(error) = startup {
            let _ = self.inner.coordinator.mark_terminal(
                &AgentId::main(),
                &id,
                AgentState::Faulted,
                "worker startup failed",
            );
            let _ = self.inner.briefs.remove_projection(&id);
            let _ = self.inner.sync_main_brief();
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
                if stable_samples >= IDLE_STABLE_SAMPLES {
                    return Ok(());
                }
            } else {
                stable_samples = 0;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(RuntimeError::IdleTimedOut);
            }
            tokio::time::sleep(IDLE_POLL_INTERVAL).await;
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
        let deadline = tokio::time::Instant::now() + SHUTDOWN_JOIN_DEADLINE;
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
            let _exit = WorkerExit {
                inner: inner.clone(),
                id: worker_id.clone(),
            };
            worker_driver(inner, worker_id, session, initial).await;
        });
        tasks.insert(id, handle);
        Ok(())
    }
}
