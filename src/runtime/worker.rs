use serde_json::json;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::brief::BriefDraft;
use crate::compaction::{
    CompactionError, CompactionOutcome, ContextEntry, LiveReason, SemanticCompactor,
};
use crate::coordinator::CoordinatorError;
use crate::domain::{AgentId, AgentState, MessageKind, SequenceRange, estimate_tokens};
use crate::journal::{JournalEvent, TranscriptRole};
use crate::provider::{ModelMessage, ModelRole, ModelTurn, ProviderFault, ToolCall};
use crate::tools::{ToolContext, ToolError, names as tool_names};

use super::constants::{ACTIVITY_WAIT, AUTONOMOUS_RETRY_BACKOFF};
use super::engine::{MainCommand, RuntimeInner};
use super::error::RuntimeError;
use super::events::{RuntimeEvent, TurnResult, record_model_delta};
use super::session::{ActiveTurn, AgentSession, SessionRecord, unique_ids};

// An unwinding worker must release the same subtree as an ordinary terminal fault.
// Process aborts cannot run destructors and remain a restart concern.
pub struct WorkerExit {
    pub inner: Arc<RuntimeInner>,
    pub id: AgentId,
}

impl Drop for WorkerExit {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        let _ = self.inner.coordinator.mark_terminal(
            &self.id,
            &self.id,
            AgentState::Faulted,
            "worker task panicked",
        );
        let _ = self.inner.briefs.remove_projection(&self.id);
        let _ = self.inner.events.send(RuntimeEvent::TeamChanged);
    }
}

pub async fn handle_main_command(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    main_id: &AgentId,
    auto: &mut watch::Receiver<bool>,
    observed_activity: &mut u64,
    auto_pending: &mut bool,
    command: MainCommand,
) -> bool {
    match command {
        MainCommand::User { text, reply } => {
            let result = run_main_turn(inner, session, text, 0).await;
            *observed_activity = session.last_request_activity;
            let auto_enabled = *auto.borrow_and_update();
            // A user-submitted turn that faults stops the loop (goes Waiting); the
            // autonomous continuation loop is what retries recoverable faults.
            *auto_pending = result.as_ref().is_ok_and(|turn| !turn.finalized) && auto_enabled;
            let _ = reply.send(result);
            false
        }
        MainCommand::Compact { reply } => {
            let _ = reply.send(run_main_compaction(inner, session).await);
            false
        }
        MainCommand::Goal { objective, reply } => {
            let _ = reply.send(update_main_goal(inner, objective));
            false
        }
        MainCommand::Bash { command, reply } => {
            let _ = reply.send(run_direct_bash(inner, session, main_id, command).await);
            false
        }
        MainCommand::Stop => true,
    }
}

