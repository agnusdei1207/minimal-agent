use std::future::Future;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, bail};
use chrono::Utc;
use clap::{Parser, Subcommand};
use minimal_agent::brief::AgentBriefStore;
use minimal_agent::coordinator::AgentCoordinator;
use minimal_agent::domain::{AgentId, ContextBudget, MAX_USER_INPUT_BYTES};
use minimal_agent::engagement::{Engagement, EngagementKind};
use minimal_agent::journal::{JournalConfig, JournalEvent, JournalEventKind, RunJournal};
use minimal_agent::provider::{OpenAiChatProvider, ProviderSlot};
use minimal_agent::runtime::{RuntimeConfig, RuntimeEvent, TeamRuntime};
use minimal_agent::settings::{ProviderSettings, ProviderSettingsStore};
use minimal_agent::tui::{UiCommand, command_help, parse_command, run_tui};
use serde_json::json;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    name = "minimal-agent",
    version,
    about = "A small autonomous team-agent runtime"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Command {
    /// Start a new run or resume an existing durable run.
    Run {
        /// Goal for a new run.
        #[arg(long)]
        goal: Option<String>,
        /// Workspace exposed to shell and file tools.
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        /// Explicit directory for a new run.
        #[arg(long, conflicts_with = "resume")]
        run: Option<PathBuf>,
        /// Existing run directory to resume.
        #[arg(long)]
        resume: Option<PathBuf>,
        /// Enable autonomous follow-up turns.
        #[arg(long)]
        auto: bool,
        /// Use line-oriented stdin/stdout instead of the TUI.
        #[arg(long)]
        plain: bool,
        /// Authorized-engagement JSON file (kind, scope, off_limits, flag_format, objective).
        #[arg(long)]
        engagement: Option<PathBuf>,
        /// Engagement kind: ctf, pentest, or lab. Overrides the file value.
        #[arg(long)]
        engagement_kind: Option<String>,
        /// Authorized target/scope (e.g. 10.10.10.0/24 or a URL). Overrides the file value.
        #[arg(long)]
        target: Option<String>,
        /// Off-limits target (repeatable). When given, replaces the file list.
        #[arg(long = "off-limits")]
        off_limits: Vec<String>,
        /// Flag format/regex used to recognize a captured flag (e.g. flag\{[^}]+\}).
        #[arg(long)]
        flag_format: Option<String>,
        /// Engagement objective. Used as the run goal when --goal is omitted.
        #[arg(long)]
        objective: Option<String>,
        /// Maximum model turns allowed per user turn (0 or 'unlimited' for no limit).
        #[arg(long)]
        max_turns: Option<String>,
        /// Non-interactive autonomous run: submit the objective, wait for the team to
        /// settle, print a one-line JSON result, and exit. Implies plain mode.
        #[arg(long)]
        headless: bool,
    },
    /// Inspect durable team state and curated briefs without a model provider.
    Inspect {
        /// Existing run directory.
        #[arg(long)]
        run: PathBuf,
        /// Limit output to one agent.
        #[arg(long)]
        agent: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Run {
            goal,
            workspace,
            run: run_root,
            resume,
            auto,
            plain,
            engagement,
            engagement_kind,
            target,
            off_limits,
            flag_format,
            objective,
            max_turns,
            headless,
        } => {
            let engagement = build_engagement(
                engagement.as_deref(),
                engagement_kind.as_deref(),
                target,
                off_limits,
                flag_format,
                objective,
            )?;
            run(RunArgs {
                goal,
                workspace,
                run: run_root,
                resume,
                auto,
                plain,
                engagement,
                max_turns,
                headless,
            })
            .await
        }
        Command::Inspect { run, agent } => inspect(&run, agent.as_deref()),
    }
}

