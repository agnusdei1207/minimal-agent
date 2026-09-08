use serde_json::json;
use std::time::Duration;

use minimal_agent::domain::{AgentId, AgentState};
use minimal_agent::engagement::Engagement;
use minimal_agent::runtime::{RuntimeEvent, TeamRuntime};

use super::constants::{
    HEADLESS_IDLE_AUTO, HEADLESS_IDLE_MANUAL, HEADLESS_MAX_WALL, HEADLESS_RETRY_BACKOFF,
    MAX_HEADLESS_RETRIES,
};
use super::observation::{HeadlessObservation, headless_journal_evidence, log_debug_event};

pub async fn run_headless(
    runtime: TeamRuntime,
    goal: String,
    engagement: Option<Engagement>,
    auto: bool,
) -> anyhow::Result<()> {
    let idle_window = if auto {
        HEADLESS_IDLE_AUTO
    } else {
        HEADLESS_IDLE_MANUAL
    };

    let observation = observe_headless(
        &runtime,
        &goal,
        engagement.as_ref(),
        HEADLESS_MAX_WALL,
        idle_window,
        auto,
    )
    .await;
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

    // Exit 0 even when the flag was not captured: the agent completed its
    // exploration legitimately. The harness classifies exit(0) + no flag as
    // "unsolved" (valid_for_score: true), whereas exit(1) would be recorded
    // as "runtime_fault" (excluded), hiding genuine misses from the benchmark
    // denominator. Reserve non-zero exits for actual crashes / panics.
    Ok(())
}

pub async fn observe_headless(
    runtime: &TeamRuntime,
    goal: &str,
    engagement: Option<&Engagement>,
    max_wall: Duration,
    idle_window: Duration,
    auto: bool,
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
    let mut retry_count = 0_usize;
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => break,
            turn = &mut submission, if !submitted => {
                submitted = true;
                match turn {
                    Ok(turn) if !turn.text.trim().is_empty() => observation.summary = turn.text.trim().to_owned(),
                    Ok(_) => {},
                    Err(error) => {
                        eprintln!("[headless] submission turn error: {error}");
                        if auto
                            && retry_count < MAX_HEADLESS_RETRIES
                            && runtime
                                .coordinator()
                                .inspect(&AgentId::main())
                                .is_ok_and(|agent| agent.state == AgentState::Waiting)
                        {
                            retry_count += 1;
                            eprintln!("[headless] retrying waiting main agent ({retry_count}/{MAX_HEADLESS_RETRIES})...");
                            tokio::time::sleep(HEADLESS_RETRY_BACKOFF).await;
                            let _ = runtime
                                .submit_user("Continue the goal from the latest brief and team inbox.")
                                .await;
                        }
                    }
                }
                idle_deadline = tokio::time::Instant::now() + idle_window;
            }
            event = events.recv() => {
                idle_deadline = tokio::time::Instant::now() + idle_window;
                match event {
                    Ok(event) => {
                        if minimal_agent::settings::debug_enabled() {
                            log_debug_event(&event);
                        }
                        if matches!(event, RuntimeEvent::TurnFinished { success: true, .. }) {
                            retry_count = 0;
                        }
                        observation.apply(event);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        if minimal_agent::settings::debug_enabled() {
                            eprintln!("[debug] broadcast lagged: {n} messages dropped");
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = tokio::time::sleep_until(idle_deadline), if submitted => {
                // Broadcast lag can lose TurnStarted/TurnFinished. Check the
                // runtime's active-turn counter before declaring the team idle.
                match tokio::time::timeout_at(deadline, runtime.wait_until_idle(Duration::from_millis(100))).await {
                    Ok(Ok(())) => {
                        if auto
                            && retry_count < MAX_HEADLESS_RETRIES
                            && runtime
                                .coordinator()
                                .inspect(&AgentId::main())
                                .is_ok_and(|agent| agent.state == AgentState::Waiting)
                        {
                            retry_count += 1;
                            eprintln!("[headless] main agent is waiting; retrying ({retry_count}/{MAX_HEADLESS_RETRIES})...");
                            tokio::time::sleep(HEADLESS_RETRY_BACKOFF).await;
                            let _ = runtime
                                .submit_user("Continue the goal from the latest brief and team inbox.")
                                .await;
                            idle_deadline = tokio::time::Instant::now() + idle_window;
                        } else {
                            break;
                        }
                    }
                    Err(_) => break,
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