pub async fn main_driver(
    inner: Arc<RuntimeInner>,
    mut receiver: mpsc::Receiver<MainCommand>,
    mut session: AgentSession,
) {
    let main_id = AgentId::main();
    let mut auto = inner.auto.subscribe();
    let mut auto_pending = false;
    let mut observed_activity = 0;
    // Consecutive recoverable-fault retries on the autonomous loop. Reset on any
    // successful turn so the user always sees a fresh count after recovery.
    let mut retry_count: u64 = 0;
    loop {
        if inner.shutdown.is_cancelled() {
            break;
        }
        if auto_pending && *auto.borrow() {
            match receiver.try_recv() {
                Ok(command) => {
                    if handle_main_command(
                        &inner,
                        &mut session,
                        &main_id,
                        &mut auto,
                        &mut observed_activity,
                        &mut auto_pending,
                        command,
                    )
                    .await
                    {
                        break;
                    }
                    continue;
                }
                Err(mpsc::error::TryRecvError::Disconnected) => break,
                Err(mpsc::error::TryRecvError::Empty) => {
                    let result = run_main_turn(
                        &inner,
                        &mut session,
                        "Continue the goal from the latest brief and team inbox.".to_owned(),
                        retry_count,
                    )
                    .await;
                    observed_activity = session.last_request_activity;
                    let auto_enabled = *auto.borrow_and_update();
                    // Keep driving the goal autonomously, turn after turn, until the
                    // model finalizes it (report final) or auto is turned off. A
                    // recoverable provider fault (timeout, rate limit, transport) is
                    // retried after a short backoff — the fault is already shown in
                    // the transcript with the retry count, so retries are visible. A
                    // non-recoverable fault (auth, payment, config) stops the loop
                    // and waits for activity.
                    let keep_going = match &result {
                        Ok(turn) => !turn.finalized,
                        Err(error) => recoverable_wait_reason(error).is_some(),
                    };
                    let retrying = result.is_err() && keep_going && auto_enabled;
                    if retrying {
                        retry_count = retry_count.saturating_add(1);
                    } else {
                        retry_count = 0;
                    }
                    auto_pending = keep_going && auto_enabled;
                    if retrying {
                        tokio::select! {
                            _ = tokio::time::sleep(AUTONOMOUS_RETRY_BACKOFF) => {}
                            _ = inner.shutdown.cancelled() => auto_pending = false,
                        }
                    }
                    continue;
                }
            }
        }

        tokio::select! {
            biased;
            command = receiver.recv() => match command {
                Some(command) => {
                    if handle_main_command(
                        &inner,
                        &mut session,
                        &main_id,
                        &mut auto,
                        &mut observed_activity,
                        &mut auto_pending,
                        command,
                    ).await {
                        break;
                    }
                }
                None => break,
            },
            _ = inner.shutdown.cancelled() => break,
            changed = auto.changed() => {
                if changed.is_err() {
                    break;
                }
                if *auto.borrow() {
                    auto_pending = true;
                }
            },
            activity = inner.coordinator.wait_for_activity_after(&main_id, observed_activity, ACTIVITY_WAIT) => {
                if let Ok(revision) = activity {
                    observed_activity = revision;
                    let projection = inner.sync_main_brief();
                    match projection {
                        Ok(()) => auto_pending = *auto.borrow_and_update(),
                        Err(error) => {
                            auto_pending = false;
                            let message = error.to_string();
                            let _ = inner.events.send(RuntimeEvent::Fault {
                                agent_id: main_id.clone(),
                                message: message.clone(),
                            });
                            let _ = inner.journal.append_sync(JournalEvent::Fault {
                                agent_id: Some(main_id.clone()),
                                code: "brief_projection".to_owned(),
                                message,
                            });
                        }
                    }
                }
            }
        }
    }
}

pub async fn worker_driver(
    inner: Arc<RuntimeInner>,
    id: AgentId,
    mut session: AgentSession,
    mut initial: Option<String>,
) {
    let mut auto = inner.auto.subscribe();
    let mut observed_activity = 0;
    loop {
        if inner.shutdown.is_cancelled()
            || inner
                .coordinator
                .inspect(&id)
                .is_ok_and(|agent| agent.state.is_terminal())
        {
            break;
        }
        if !*auto.borrow() {
            tokio::select! {
                _ = inner.shutdown.cancelled() => break,
                changed = auto.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    continue;
                },
            }
        }

        let input = if initial.take().is_some() {
            match inner.coordinator.inspect(&id) {
                Ok(agent) => format!("Begin or resume the current assignment: {}", agent.task),
                Err(_) => break,
            }
        } else {
            let revision = match inner.coordinator.activity_revision(&id) {
                Ok(revision) => revision,
                Err(_) => break,
            };
            if revision <= observed_activity {
                tokio::select! {
                    _ = inner.shutdown.cancelled() => break,
                    changed = auto.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        continue;
                    },
                    activity = inner.coordinator.wait_for_activity_after(&id, observed_activity, ACTIVITY_WAIT) => {
                        match activity {
                            Ok(_) => {}
                            Err(CoordinatorError::WaitTimedOut) => continue,
                            Err(_) => break,
                        }
                    }
                }
            }
            match inner.coordinator.inspect(&id) {
                Ok(agent) => format!(
                    "Integrate new team activity and continue the current assignment: {}",
                    agent.task
                ),
                Err(_) => break,
            }
        };
        if inner
            .coordinator
            .inspect(&id)
            .is_ok_and(|agent| agent.state == AgentState::Waiting)
        {
            let _ = inner
                .coordinator
                .mark_running(&id, "new assignment or message");
        }
        let turn_interrupt = CancellationToken::new();
        match run_agent_turn(&inner, &mut session, &id, input, &turn_interrupt, 0).await {
            Ok(_) => observed_activity = session.last_request_activity,
            Err(RuntimeError::Cancelled) => {
                if inner
                    .coordinator
                    .inspect(&id)
                    .is_ok_and(|agent| agent.state == AgentState::Recalling)
                {
                    let _ = inner.coordinator.mark_terminal(
                        &id,
                        &id,
                        AgentState::Stopped,
                        "recall completed",
                    );
                }
                break;
            }
            Err(RuntimeError::Stopped) => break,
            Err(error) => {
                if let Some(reason) = recoverable_wait_reason(&error) {
                    let _ = inner.coordinator.mark_waiting(&id, reason);
                    observed_activity = session.last_request_activity;
                } else {
                    let _ = inner.coordinator.mark_terminal(
                        &id,
                        &id,
                        AgentState::Faulted,
                        error.to_string(),
                    );
                    break;
                }
            }
        }
    }
    if inner
        .coordinator
        .inspect(&id)
        .is_ok_and(|agent| agent.state.is_terminal())
    {
        let _ = inner.briefs.remove_projection(&id);
        let _ = inner.events.send(RuntimeEvent::TeamChanged);
    }
}