fn build_engagement(
    file: Option<&Path>,
    kind: Option<&str>,
    target: Option<String>,
    off_limits: Vec<String>,
    flag_format: Option<String>,
    objective: Option<String>,
) -> anyhow::Result<Option<Engagement>> {
    let has_flag = kind.is_some()
        || target.is_some()
        || !off_limits.is_empty()
        || flag_format.is_some()
        || objective.is_some();
    if file.is_none() && !has_flag {
        return Ok(None);
    }
    let base = match file {
        Some(path) => Engagement::from_file(path)?,
        None => Engagement::default(),
    };
    let kind = kind.map(EngagementKind::parse).transpose()?;
    let off_limits = (!off_limits.is_empty()).then_some(off_limits);
    let engagement = base.overlay(kind, target, off_limits, flag_format, objective, None)?;
    Ok(Some(engagement))
}

struct RunArgs {
    goal: Option<String>,
    workspace: PathBuf,
    run: Option<PathBuf>,
    resume: Option<PathBuf>,
    auto: bool,
    plain: bool,
    engagement: Option<Engagement>,
    max_turns: Option<String>,
    headless: bool,
}

async fn run(args: RunArgs) -> anyhow::Result<()> {
    let RunArgs {
        goal,
        workspace,
        run,
        resume,
        auto,
        plain,
        engagement,
        max_turns,
        headless,
    } = args;
    // A run objective may come from --goal or, failing that, the engagement.
    let goal = goal.filter(|goal| !goal.is_empty()).or_else(|| {
        engagement
            .as_ref()
            .and_then(|engagement| engagement.objective.clone())
    });
    if resume.is_none() && goal.is_none() {
        bail!("a new run requires --goal or an engagement objective");
    }
    let settings_root = provider_settings_root(run.as_deref(), resume.as_deref(), &workspace);
    let settings_store = ProviderSettingsStore::new(settings_root);
    let provider = load_provider_slot(&settings_store).await?;
    let max_model_turns = match max_turns.as_deref() {
        Some(val) => {
            let trimmed = val.trim();
            if trimmed.eq_ignore_ascii_case("unlimited")
                || trimmed.eq_ignore_ascii_case("infinite")
                || trimmed == "0"
            {
                usize::MAX
            } else {
                trimmed.parse().context("invalid --max-turns value")?
            }
        }
        None => RuntimeConfig::default().max_model_turns,
    };
    let config = RuntimeConfig {
        auto,
        max_model_turns,
        engagement: engagement.clone(),
        ..RuntimeConfig::default()
    };
    let (runtime, displayed_goal) = if let Some(run_root) = resume {
        let runtime = TeamRuntime::resume(&run_root, &workspace, provider.clone(), config).await?;
        let displayed_goal = runtime.coordinator().inspect(&AgentId::main())?.task;
        (runtime, displayed_goal)
    } else {
        let goal = goal.expect("new run goal was validated");
        let run_root = run.unwrap_or_else(|| default_run_root(&workspace));
        let runtime =
            TeamRuntime::create(&run_root, &workspace, &goal, provider.clone(), config).await?;
        eprintln!("run: {}", run_root.display());
        (runtime, goal)
    };

    if headless {
        return run_headless(runtime, displayed_goal, engagement, auto).await;
    }

    let use_tui = !plain && std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    if use_tui {
        let result = run_tui(runtime.clone(), displayed_goal, provider, settings_store).await;
        runtime.shutdown().await;
        result
    } else {
        run_plain(runtime, auto).await
    }
}

fn provider_settings_root(run: Option<&Path>, resume: Option<&Path>, workspace: &Path) -> PathBuf {
    run.or(resume)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| workspace.join(".minimal-agent"))
}

async fn load_provider_slot(store: &ProviderSettingsStore) -> anyhow::Result<Arc<ProviderSlot>> {
    let settings = store.load()?.or(ProviderSettings::from_standard_env()?);
    let context_limit = settings
        .as_ref()
        .map_or(128_000, |settings| settings.context_tokens);
    let slot = Arc::new(ProviderSlot::unconfigured(context_limit));
    if let Some(settings) = settings {
        let provider = provider_from_settings(&settings)?;
        let model = settings.model.clone();
        slot.replace(Arc::new(provider), model).await;
    }
    Ok(slot)
}

fn provider_from_settings(settings: &ProviderSettings) -> anyhow::Result<OpenAiChatProvider> {
    settings.build_provider()
}

