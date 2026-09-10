use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;

use pentesting::coordinator::AgentCoordinator;
use pentesting::journal::{JournalConfig, RunJournal};
use tempfile::tempdir;

#[test]
fn help_and_version_expose_the_small_public_surface() {
    let binary = env!("CARGO_BIN_EXE_pentesting");
    let version = Command::new(binary).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        "pentesting 0.200.1"
    );

    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("run"));
    assert!(help.contains("inspect"));
}

#[test]
fn inspect_reads_a_durable_run_without_a_provider() {
    let dir = tempdir().unwrap();
    let run_root = dir.path().join("run");
    let journal = Arc::new(RunJournal::open(&run_root, JournalConfig::default()).unwrap());
    AgentCoordinator::new(journal.clone(), "durable goal").unwrap();
    drop(journal);

    let output = Command::new(env!("CARGO_BIN_EXE_pentesting"))
        .args(["inspect", "--run"])
        .arg(&run_root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("main"));
    assert!(stdout.contains("durable goal"));
}

#[test]
fn run_opens_without_provider_environment_so_model_can_be_selected_in_process() {
    let dir = tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_pentesting"))
        .args(["run", "--plain", "--goal", "configure later", "--run"])
        .arg(dir.path().join("run"))
        .arg("--workspace")
        .arg(dir.path())
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENAI_MODEL")
        .env_remove("PENTESTING_API_KEY")
        .env_remove("PENTESTING_MODEL")
        .env_remove("MINIMAL_AGENT_API_KEY")
        .env_remove("MINIMAL_AGENT_MODEL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"/exit\n").unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("/help for commands"));
}