pub async fn run_main_turn(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    input: String,
    attempt: u64,
) -> Result<TurnResult, RuntimeError> {
    let main = AgentId::main();
    if inner
        .coordinator
        .inspect(&main)
        .is_ok_and(|agent| agent.state == AgentState::Waiting)
    {
        inner
            .coordinator
            .mark_running(&main, "explicit input or autonomous resume")?;
    }
    inner.sync_main_brief()?;
    let turn_interrupt = CancellationToken::new();
    inner
        .main_turn_interrupt
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .replace(turn_interrupt.clone());
    let mut result = run_agent_turn(inner, session, &main, input, &turn_interrupt, attempt).await;
    // Catch steering that raced this turn's end so no instruction is dropped:
    // address it in a follow-up turn that keeps the same session context.
    while result.is_ok() {
        let pending = {
            let mut queue = inner
                .main_steer
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if queue.is_empty() {
                break;
            }
            queue.drain(..).collect::<Vec<_>>().join("\n\n")
        };
        result = run_agent_turn(inner, session, &main, pending, &turn_interrupt, attempt).await;
    }
    inner
        .main_turn_interrupt
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    // The per-attempt provider/tool fault is already surfaced through
    // `RuntimeEvent::Fault` inside `run_agent_turn_inner` (with the attempt count),
    // so do not emit a second identical Fault here. Just transition main to
    // Waiting so the autonomous loop retries the recoverable fault.
    if let Err(error) = &result
        && recoverable_wait_reason(error).is_some()
    {
        inner
            .coordinator
            .mark_waiting(&main, recoverable_wait_reason(error).unwrap_or_default())?;
        inner.sync_main_brief()?;
    }
    result
}

pub async fn run_main_compaction(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
) -> Result<bool, RuntimeError> {
    let main = AgentId::main();
    let inbox = inner.coordinator.inbox(&main)?;
    session.integrate_inbox(&inbox)?;
    let turn_interrupt = CancellationToken::new();
    let result = compact_if_needed(inner, session, &main, &turn_interrupt).await;
    match &result {
        Ok(true)
            if inner
                .coordinator
                .inspect(&main)
                .is_ok_and(|agent| agent.state == AgentState::Waiting) =>
        {
            inner
                .coordinator
                .mark_running(&main, "context curation recovered")?;
        }
        Err(error) => {
            if let Some(reason) = recoverable_wait_reason(error) {
                inner.coordinator.mark_waiting(&main, reason)?;
            }
        }
        _ => {}
    }
    inner.sync_main_brief()?;
    result
}

pub fn recoverable_wait_reason(error: &RuntimeError) -> Option<String> {
    match error {
        // A missing or invalid provider configuration is terminal: retrying the
        // same unconfigured provider only replays the identical fault, and in the
        // main driver it would spin the autonomous loop on a non-recoverable
        // error (emitting a duplicate Fault line every turn). Stop and wait.
        RuntimeError::Provider(ProviderFault::Configuration { .. }) => None,
        RuntimeError::Provider(error) => Some(provider_wait_reason(error).to_owned()),
        RuntimeError::Compaction(CompactionError::Provider(error)) => {
            Some(provider_wait_reason(error).to_owned())
        }
        RuntimeError::Compaction(CompactionError::ContextCompactionBlocked { .. }) => {
            Some("context curation blocked".to_owned())
        }
        RuntimeError::CompactionTimedOut(_) => Some("context curation timed out".to_owned()),
        _ => None,
    }
}