fn default_run_root(workspace: &Path) -> PathBuf {
    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let suffix = Uuid::new_v4().simple().to_string();
    workspace
        .join(".minimal-agent")
        .join("runs")
        .join(format!("{timestamp}-{}", &suffix[..8]))
}

async fn run_plain(runtime: TeamRuntime, mut auto_enabled: bool) -> anyhow::Result<()> {
    println!("minimal-agent plain mode; /help for commands");
    let mut input = BufReader::new(tokio::io::stdin());
    loop {
        tokio::select! {
            line = read_bounded_line(&mut input, MAX_USER_INPUT_BYTES) => {
                let Some(line) = line.context("read stdin")? else { break };
                if line.trim().is_empty() {
                    continue;
                }
                match parse_command(&line) {
                    Ok(Some(UiCommand::Exit)) => break,
                    Ok(Some(UiCommand::Auto)) => {
                        let enabled = !auto_enabled;
                        runtime.set_auto(enabled);
                        auto_enabled = enabled;
                        println!("auto {}", if enabled { "on" } else { "off" });
                    }
                    Ok(Some(UiCommand::Status)) => print_status(&runtime)?,
                    Ok(Some(UiCommand::Agent)) => print_status(&runtime)?,
                    Ok(Some(UiCommand::AgentSwitch(agent))) => print_agent(&runtime, &agent)?,
                    Ok(Some(UiCommand::Help)) => println!("{}", command_help()),
                    Ok(Some(UiCommand::Compact)) => {
                        let Some(compacted) = interrupt_on_ctrl_c(runtime.compact_main()).await?
                        else {
                            break;
                        };
                        println!(
                            "{}",
                            if compacted {
                                "main context compacted and coverage verified"
                            } else {
                                "main context is below the semantic compaction threshold"
                            }
                        );
                    }
                    Ok(Some(UiCommand::Goal(objective))) => {
                        if interrupt_on_ctrl_c(runtime.set_goal(objective.clone()))
                            .await?
                            .is_none()
                        {
                            break;
                        }
                        // Setting a goal starts autonomous work; clearing it stops.
                        let enabled = objective.is_some();
                        runtime.set_auto(enabled);
                        auto_enabled = enabled;
                        println!(
                            "{}",
                            objective.map_or_else(
                                || "goal cleared; autonomous loop stopped".to_owned(),
                                |goal| format!("goal set: {goal}; autonomous loop started"),
                            )
                        );
                    }
                    Ok(Some(UiCommand::Bash(command))) => {
                        let Some(output) = interrupt_on_ctrl_c(runtime.run_bash(command)).await?
                        else {
                            break;
                        };
                        println!("{output}");
                    }
                    Ok(Some(UiCommand::Resume)) => println!(
                        "minimal-agent run --resume {}",
                        runtime.coordinator().journal().root().display()
                    ),
                    Ok(Some(UiCommand::Model(query))) => println!(
                        "{}",
                        query.map_or_else(
                            || "run without --plain and use /model for interactive setup".to_owned(),
                            |query| format!("interactive model filter requested: {query}; rerun without --plain"),
                        )
                    ),
                    Ok(Some(UiCommand::Update)) => println!("npm install -g minimal-agent@latest"),
                    Ok(Some(UiCommand::New)) => println!(
                        "start a new durable run with `minimal-agent run --goal ...`"
                    ),
                    Ok(Some(UiCommand::Target(scope))) => match runtime.set_engagement_scope(scope.clone()) {
                        Ok(()) => println!(
                            "{}",
                            match scope {
                                Some(scope) => format!("authorized target set: {scope}"),
                                None => "authorized target cleared".to_owned(),
                            }
                        ),
                        Err(error) => eprintln!("target error: {error}"),
                    },
                    Ok(None) => match interrupt_on_ctrl_c(runtime.submit_user(line)).await {
                        Ok(Some(result)) if !result.text.is_empty() => println!("{}", result.text),
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(error) => eprintln!("runtime error: {error}"),
                    },
                    Err(error) => eprintln!("command error: {error}"),
                }
            }
            signal = tokio::signal::ctrl_c() => {
                signal.context("install Ctrl+C handler")?;
                break;
            }
        }
    }
    runtime.shutdown().await;
    Ok(())
}

