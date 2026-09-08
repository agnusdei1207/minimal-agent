use anyhow::Context;
use std::future::Future;
use std::io;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};

use minimal_agent::domain::{AgentId, MAX_USER_INPUT_BYTES};
use minimal_agent::runtime::TeamRuntime;
use minimal_agent::tui::{UiCommand, command_help, parse_command};

use super::inspect::agent_json;

pub async fn run_plain(runtime: TeamRuntime, mut auto_enabled: bool) -> anyhow::Result<()> {
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

pub async fn interrupt_on_ctrl_c<T, E>(
    operation: impl Future<Output = Result<T, E>>,
) -> anyhow::Result<Option<T>>
where
    E: std::error::Error + Send + Sync + 'static,
{
    select_operation_or_shutdown(operation, tokio::signal::ctrl_c()).await
}

pub async fn select_operation_or_shutdown<T, E>(
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

pub async fn read_bounded_line<R>(reader: &mut R, max_bytes: usize) -> io::Result<Option<String>>
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

pub fn print_status(runtime: &TeamRuntime) -> anyhow::Result<()> {
    let team = runtime.coordinator().live_team()?;
    let team_json: Vec<serde_json::Value> = team.iter().map(agent_json).collect();
    println!("{}", serde_json::to_string_pretty(&team_json)?);
    Ok(())
}

pub fn print_agent(runtime: &TeamRuntime, agent: &AgentId) -> anyhow::Result<()> {
    let snapshot = runtime.coordinator().inspect(agent)?;
    let brief = runtime.brief(agent)?;
    println!(
        "{}\n\n{brief}",
        serde_json::to_string_pretty(&agent_json(&snapshot))?
    );
    Ok(())
}