pub fn provider_wait_reason(error: &ProviderFault) -> &'static str {
    match error {
        ProviderFault::RateLimited { .. } => "provider unavailable: rate limited",
        ProviderFault::PaymentRequired { .. } => "provider unavailable: payment required",
        ProviderFault::Authentication { .. } => "provider unavailable: authentication",
        ProviderFault::ContextOverflow { .. } => "provider unavailable: context overflow",
        ProviderFault::Upstream { .. } => "provider unavailable: upstream",
        ProviderFault::Http { .. } => "provider unavailable: HTTP error",
        ProviderFault::Transport { .. } => "provider unavailable: transport",
        ProviderFault::Stream { .. } => "provider unavailable: stream",
        ProviderFault::InvalidResponse { .. }
        | ProviderFault::EmptyCompletion
        | ProviderFault::MalformedToolCall { .. } => "provider unavailable: invalid response",
        ProviderFault::Configuration { .. } => "provider unavailable: configuration",
    }
}

pub async fn run_agent_turn(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    input: String,
    turn_interrupt: &CancellationToken,
    attempt: u64,
) -> Result<TurnResult, RuntimeError> {
    let _ = inner.events.send(RuntimeEvent::TurnStarted {
        agent_id: agent_id.clone(),
    });
    let result =
        run_agent_turn_inner(inner, session, agent_id, input, turn_interrupt, attempt).await;
    let _ = inner.events.send(RuntimeEvent::TurnFinished {
        agent_id: agent_id.clone(),
        success: result.is_ok(),
    });
    result
}

/// Journal one user message and append it to an agent's session.
pub fn push_user_record(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    text: String,
) -> Result<(), RuntimeError> {
    let ack = inner.journal.append_sync(JournalEvent::Transcript {
        agent_id: agent_id.clone(),
        role: TranscriptRole::User,
        content: text.clone(),
        complete: true,
        atomic_group: None,
    })?;
    session.records.push(SessionRecord {
        message: ModelMessage::new(ModelRole::User, text.clone()),
        context: ContextEntry::completed(SequenceRange::new(ack.sequence, ack.sequence)?, text),
    });
    Ok(())
}

/// Drain user input queued for the main turn into the session as user messages.
/// Returns whether anything was drained. No-op for non-main agents.
pub fn drain_main_steering(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
) -> Result<bool, RuntimeError> {
    if !agent_id.is_main() {
        return Ok(false);
    }
    let mut drained = false;
    while let Some(text) = inner
        .main_steer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pop_front()
    {
        push_user_record(inner, session, agent_id, text)?;
        drained = true;
    }
    Ok(drained)
}

pub fn main_steer_pending(inner: &Arc<RuntimeInner>) -> bool {
    !inner
        .main_steer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_empty()
}