/// Non-interactive autonomous run for external injection (xbow-style benchmarks,
/// ADR-0002 §3.3). Submits the objective once, waits for the team to settle,
/// prints a one-line JSON result, and exits non-zero when a flag was required
/// but never captured from real output.
async fn run_headless(
    runtime: TeamRuntime,
    goal: String,
    engagement: Option<Engagement>,
    auto: bool,
) -> anyhow::Result<()> {
    const MAX_WALL: Duration = Duration::from_secs(1800);
    let idle_window = if auto {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(2)
    };

    let observation =
        observe_headless(&runtime, &goal, engagement.as_ref(), MAX_WALL, idle_window).await;
    runtime.shutdown().await;
    let observation = observation?;
    let flag = observation.flag;
    let flag_required = engagement
        .as_ref()
        .is_some_and(|engagement| engagement.flag_format.is_some());
    println!(
        "{}",
        serde_json::to_string(&json!({
            "goal": goal,
            "flag": flag,
            "flag_required": flag_required,
            "summary": observation.summary,
        }))?
    );

    if flag_required && flag.is_none() {
        std::process::exit(1);
    }
    Ok(())
}

#[derive(Default)]
struct HeadlessObservation {
    summary: String,
    flag: Option<String>,
}

async fn observe_headless(
    runtime: &TeamRuntime,
    goal: &str,
    engagement: Option<&Engagement>,
    max_wall: Duration,
    idle_window: Duration,
) -> anyhow::Result<HeadlessObservation> {
    let deadline = tokio::time::Instant::now() + max_wall;
    let mut events = runtime.subscribe();
    let journal = runtime.coordinator().journal();
    // A resumed run must not inherit an old flag as evidence for this submission.
    let prior_sequence = journal.latest_sequence()?;
    let mut observation = HeadlessObservation::default();
    let submission = runtime.submit_user(goal);
    tokio::pin!(submission);
    let mut submitted = false;
    let mut idle_deadline = tokio::time::Instant::now() + idle_window;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => break,
            turn = &mut submission, if !submitted => {
                submitted = true;
                idle_deadline = tokio::time::Instant::now() + idle_window;
                match turn {
                    Ok(turn) if !turn.text.trim().is_empty() => observation.summary = turn.text.trim().to_owned(),
                    Ok(_) => {},
                    Err(error) => eprintln!("runtime error: {error}"),
                }
            }
            event = events.recv() => {
                idle_deadline = tokio::time::Instant::now() + idle_window;
                match event {
                    Ok(event) => observation.apply(event),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {},
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = tokio::time::sleep_until(idle_deadline), if submitted => {
                // Broadcast lag can lose TurnStarted/TurnFinished. Check the
                // runtime's active-turn counter before declaring the team idle.
                match tokio::time::timeout_at(deadline, runtime.wait_until_idle(Duration::from_millis(100))).await {
                    Ok(Ok(())) | Err(_) => break,
                    Ok(Err(_)) => idle_deadline = tokio::time::Instant::now() + idle_window,
                }
            }
        }
    }
    // Stop writers before the final replay, including a submission still in
    // flight when the deadline fired. ToolResult is the durable evidence source.
    runtime.shutdown().await;
    if let Some(engagement) = engagement.filter(|value| value.flag_format.is_some()) {
        // Always use the watermark-filtered journal. A resumed worker can emit
        // a broadcast before the watermark is captured; that old event must not
        // become evidence for the new submission merely because it was queued.
        observation.flag = headless_journal_evidence(&journal, engagement, prior_sequence)?;
    }
    Ok(observation)
}

