use crate::domain::AgentId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    Compact,
    New,
    Status,
    Exit,
    Update,
    Help,
    Goal(Option<String>),
    Auto,
    Resume,
    Model(Option<String>),
    Agent,
    AgentSwitch(AgentId),
    Bash(String),
    /// Set (or clear, when `None`) the authorized-engagement target/scope live.
    Target(Option<String>),
}

pub fn parse_command(input: &str) -> Result<Option<UiCommand>, String> {
    let trimmed = input.trim();
    if let Some(command) = trimmed.strip_prefix('!') {
        let command = command.trim();
        return if command.is_empty() {
            Err("shell command cannot be empty".to_owned())
        } else {
            Ok(Some(UiCommand::Bash(command.to_owned())))
        };
    }
    if !trimmed.starts_with('/') {
        return Ok(None);
    }

    let mut parts = trimmed.split_ascii_whitespace();
    let command = parts.next().expect("slash input is not empty");
    let arguments = parts.collect::<Vec<_>>();
    let joined = || arguments.join(" ");
    let parsed = match command {
        "/compact" => UiCommand::Compact,
        "/new" | "/clear" => UiCommand::New,
        "/status" => UiCommand::Status,
        "/exit" => UiCommand::Exit,
        "/update" => UiCommand::Update,
        "/help" => UiCommand::Help,
        "/goal" => {
            let objective = joined();
            UiCommand::Goal((!objective.is_empty()).then_some(objective))
        }
        "/auto" if arguments.is_empty() => UiCommand::Auto,
        "/auto" => {
            return Err(
                "/auto takes no arguments; set a goal with /goal <objective> first".to_owned(),
            );
        }
        "/resume" if arguments.is_empty() => UiCommand::Resume,
        "/resume" => return Err("usage: /resume".to_owned()),
        "/model" => {
            let query = joined();
            UiCommand::Model((!query.is_empty()).then_some(query))
        }
        "/target" => {
            let scope = joined();
            UiCommand::Target((!scope.is_empty()).then_some(scope))
        }
        "/agent" if arguments.is_empty() => UiCommand::Agent,
        command if command.starts_with("/agent-") && arguments.is_empty() => {
            let id = command.trim_start_matches("/agent-");
            UiCommand::AgentSwitch(AgentId::new(id).map_err(|error| error.to_string())?)
        }
        _ => {
            return Err(format!(
                "unknown command '{command}'; use /help or remove '/' to send it as a message"
            ));
        }
    };
    Ok(Some(parsed))
}

/// Commands offered by the `/` menu, filtered by the current input prefix.
pub(crate) fn command_menu_matches(input: &str) -> Vec<(&'static str, &'static str)> {
    const CATALOG: &[(&str, &str)] = &[
        ("/help", "Show commands"),
        ("/status", "Main battlefield (modal)"),
        ("/goal", "Set goal & start the autonomous loop"),
        ("/target", "Set the authorized target/scope"),
        ("/auto", "Stop or resume the autonomous loop"),
        ("/agent", "List agents"),
        ("/compact", "Compact the main context now"),
        ("/model", "Configure provider, model, or endpoint"),
        ("/resume", "Browse & resume saved sessions (modal)"),
        ("/new", "Start a new conversation"),
        ("/update", "Show the update command"),
        ("/exit", "Save and quit"),
    ];
    let query = input.trim();
    if !query.starts_with('/') {
        return Vec::new();
    }
    CATALOG
        .iter()
        .filter(|(name, _)| name.starts_with(query))
        .copied()
        .collect()
}

pub fn command_help() -> &'static str {
    "/help             Show commands\n\
/status           Open live team & battlefield modal\n\
/goal [objective] Set goal & start autonomous loop (empty stops)\n\
/target [scope]   Set authorized engagement target & scope\n\
/auto             Pause or resume autonomous loop\n\
/agent            List agents (/agent-<id> for details)\n\
/compact          Compact main context into brief now\n\
/model [query]    Configure provider, model, or custom endpoint\n\
/resume           Browse saved sessions & resume instructions\n\
/new              Start a new durable run\n\
/update           Update pentesting to the latest release\n\
/exit             Save session, restore terminal, and quit\n\
!<cmd>            Run shell command in workspace"
}