pub async fn run_agent_turn_inner(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    input: String,
    turn_interrupt: &CancellationToken,
    attempt: u64,
) -> Result<TurnResult, RuntimeError> {
    let _active = ActiveTurn::new(inner.clone());
    session.integrate_inbox(&inner.coordinator.inbox(agent_id)?)?;
    push_user_record(inner, session, agent_id, input)?;

    let max_turns = if inner.config.max_model_turns == 0 {
        usize::MAX
    } else {
        inner.config.max_model_turns
    };
    for turn_idx in 1..=max_turns {
        if inner.shutdown.is_cancelled() {
            return Err(RuntimeError::Stopped);
        }
        if turn_interrupt.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        let cancellation = inner.coordinator.cancellation_token(agent_id)?;
        if cancellation.is_cancelled() {
            return Err(RuntimeError::Cancelled);
        }
        // Fold any input the user typed while this turn was running into the
        // session as a fresh user message before building the next request.
        drain_main_steering(inner, session, agent_id)?;
        let (compaction_activity, inbox) = inner.coordinator.inbox_snapshot(agent_id)?;
        session.integrate_inbox(&inbox)?;
        session.last_request_activity = compaction_activity;
        compact_if_needed(inner, session, agent_id, turn_interrupt).await?;
        let (request_activity, latest_inbox) = inner.coordinator.inbox_snapshot(agent_id)?;
        session.integrate_inbox(&latest_inbox)?;
        let request = build_request(inner, session, agent_id)?;
        session.last_request_activity = request_activity;
        let (delta_tx, mut delta_rx) = tokio::sync::mpsc::unbounded_channel();
        let delta_done = CancellationToken::new();
        let collector_done = delta_done.clone();
        let events = inner.events.clone();
        let streamed_agent = agent_id.clone();
        let collector = tokio::spawn(async move {
            let mut partial = String::new();
            loop {
                tokio::select! {
                    biased;
                    delta = delta_rx.recv() => match delta {
                        Some(delta) => record_model_delta(
                            &mut partial,
                            &events,
                            &streamed_agent,
                            delta,
                        ),
                        None => break,
                    },
                    _ = collector_done.cancelled() => {
                        while let Ok(delta) = delta_rx.try_recv() {
                            record_model_delta(
                                &mut partial,
                                &events,
                                &streamed_agent,
                                delta,
                            );
                        }
                        break;
                    }
                }
            }
            partial
        });
        let permit = tokio::select! {
            biased;
            _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
            _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
            _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
            permit = inner.requests.clone().acquire_owned() => {
                permit.map_err(|_| RuntimeError::Stopped)?
            }
        };
        let mut provider_call = Box::pin(inner.provider.complete(request, Some(delta_tx)));
        let response = tokio::select! {
            biased;
            _ = inner.shutdown.cancelled() => {
                delta_done.cancel();
                drop(provider_call);
                drop(permit);
                let _ = collector.await;
                return Err(RuntimeError::Stopped);
            }
            _ = cancellation.cancelled() => {
                delta_done.cancel();
                drop(provider_call);
                drop(permit);
                let _ = collector.await;
                return Err(RuntimeError::Cancelled);
            }
            _ = turn_interrupt.cancelled() => {
                delta_done.cancel();
                drop(provider_call);
                drop(permit);
                let _ = collector.await;
                return Err(RuntimeError::Cancelled);
            }
            response = provider_call.as_mut() => response,
        };
        delta_done.cancel();
        drop(provider_call);
        drop(permit);
        let partial = collector.await.unwrap_or_default();
        let turn = match response {
            Ok(turn) => turn,
            Err(error) => {
                let partial_len = partial.len();
                eprintln!(
                    "[runtime] provider error for agent '{}' (attempt {attempt}, model-turn {turn_idx}/{max_turns}, partial_len={}): {error}",
                    agent_id, partial_len
                );
                if !partial.is_empty() {
                    let ack = inner.journal.append_sync(JournalEvent::Transcript {
                        agent_id: agent_id.clone(),
                        role: TranscriptRole::Assistant,
                        content: partial.clone(),
                        complete: false,
                        atomic_group: None,
                    })?;
                    let has_corrupted_tokens = partial.contains("<｜") || partial.contains("<|");
                    let is_malformed_or_empty = matches!(
                        error,
                        ProviderFault::MalformedToolCall { .. } | ProviderFault::EmptyCompletion
                    );
                    if !has_corrupted_tokens && !is_malformed_or_empty {
                        session.records.push(SessionRecord {
                            message: ModelMessage::new(ModelRole::Assistant, partial.clone()),
                            context: ContextEntry::completed(
                                SequenceRange::new(ack.sequence, ack.sequence)?,
                                partial,
                            )
                            .protect(LiveReason::PartialOutput),
                        });
                    } else {
                        eprintln!(
                            "[runtime] omitted corrupted/partial assistant output from session records to protect retry context"
                        );
                    }
                }
                let _ = inner.events.send(RuntimeEvent::Fault {
                    agent_id: agent_id.clone(),
                    message: format!(
                        "provider 실패 (자율 재시도 {attempt}회째, model-turn {turn_idx}/{max_turns}): {error}"
                    ),
                });
                inner.journal.append_sync(JournalEvent::Fault {
                    agent_id: Some(agent_id.clone()),
                    code: "provider".to_owned(),
                    message: format!(
                        "provider failure (autonomous retry {attempt}, model-turn {turn_idx}/{max_turns}): {error}"
                    ),
                })?;
                return Err(RuntimeError::Provider(error));
            }
        };
        let tool_group = record_assistant_turn(inner, session, agent_id, &turn)?;
        let consumed_ids = latest_inbox
            .iter()
            .map(|delivery| delivery.message.id)
            .collect::<Vec<_>>();
        inner.coordinator.consume(agent_id, &consumed_ids)?;
        session.acknowledge_inbox(&latest_inbox);
        session.release_recovery_protection();

        if turn.tool_calls.is_empty() {
            if !turn.text.is_empty() {
                let _ = inner.events.send(RuntimeEvent::Assistant {
                    agent_id: agent_id.clone(),
                    text: turn.text.clone(),
                });
            }
            // If the user steered while the model was answering, keep the turn
            // alive and address the new instruction instead of ending here.
            if agent_id.is_main() && main_steer_pending(inner) {
                continue;
            }
            return Ok(TurnResult {
                text: turn.text,
                finalized: false,
            });
        }

        let mut finalized = false;
        for call in turn.tool_calls {
            let is_final = call.name == "report"
                && call.arguments.get("op").and_then(serde_json::Value::as_str) == Some("final");
            let executed = execute_tool_call(
                inner,
                session,
                agent_id,
                &call,
                tool_group.as_deref(),
                &cancellation,
                turn_interrupt,
            )
            .await?;
            finalized |= is_final && executed.success;
            if inner
                .coordinator
                .inspect(agent_id)
                .is_ok_and(|agent| agent.state.is_terminal())
            {
                return Ok(TurnResult {
                    text: turn.text,
                    finalized: true,
                });
            }
        }
        if agent_id.is_main() {
            inner.sync_main_brief()?;
        }
        if finalized {
            return Ok(TurnResult {
                text: turn.text,
                finalized,
            });
        }
    }
    Err(RuntimeError::ModelTurnLimit)
}