fn headless_journal_evidence(
    journal: &RunJournal,
    engagement: &Engagement,
    after: u64,
) -> anyhow::Result<Option<String>> {
    const MAX_TOOL_EVENT_BYTES: usize = 8 * 1024 * 1024;
    let mut flag = None;
    journal.visit_kind_after(
        after,
        JournalEventKind::ToolResult,
        MAX_TOOL_EVENT_BYTES,
        |entry| {
            if flag.is_none()
                && let JournalEvent::ToolResult { content, .. } = entry.event
            {
                flag = engagement.extract_flag(&content);
            }
        },
    )?;
    Ok(flag)
}

impl HeadlessObservation {
    fn apply(&mut self, event: RuntimeEvent) {
        match event {
            RuntimeEvent::Assistant { text, .. } if !text.trim().is_empty() => {
                self.summary = text.trim().to_owned();
            }
            _ => {}
        }
    }
}

async fn interrupt_on_ctrl_c<T, E>(
    operation: impl Future<Output = Result<T, E>>,
) -> anyhow::Result<Option<T>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    select_operation_or_shutdown(operation, tokio::signal::ctrl_c()).await
}

async fn select_operation_or_shutdown<T, E>(
    operation: impl Future<Output = Result<T, E>>,
    shutdown: impl Future<Output = io::Result<()>>,
) -> anyhow::Result<Option<T>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    tokio::select! {
        result = operation => result.map(Some).map_err(anyhow::Error::new),
        signal = shutdown => {
            signal.context("install Ctrl+C handler")?;
            Ok(None)
        }
    }
}

async fn read_bounded_line<R>(reader: &mut R, max_bytes: usize) -> io::Result<Option<String>>
where
    R: AsyncBufRead + Unpin,
{
    let mut bytes = Vec::with_capacity(max_bytes.min(8 * 1_024));
    loop {
        let (consumed, complete) = {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                if bytes.is_empty() {
                    return Ok(None);
                }
                (0, true)
            } else {
                let newline = available.iter().position(|byte| *byte == b'\n');
                let consumed = newline.map_or(available.len(), |position| position + 1);
                if bytes.len().saturating_add(consumed) > max_bytes.saturating_add(2) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("input line exceeds {max_bytes} bytes"),
                    ));
                }
                bytes.extend_from_slice(&available[..consumed]);
                (consumed, newline.is_some())
            }
        };
        reader.consume(consumed);
        if complete {
            break;
        }
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    if bytes.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("input line exceeds {max_bytes} bytes"),
        ));
    }
    String::from_utf8(bytes).map(Some).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("input line is not valid UTF-8: {error}"),
        )
    })
}

fn print_status(runtime: &TeamRuntime) -> anyhow::Result<()> {
    let team = runtime.coordinator().live_team()?;
    println!("{}", serde_json::to_string_pretty(&team_json(&team))?);
    Ok(())
}

fn print_agent(runtime: &TeamRuntime, agent: &AgentId) -> anyhow::Result<()> {
    let snapshot = runtime.coordinator().inspect(agent)?;
    let brief = runtime.brief(agent)?;
    println!(
        "{}\n\n{brief}",
        serde_json::to_string_pretty(&agent_json(&snapshot))?
    );
    Ok(())
}

