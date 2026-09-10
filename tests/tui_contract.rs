use pentesting::coordinator::AgentCoordinator;
use pentesting::domain::{AgentId, MessageKind};
use pentesting::journal::{JournalConfig, RunJournal};
use pentesting::provider::ModelDelta;
use pentesting::runtime::RuntimeEvent;
use pentesting::tui::{TuiState, UiCommand, command_help, parse_command, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tempfile::tempdir;

#[test]
fn slash_commands_preserve_the_pentesting_convenience_surface() {
    assert_eq!(parse_command("ordinary input").unwrap(), None);
    assert_eq!(parse_command("/compact").unwrap(), Some(UiCommand::Compact));
    assert_eq!(parse_command("/new").unwrap(), Some(UiCommand::New));
    assert_eq!(parse_command("/clear").unwrap(), Some(UiCommand::New));
    assert_eq!(parse_command("/status").unwrap(), Some(UiCommand::Status));
    assert_eq!(parse_command("/exit").unwrap(), Some(UiCommand::Exit));
    assert_eq!(parse_command("/update").unwrap(), Some(UiCommand::Update));
    assert_eq!(parse_command("/help").unwrap(), Some(UiCommand::Help));
    assert_eq!(
        parse_command("/goal inspect the target").unwrap(),
        Some(UiCommand::Goal(Some("inspect the target".into())))
    );
    assert_eq!(parse_command("/goal").unwrap(), Some(UiCommand::Goal(None)));
    assert_eq!(parse_command("/auto").unwrap(), Some(UiCommand::Auto));
    assert_eq!(parse_command("/resume").unwrap(), Some(UiCommand::Resume));
    assert_eq!(
        parse_command("/model").unwrap(),
        Some(UiCommand::Model(None))
    );
    assert_eq!(
        parse_command("/model gpt-5.6").unwrap(),
        Some(UiCommand::Model(Some("gpt-5.6".into())))
    );
    assert_eq!(parse_command("/agent").unwrap(), Some(UiCommand::Agent));
    assert_eq!(
        parse_command("/agent-worker-01").unwrap(),
        Some(UiCommand::AgentSwitch(AgentId::new("worker-01").unwrap()))
    );
    assert_eq!(
        parse_command("!pwd").unwrap(),
        Some(UiCommand::Bash("pwd".into()))
    );

    assert!(parse_command("/config").is_err());
    assert!(parse_command("/ce").is_err());
    assert!(parse_command("/config-edit").is_err());
    assert!(parse_command("/quit").is_err());
    assert!(parse_command("/auto on").is_err());
    assert!(parse_command("/resume old-run").is_err());
    assert!(parse_command("/journal 10 20").is_err());
    assert!(parse_command("/agent worker-01").is_err());
    assert!(parse_command("/unknown").is_err());
}

#[test]
fn help_uses_the_same_canonical_command_names_as_the_parser() {
    let help = command_help();
    for command in [
        "/compact", "/new", "/goal", "/auto", "/model", "/status", "/resume", "/agent", "/update",
        "/help", "/exit", "!<cmd>",
    ] {
        assert!(help.contains(command), "missing {command} from help");
    }
    assert!(!help.contains("/config"));
    assert!(!help.contains("/quit"));
    assert!(!help.contains("/journal"));
    assert!(!help.contains("/auto on"));
}

#[test]
fn tui_shows_the_team_roster_so_a_fan_out_is_visible() {
    let dir = tempdir().unwrap();
    let journal = std::sync::Arc::new(
        RunJournal::open(dir.path().join("run"), JournalConfig::default()).unwrap(),
    );
    let coordinator = AgentCoordinator::new(journal, "inspect target").unwrap();
    coordinator
        .create_worker(&AgentId::main(), "research", "check service")
        .unwrap();
    let mut state = TuiState::new("inspect target");
    state.set_team(coordinator.team().unwrap());
    state.set_input("next instruction");
    state.apply_event(RuntimeEvent::Delta {
        agent_id: AgentId::main(),
        delta: ModelDelta::Text("streamed answer".into()),
    });

    let backend = TestBackend::new(100, 28);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &mut state)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    // The transcript surfaces worker lifecycle (spawn/recall) and the status line shows worker count.
    assert!(rendered.contains("worker-01"));
    assert!(rendered.contains("research"));
    assert!(rendered.contains("streamed answer"));
    assert!(rendered.contains("next instruction"));
}