pub fn record_assistant_turn(
    inner: &RuntimeInner,
    session: &mut AgentSession,
    agent_id: &AgentId,
    turn: &ModelTurn,
) -> Result<Option<String>, RuntimeError> {
    let tool_group =
        (!turn.tool_calls.is_empty()).then(|| format!("model-turn-{}", Uuid::new_v4().simple()));
    let journal_content = if turn.text.is_empty() && !turn.tool_calls.is_empty() {
        serde_json::to_string(&turn.tool_calls)?
    } else {
        turn.text.clone()
    };
    let ack = inner.journal.append_sync(JournalEvent::Transcript {
        agent_id: agent_id.clone(),
        role: TranscriptRole::Assistant,
        content: journal_content.clone(),
        complete: true,
        atomic_group: tool_group.clone(),
    })?;
    let mut message = ModelMessage::new(ModelRole::Assistant, turn.text.clone());
    message.tool_calls = turn.tool_calls.clone();
    let mut context = ContextEntry::completed(
        SequenceRange::new(ack.sequence, ack.sequence)?,
        journal_content,
    );
    if let Some(group) = &tool_group {
        context = context.atomic(group.clone());
    }
    session.records.push(SessionRecord { message, context });
    Ok(tool_group)
}

pub fn update_main_goal(
    inner: &Arc<RuntimeInner>,
    objective: Option<String>,
) -> Result<(), RuntimeError> {
    let main = AgentId::main();
    inner
        .coordinator
        .set_goal(&main, objective.unwrap_or_default())?;
    inner.sync_main_brief()?;
    let _ = inner.events.send(RuntimeEvent::TeamChanged);
    Ok(())
}

pub struct ExecutedTool {
    pub content: String,
    pub success: bool,
}

pub async fn execute_tool_call(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    call: &ToolCall,
    atomic_group: Option<&str>,
    cancellation: &CancellationToken,
    turn_interrupt: &CancellationToken,
) -> Result<ExecutedTool, RuntimeError> {
    let _ = inner.events.send(RuntimeEvent::ToolStarted {
        agent_id: agent_id.clone(),
        name: call.name.clone(),
        summary: crate::prompt::tool_call_summary(&call.name, &call.arguments),
    });
    let call_ack = inner.journal.append_sync(JournalEvent::ToolCall {
        agent_id: agent_id.clone(),
        call_id: call.id.clone(),
        name: call.name.clone(),
        arguments: call.arguments.clone(),
    })?;
    let context = ToolContext::with_briefs(
        agent_id.clone(),
        &inner.workspace,
        inner.coordinator.clone(),
        inner.journal.clone(),
        inner.briefs.clone(),
    )?;
    let result = tokio::select! {
        biased;
        _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
        result = inner.tools.execute(&call.name, call.arguments.clone(), &context) => result,
    };
    let (content, success) = match result {
        Ok(output) => (output.content, output.success),
        Err(ToolError::Cancelled) => return Err(RuntimeError::Cancelled),
        Err(error) => (format!("tool error: {error}"), false),
    };
    let result_ack = inner.journal.append_sync(JournalEvent::ToolResult {
        agent_id: agent_id.clone(),
        call_id: call.id.clone(),
        content: content.clone(),
        success,
    })?;
    let mut message = ModelMessage::new(ModelRole::Tool, content.clone());
    message.tool_call_id = Some(call.id.clone());
    session.records.push(SessionRecord {
        message,
        context: ContextEntry::completed(
            SequenceRange::new(call_ack.sequence, result_ack.sequence)?,
            format!(
                "tool={} arguments={}\nresult={}",
                call.name, call.arguments, content
            ),
        )
        .atomic(atomic_group.map_or_else(|| call.id.clone(), str::to_owned)),
    });
    if success
        && call.name == "team"
        && let Some(event) = team_message_event(agent_id, &call.arguments)
    {
        let _ = inner.events.send(event);
    }
    let _ = inner.events.send(RuntimeEvent::ToolFinished {
        agent_id: agent_id.clone(),
        name: call.name.clone(),
        success,
        output: content.clone(),
    });
    Ok(ExecutedTool { content, success })
}

