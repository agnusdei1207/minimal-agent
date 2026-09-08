use anyhow::{Context, bail};
use chrono::Utc;
use clap::{Parser, Subcommand};
use minimal_agent::brief::AgentBriefStore;
use minimal_agent::coordinator::AgentCoordinator;
use minimal_agent::domain::{AgentId, ContextBudget, MAX_USER_INPUT_BYTES};
use minimal_agent::engagement::{Engagement, EngagementKind};
use minimal_agent::journal::{JournalConfig, RunJournal};
use minimal_agent::provider::{OpenAiChatProvider, ProviderSlot};
use minimal_agent::runtime::{RuntimeConfig, TeamRuntime};
use minimal_agent::settings::{ProviderSettings, ProviderSettingsStore};
use minimal_agent::tui::{UiCommand, command_help, parse_command, run_tui};
use serde_json::json;
use std::future::Future;
use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use uuid::Uuid;

mod headless;

use headless::run_headless;

#[derive(Parser)]
#[command(
    name = "pentesting",
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
        /// Operator-imposed context ceiling. Overrides OPENAI_CONTEXT_TOKENS.
        #[arg(long, alias = "max-context-tokens")]
        context_tokens: Option<String>,
        /// Maximum completion tokens reserved for model output. Overrides OPENAI_MAX_TOKENS.
        #[arg(long, alias = "max-output-tokens", alias = "output-max-tokens")]
        max_tokens: Option<String>,
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
            context_tokens,
            max_tokens,
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
                context_tokens,
                max_tokens,
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
    context_tokens: Option<String>,
    max_tokens: Option<String>,
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
        context_tokens,
        max_tokens,
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
    let settings = ProviderSettings::from_standard_env()?.or(settings_store.load()?);
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
    let cli_max_tokens = max_tokens
        .as_deref()
        .and_then(minimal_agent::settings::parse_token_input);
    let cli_context_tokens = context_tokens
        .as_deref()
        .and_then(minimal_agent::settings::parse_token_input);
    let reserved_response_tokens = cli_max_tokens
        .or_else(|| settings.as_ref().and_then(|s| s.max_output_tokens))
        .unwrap_or_else(|| RuntimeConfig::default().reserved_response_tokens);
    let configured_context_tokens = cli_context_tokens
        .or_else(|| settings.as_ref().map(|s| s.context_tokens))
        .unwrap_or_else(|| RuntimeConfig::default().configured_context_tokens);
    let config = RuntimeConfig {
        auto,
        max_model_turns,
        configured_context_tokens,
        reserved_response_tokens,
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
        let session_root = runtime.coordinator().journal().root().to_path_buf();
        let result = run_tui(runtime.clone(), displayed_goal, provider, settings_store).await;
        runtime.shutdown().await;
        print_session_exit_banner(&session_root);
        result
    } else {
        run_plain(runtime, auto).await
    }
}

fn print_session_exit_banner(run_root: &Path) {
    println!();
    println!("  \x1b[38;2;200;255;0mpentesting\x1b[0m — session saved");
    println!("  Run root: {}", run_root.display());
    println!("  To resume this session:");
    println!(
        "    \x1b[1mpentesting run --resume {}\x1b[0m",
        run_root.display()
    );
    println!();
}

fn provider_settings_root(run: Option<&Path>, resume: Option<&Path>, workspace: &Path) -> PathBuf {
    if let Some(parent) = run.or(resume).and_then(Path::parent) {
        return parent.to_path_buf();
    }
    let pentesting_dir = workspace.join(".pentesting");
    if pentesting_dir.exists() {
        return pentesting_dir;
    }
    let legacy_dir = workspace.join(".minimal-agent");
    if legacy_dir.exists() {
        return legacy_dir;
    }
    pentesting_dir
}

async fn load_provider_slot(store: &ProviderSettingsStore) -> anyhow::Result<Arc<ProviderSlot>> {
    let settings = ProviderSettings::from_standard_env()?.or(store.load()?);
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
    let root =
        if workspace.join(".minimal-agent").exists() && !workspace.join(".pentesting").exists() {
            workspace.join(".minimal-agent")
        } else {
            workspace.join(".pentesting")
        };
    root.join("runs")
        .join(format!("{timestamp}-{}", &suffix[..8]))
}

async fn run_plain(runtime: TeamRuntime, mut auto_enabled: bool) -> anyhow::Result<()> {
    println!("pentesting plain mode; /help for commands");
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
                        "pentesting run --resume {}",
                        runtime.coordinator().journal().root().display()
                    ),
                    Ok(Some(UiCommand::Model(query))) => println!(
                        "{}",
                        query.map_or_else(
                            || "run without --plain and use /model for interactive setup".to_owned(),
                            |query| format!("interactive model filter requested: {query}; rerun without --plain"),
                        )
                    ),
                    Ok(Some(UiCommand::Update)) => println!("npm install -g pentesting@latest"),
                    Ok(Some(UiCommand::New)) => println!(
                        "start a new durable run with `pentesting run --goal ...`"
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
                max_output_tokens: None,
            })
            .unwrap();

        let slot = load_provider_slot(&store).await.unwrap();

        assert_eq!(slot.runtime_context_limit(), 16_384);
        assert!(slot.is_configured());
    }
}
