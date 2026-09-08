use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

use super::constants::MAX_BASH_STREAM_BYTES;
use super::context::ToolContext;
use super::error::ToolError;
use super::truncate::read_bounded;
use super::types::ToolOutput;

#[cfg(unix)]
pub fn platform_shell(command: &str) -> Command {
    // bash explicitly (not `sh`, which is dash on the runtime image) — the base
    // image ships bash and the agent's tooling assumes bash features (INTENT-0002 §3.16).
    let mut process = Command::new("bash");
    process.arg("-lc").arg(command);
    process
}

#[cfg(windows)]
pub fn platform_shell(command: &str) -> Command {
    let mut process = Command::new("powershell");
    process
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(command);
    process
}

pub async fn stop_child(child: &mut tokio::process::Child) {
    let _ = child.kill().await;
    let _ = child.wait().await;
}

/// Shared one-shot shell execution: spawn, bounded stdout/stderr capture,
/// timeout, and cancellation. Reused by `bash` and `tmux` so both obey the
/// exact same execution invariants.
pub async fn run_shell(
    command: String,
    context: &ToolContext,
    timeout: Duration,
) -> Result<ToolOutput, ToolError> {
    if command.trim().is_empty() {
        return Err(ToolError::InvalidArguments(
            "shell command cannot be empty".to_owned(),
        ));
    }
    if context.cancellation.is_cancelled() {
        return Err(ToolError::Cancelled);
    }
    let mut cmd = platform_shell(&command);
    cmd.current_dir(&context.workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ToolError::Process("shell stdout was not piped".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ToolError::Process("shell stderr was not piped".to_owned()))?;
    let deadline = tokio::time::Instant::now() + timeout;
    let streams = tokio::select! {
        _ = context.cancellation.cancelled() => {
            stop_child(&mut child).await;
            return Err(ToolError::Cancelled);
        },
        result = tokio::time::timeout_at(deadline, async {
            tokio::try_join!(
                read_bounded(stdout, MAX_BASH_STREAM_BYTES),
                read_bounded(stderr, MAX_BASH_STREAM_BYTES),
            )
        }) => match result {
            Ok(Ok(streams)) => streams,
            Ok(Err(error)) => {
                stop_child(&mut child).await;
                return Err(error);
            }
            Err(_) => {
                stop_child(&mut child).await;
                return Err(ToolError::TimedOut(timeout));
            }
        }
    };
    let status = tokio::select! {
        _ = context.cancellation.cancelled() => {
            stop_child(&mut child).await;
            return Err(ToolError::Cancelled);
        },
        result = tokio::time::timeout_at(deadline, child.wait()) => match result {
            Ok(status) => status?,
            Err(_) => {
                stop_child(&mut child).await;
                return Err(ToolError::TimedOut(timeout));
            }
        }
    };
    let (stdout, stderr) = streams;
    let mut content = String::from_utf8_lossy(&stdout).into_owned();
    if !stderr.is_empty() {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&String::from_utf8_lossy(&stderr));
    }
    Ok(ToolOutput {
        content,
        success: status.success(),
    })
}