pub fn team_message_event(sender: &AgentId, arguments: &serde_json::Value) -> Option<RuntimeEvent> {
    let op = arguments.get("op")?.as_str()?;
    let body = arguments.get("body")?.as_str()?.to_owned();
    let (recipients, kind) = match op {
        "send" => {
            let recipients = arguments
                .get("to")?
                .as_array()?
                .iter()
                .map(|recipient| {
                    recipient
                        .as_str()
                        .and_then(|value| AgentId::new(value).ok())
                })
                .collect::<Option<Vec<_>>>()?;
            let kind = arguments
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .and_then(MessageKind::parse)
                .unwrap_or(MessageKind::Progress);
            (recipients, kind)
        }
        "finish" => (vec![AgentId::main()], MessageKind::Final),
        _ => return None,
    };
    Some(RuntimeEvent::AgentMessage {
        sender: sender.clone(),
        recipients,
        kind,
        body,
    })
}

pub async fn run_direct_bash(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    command: String,
) -> Result<String, RuntimeError> {
    let call = ToolCall {
        id: format!("ui-bash-{}", Uuid::new_v4().simple()),
        name: tool_names::BASH.to_owned(),
        arguments: json!({"command": command}),
    };
    let tool_group = record_assistant_turn(
        inner,
        session,
        agent_id,
        &ModelTurn {
            text: String::new(),
            tool_calls: vec![call.clone()],
            usage: None,
            finish_reason: Some("tool_calls".to_owned()),
        },
    )?;
    let cancellation = inner.coordinator.cancellation_token(agent_id)?;
    let turn_interrupt = CancellationToken::new();
    Ok(execute_tool_call(
        inner,
        session,
        agent_id,
        &call,
        tool_group.as_deref(),
        &cancellation,
        &turn_interrupt,
    )
    .await?
    .content)
}

