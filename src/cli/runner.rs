use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, bail};
use chrono::Utc;
use uuid::Uuid;

use pentesting::domain::AgentId;
use pentesting::provider::{OpenAiChatProvider, ProviderSlot};
use pentesting::runtime::{RuntimeConfig, TeamRuntime};
use pentesting::settings::{ProviderSettings, ProviderSettingsStore};
use pentesting::tui::run_tui;

use super::args::RunArgs;
use super::plain::run_plain;
use crate::headless::run_headless;

pub async fn run(args: RunArgs) -> anyhow::Result<()> {
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
        .and_then(pentesting::settings::parse_token_input);
    let cli_context_tokens = context_tokens
        .as_deref()
        .and_then(pentesting::settings::parse_token_input);
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

pub fn print_session_exit_banner(run_root: &Path) {
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

pub fn provider_settings_root(
    run: Option<&Path>,
    resume: Option<&Path>,
    workspace: &Path,
) -> PathBuf {
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

pub async fn load_provider_slot(
    store: &ProviderSettingsStore,
) -> anyhow::Result<Arc<ProviderSlot>> {
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

pub fn provider_from_settings(settings: &ProviderSettings) -> anyhow::Result<OpenAiChatProvider> {
    settings.build_provider()
}

pub fn default_run_root(workspace: &Path) -> PathBuf {
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