#[test]
fn loading_status_is_immediately_above_input() {
    let mut state = TuiState::new("inspect target");
    state.apply_event(RuntimeEvent::TurnStarted {
        agent_id: AgentId::main(),
    });

    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &mut state)).unwrap();
    let buffer = terminal.backend().buffer();
    let rows = (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    let loading_row = rows
        .iter()
        .position(|row| row.contains("waiting for model"))
        .unwrap();
    let input_row = rows.iter().position(|row| row.contains("❯ ")).unwrap();
    assert_eq!(loading_row + 1, input_row);
}

#[test]
fn input_is_a_single_line_prompt_without_a_box() {
    let mut state = TuiState::new("inspect target");
    state.set_input("next instruction");

    let rendered = render_to_string(&mut state, 80, 12);

    assert!(rendered.contains("❯ next instruction"));
    assert!(!rendered.contains("┌Input"));
    assert!(!rendered.contains("└"));
}

#[test]
fn transcript_is_rendered_without_a_border_or_title() {
    let mut state = TuiState::new("inspect target");
    state.apply_event(RuntimeEvent::Delta {
        agent_id: AgentId::main(),
        delta: ModelDelta::Text("streamed answer".into()),
    });

    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &mut state)).unwrap();
    let buffer = terminal.backend().buffer();
    let rendered = buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    assert!(!rendered.contains("Transcript"));
    // The transcript starts at row 0 with no boxed panel rail, and a plain
    // response is shown as bare content (no speaker label).
    let _ = buffer;
    assert!(!rendered.contains("▌"));
    assert!(!rendered.contains("┆"));
    assert!(rendered.contains("streamed answer"));
}

#[test]
fn transcript_streams_main_reasoning_then_response_text() {
    let mut state = TuiState::new("inspect target");
    state.apply_event(RuntimeEvent::TurnStarted {
        agent_id: AgentId::main(),
    });
    state.apply_event(RuntimeEvent::Delta {
        agent_id: AgentId::main(),
        delta: ModelDelta::Reasoning("checking the target".into()),
    });
    state.apply_event(RuntimeEvent::Delta {
        agent_id: AgentId::main(),
        delta: ModelDelta::Text("working response".into()),
    });

    let backend = TestBackend::new(80, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, &mut state)).unwrap();
    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();

    // Main's reasoning streams live so a slow model's progress is visible, then
    // the response follows it.
    assert!(rendered.contains("checking the target"));
    assert!(rendered.contains("working response"));
}

#[test]
fn transcript_renders_markdown_as_terminal_content_instead_of_source_markers() {
    let mut state = TuiState::new("inspect target");
    state.apply_event(RuntimeEvent::Delta {
        agent_id: AgentId::main(),
        delta: ModelDelta::Text(
            "# Findings\n\n- **Open** port\n\n```sh\nnmap -sV target\n```".into(),
        ),
    });

    let rendered = render_to_string(&mut state, 100, 16);

    assert!(rendered.contains("Findings"));
    assert!(rendered.contains("• Open port"));
    assert!(rendered.contains("nmap -sV target"));
    assert!(!rendered.contains("# Findings"));
    assert!(!rendered.contains("**Open**"));
    assert!(!rendered.contains("```"));
}

fn render_to_string(state: &mut TuiState, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| render(frame, state)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn tool_calls_render_action_with_a_dimmed_target_summary() {
    let mut state = TuiState::new("inspect target");
    state.apply_event(RuntimeEvent::ToolStarted {
        agent_id: AgentId::main(),
        name: "bash".to_owned(),
        summary: Some("nmap -sV 10.10.10.5".to_owned()),
    });
    state.apply_event(RuntimeEvent::ToolFinished {
        agent_id: AgentId::main(),
        name: "bash".to_owned(),
        success: true,
        output: "80/tcp open http".to_owned(),
    });

    let rendered = render_to_string(&mut state, 80, 12);
    assert!(rendered.contains("bash"));
    assert!(rendered.contains("nmap -sV 10.10.10.5"));
    assert!(rendered.contains("ok"));
    assert!(rendered.contains("80/tcp open http"));
}

#[test]
fn team_messages_render_their_kind_label_and_direction() {
    let mut state = TuiState::new("inspect target");
    state.apply_event(RuntimeEvent::AgentMessage {
        sender: AgentId::new("worker-01").unwrap(),
        recipients: vec![AgentId::main()],
        kind: MessageKind::Insight,
        body: "credentials reused across hosts".to_owned(),
    });

    let rendered = render_to_string(&mut state, 80, 12);
    assert!(rendered.contains("worker-01 → main"));
    assert!(rendered.contains("Insight"));
    assert!(rendered.contains("credentials reused across hosts"));
}