pub async fn compact_if_needed(
    inner: &Arc<RuntimeInner>,
    session: &mut AgentSession,
    agent_id: &AgentId,
    turn_interrupt: &CancellationToken,
) -> Result<bool, RuntimeError> {
    let brief = inner.briefs.read(agent_id)?;
    let entries = session
        .records
        .iter()
        .map(|record| record.context.clone())
        .collect::<Vec<_>>();
    let fixed_tokens = estimate_tokens(&build_system(inner, agent_id, "")?).saturating_add(
        estimate_tokens(&serde_json::to_string(&inner.tools.definitions())?),
    );
    let projected_tokens = fixed_tokens
        .saturating_add(estimate_tokens(&brief))
        .saturating_add(
            entries
                .iter()
                .map(|entry| estimate_tokens(&entry.content))
                .sum::<u64>(),
        );
    if !inner.budget.needs_compaction(projected_tokens) {
        return Ok(false);
    }
    let cancellation = inner.coordinator.cancellation_token(agent_id)?;
    let permit = tokio::select! {
        biased;
        _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
        permit = inner.requests.clone().acquire_owned() => {
            permit.map_err(|_| RuntimeError::Stopped)?
        }
    };
    let compactor = SemanticCompactor::new(inner.provider.clone(), inner.config.compaction);
    let mut compaction =
        Box::pin(compactor.compact(agent_id, inner.budget, &brief, entries, fixed_tokens));
    let result: Result<CompactionOutcome, RuntimeError> = tokio::select! {
        _ = inner.shutdown.cancelled() => return Err(RuntimeError::Stopped),
        _ = cancellation.cancelled() => return Err(RuntimeError::Cancelled),
        _ = turn_interrupt.cancelled() => return Err(RuntimeError::Cancelled),
        result = tokio::time::timeout(inner.config.compaction_timeout, compaction.as_mut()) => {
            match result {
                Ok(result) => result.map_err(RuntimeError::from),
                Err(_) => Err(RuntimeError::CompactionTimedOut(inner.config.compaction_timeout)),
            }
        },
    };
    drop(permit);
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            let message = error.to_string();
            inner.journal.append_sync(JournalEvent::Fault {
                agent_id: Some(agent_id.clone()),
                code: "context_curation".to_owned(),
                message: message.clone(),
            })?;
            let _ = inner.events.send(RuntimeEvent::Fault {
                agent_id: agent_id.clone(),
                message,
            });
            return Err(error);
        }
    };
    let result = match result {
        CompactionOutcome::NotNeeded => return Ok(false),
        // Semantic compaction could not proceed; the compactor returned a bounded
        // mechanical trim so the agent does not stall. Drop the non-kept records
        // from context (the journal keeps them) and leave the brief as-is.
        CompactionOutcome::MechanicallyTrimmed { kept_ranges } => {
            let keep: HashSet<_> = kept_ranges
                .iter()
                .map(|range| (range.start, range.end))
                .collect();
            session.records.retain(|record| {
                keep.contains(&(record.context.range.start, record.context.range.end))
            });
            inner.journal.append_sync(JournalEvent::Fault {
                agent_id: Some(agent_id.clone()),
                code: "context_curation_fallback".to_owned(),
                message: "semantic compaction failed; mechanically trimmed context (full history preserved in the journal)".to_owned(),
            })?;
            let _ = inner.events.send(RuntimeEvent::Fault {
                agent_id: agent_id.clone(),
                message: "context curation: mechanical fallback (history kept in journal)"
                    .to_owned(),
            });
            return Ok(true);
        }
        CompactionOutcome::Compacted(result) => result,
    };
    let required_insights = unique_ids(
        result
            .coverage
            .covered_insight_ids
            .iter()
            .chain(result.coverage.superseded_insight_ids.iter())
            .cloned(),
    );
    inner.briefs.commit(
        agent_id,
        agent_id,
        BriefDraft {
            markdown: result.markdown,
            source_sha256: result.source_sha256,
            source_ranges: result.source_ranges.clone(),
            coverage: result.coverage,
        },
        &required_insights,
    )?;
    if agent_id.is_main() {
        inner.sync_main_brief()?;
    }
    let live_ranges: HashSet<_> = result
        .live_tail
        .iter()
        .map(|entry| (entry.range.start, entry.range.end))
        .collect();
    session.records.retain(|record| {
        live_ranges.contains(&(record.context.range.start, record.context.range.end))
    });
    Ok(true)
}

pub fn build_request(
    inner: &RuntimeInner,
    session: &AgentSession,
    agent_id: &AgentId,
) -> Result<crate::provider::ModelRequest, RuntimeError> {
    let brief = inner.briefs.read_effective(agent_id)?;
    let mut messages = vec![ModelMessage::new(
        ModelRole::System,
        build_system(inner, agent_id, &brief)?,
    )];
    messages.extend(session.records.iter().map(|record| record.message.clone()));
    Ok(crate::provider::ModelRequest {
        messages,
        tools: inner.tools.definitions(),
        tools_enabled: true,
        max_output_tokens: inner.config.reserved_response_tokens,
        temperature: None,
    })
}

/// Gather the live team and engagement, then delegate the full system-prompt
/// assembly to `crate::prompt` (INTENT-0004 §3.2/§3.4).
pub fn build_system(
    inner: &RuntimeInner,
    agent_id: &AgentId,
    brief: &str,
) -> Result<String, RuntimeError> {
    let agent = inner.coordinator.inspect(agent_id)?;
    let team = inner.coordinator.live_team()?;
    let engagement = inner
        .engagement
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    Ok(crate::prompt::build_system(
        &agent,
        &team,
        engagement.as_ref(),
        brief,
    ))
}