fn inspect(run_root: &Path, selected_agent: Option<&str>) -> anyhow::Result<()> {
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

fn inspect_brief(store: &AgentBriefStore, agent: &AgentId) -> anyhow::Result<String> {
    if !store.path(agent).exists() {
        return Ok("<no live brief projection; durable source remains in the journal>".to_owned());
    }
    Ok(store.read(agent)?)
}

fn team_json(team: &[minimal_agent::coordinator::AgentSnapshot]) -> Vec<serde_json::Value> {
    team.iter().map(agent_json).collect()
}

fn agent_json(agent: &minimal_agent::coordinator::AgentSnapshot) -> serde_json::Value {
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use minimal_agent::provider::{
        ModelDelta, ModelProvider, ModelRequest, ModelTurn, ProviderFault, ToolCall,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct HeadlessFixtureProvider {
        calls: AtomicUsize,
        pending: bool,
    }

    #[async_trait]
    impl ModelProvider for HeadlessFixtureProvider {
        fn model_id(&self) -> &str {
            "headless-fixture"
        }
        fn context_limit(&self) -> u64 {
            128_000
        }
        async fn complete(
            &self,
            _: ModelRequest,
            deltas: Option<tokio::sync::mpsc::UnboundedSender<ModelDelta>>,
        ) -> Result<ModelTurn, ProviderFault> {
            if self.pending {
                std::future::pending::<()>().await;
            }
            let first = self.calls.fetch_add(1, Ordering::SeqCst) == 0;
            if !first && let Some(sender) = deltas {
                for _ in 0..4096 {
                    let _ = sender.send(ModelDelta::Text("x".to_owned()));
                }
            }
            Ok(ModelTurn {
                text: if first {
                    String::new()
                } else {
                    "flag{model-invented}".to_owned()
                },
                tool_calls: if first {
                    vec![ToolCall {
                        id: "read-fixture".to_owned(),
                        name: "workspace".to_owned(),
                        arguments: json!({"op":"read","path":"evidence.txt"}),
                    }]
                } else {
                    vec![]
                },
                usage: None,
                finish_reason: Some("stop".to_owned()),
            })
        }
    }

    async fn headless_fixture(directory: &Path, pending: bool, evidence: &str) -> TeamRuntime {
        let runtime = TeamRuntime::create(
            directory.join("run"),
            directory.join("workspace"),
            "read local evidence",
            Arc::new(HeadlessFixtureProvider {
                calls: AtomicUsize::new(0),
                pending,
            }),
            RuntimeConfig::default(),
        )
        .await
        .unwrap();
        std::fs::write(directory.join("workspace/evidence.txt"), evidence).unwrap();
        runtime
    }

    #[tokio::test]
    async fn headless_recovers_tool_flag_despite_a_broadcast_flood() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = headless_fixture(directory.path(), false, "flag{durable-evidence}").await;
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        let observation = observe_headless(
            &runtime,
            "read local evidence",
            Some(&engagement),
            Duration::from_secs(3),
            Duration::from_millis(10),
        )
        .await
        .unwrap();
        runtime.shutdown().await;
        assert_eq!(observation.flag.as_deref(), Some("flag{durable-evidence}"));
    }

    #[tokio::test]
    async fn headless_wall_deadline_includes_the_initial_submission() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = headless_fixture(directory.path(), true, "unused").await;
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            observe_headless(
                &runtime,
                "read local evidence",
                None,
                Duration::from_millis(40),
                Duration::from_millis(10),
            ),
        )
        .await;
        runtime.shutdown().await;
        assert!(
            result.is_ok(),
            "the initial model call must obey the headless wall deadline"
        );
    }

    #[tokio::test]
    async fn headless_does_not_score_prior_run_or_model_only_flags() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = headless_fixture(directory.path(), false, "no flag in target output").await;
        runtime
            .coordinator()
            .journal()
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "prior-run".to_owned(),
                content: "flag{stale}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        let observation = observe_headless(
            &runtime,
            "read local evidence",
            Some(&engagement),
            Duration::from_secs(3),
            Duration::from_millis(10),
        )
        .await
        .unwrap();
        assert!(observation.flag.is_none());
        assert_eq!(observation.summary, "flag{model-invented}");
    }

    #[test]
    fn headless_durable_recovery_scans_long_history_and_respects_the_start_sequence() {
        let directory = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(directory.path(), JournalConfig::default()).unwrap();
        let old = journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "old".to_owned(),
                content: "flag{old}".to_owned(),
                success: true,
            })
            .unwrap();
        for _ in 0..260 {
            journal
                .append_sync(JournalEvent::Transcript {
                    agent_id: AgentId::main(),
                    role: minimal_agent::journal::TranscriptRole::Assistant,
                    content: "flag{invented}".to_owned(),
                    complete: true,
                    atomic_group: None,
                })
                .unwrap();
        }
        let last = journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "new".to_owned(),
                content: "flag{recovered}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        let flag = headless_journal_evidence(&journal, &engagement, old.sequence).unwrap();
        assert_eq!(flag.as_deref(), Some("flag{recovered}"));
        assert_eq!(
            headless_journal_evidence(&journal, &engagement, last.sequence).unwrap(),
            None
        );
    }

    #[test]
    fn headless_durable_recovery_skips_large_unrelated_blobs() {
        let directory = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(directory.path(), JournalConfig::default()).unwrap();
        for _ in 0..2 {
            journal
                .append_sync(JournalEvent::Transcript {
                    agent_id: AgentId::main(),
                    role: minimal_agent::journal::TranscriptRole::Assistant,
                    content: "x".repeat(5 * 1024 * 1024),
                    complete: true,
                    atomic_group: None,
                })
                .unwrap();
        }
        journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "large-history".to_owned(),
                content: "flag{after-large-records}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            headless_journal_evidence(&journal, &engagement, 0)
                .unwrap()
                .as_deref(),
            Some("flag{after-large-records}")
        );
    }

    #[test]
    fn headless_durable_recovery_skips_an_unrelated_record_above_the_response_budget() {
        let directory = tempfile::tempdir().unwrap();
        let journal = RunJournal::open(directory.path(), JournalConfig::default()).unwrap();
        journal
            .append_sync(JournalEvent::Transcript {
                agent_id: AgentId::main(),
                role: minimal_agent::journal::TranscriptRole::Assistant,
                content: "x".repeat(9 * 1024 * 1024),
                complete: true,
                atomic_group: None,
            })
            .unwrap();
        journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: "after-large-record".to_owned(),
                content: "flag{after-large-record}".to_owned(),
                success: true,
            })
            .unwrap();
        let engagement = Engagement {
            flag_format: Some(r"flag\{[^}]+\}".to_owned()),
            ..Default::default()
        };
        assert_eq!(
            headless_journal_evidence(&journal, &engagement, 0)
                .unwrap()
                .as_deref(),
            Some("flag{after-large-record}")
        );
    }

    #[tokio::test]
    async fn plain_input_reader_never_buffers_beyond_its_line_limit() {
        let mut accepted = BufReader::new(&b"hello\nnext\n"[..]);
        assert_eq!(
            read_bounded_line(&mut accepted, 5)
                .await
                .unwrap()
                .as_deref(),
            Some("hello")
        );
        assert_eq!(
            read_bounded_line(&mut accepted, 5)
                .await
                .unwrap()
                .as_deref(),
            Some("next")
        );

        let mut oversized = BufReader::new(&b"123456\n"[..]);
        assert_eq!(
            read_bounded_line(&mut oversized, 5)
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    #[tokio::test]
    async fn shutdown_wins_while_a_plain_operation_is_pending() {
        let result = select_operation_or_shutdown(
            std::future::pending::<Result<(), io::Error>>(),
            std::future::ready(Ok(())),
        )
        .await
        .unwrap();

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn saved_provider_context_limit_is_applied_before_runtime_creation() {
        let directory = tempfile::tempdir().unwrap();
        let store = ProviderSettingsStore::new(directory.path());
        store
            .save(&minimal_agent::settings::ProviderSettings {
                provider: "openai-compatible".to_owned(),
                base_url: "https://example.test/v1".to_owned(),
                model: "small-model".to_owned(),
                api_key: "secret".to_owned(),
                context_tokens: 16_384,
            })
            .unwrap();

        let slot = load_provider_slot(&store).await.unwrap();

        assert_eq!(slot.runtime_context_limit(), 16_384);
        assert!(slot.is_configured());
    }

    #[test]
    fn headless_live_events_cannot_score_broadcast_only_or_stale_tool_flags() {
        let mut observation = HeadlessObservation::default();
        observation.apply(RuntimeEvent::Assistant {
            agent_id: AgentId::main(),
            text: "flag{invented}".to_owned(),
        });
        assert!(observation.flag.is_none());

        observation.apply(RuntimeEvent::ToolFinished {
            agent_id: AgentId::main(),
            name: "shell".to_owned(),
            success: true,
            output: "queued before current submission: flag{old}".to_owned(),
        });
        assert!(observation.flag.is_none());
    }
}
