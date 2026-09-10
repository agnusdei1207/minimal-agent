use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use pentesting::engagement::{Engagement, EngagementKind};

#[derive(Parser)]
#[command(
    name = "pentesting",
    version,
    about = "A small autonomous team-agent runtime"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
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

pub struct RunArgs {
    pub goal: Option<String>,
    pub workspace: PathBuf,
    pub run: Option<PathBuf>,
    pub resume: Option<PathBuf>,
    pub auto: bool,
    pub plain: bool,
    pub engagement: Option<Engagement>,
    pub max_turns: Option<String>,
    pub context_tokens: Option<String>,
    pub max_tokens: Option<String>,
    pub headless: bool,
}

pub fn build_engagement(
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
