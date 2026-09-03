use std::sync::Arc;
use std::time::Duration;

use minimal_agent::coordinator::{AgentCoordinator, CoordinatorError};
use minimal_agent::domain::{
    AgentId, AgentState, DomainError, Insight, InsightId, InsightLabel, MAX_GOAL_BYTES,
    MAX_ROLE_BYTES, MAX_TASK_BYTES, MessageKind,
};
use minimal_agent::journal::{JournalConfig, JournalError, JournalEvent, RunJournal};
use tempfile::tempdir;

fn coordinator() -> (tempfile::TempDir, Arc<RunJournal>, AgentCoordinator) {
    let dir = tempdir().unwrap();
    let journal = Arc::new(RunJournal::open(dir.path(), JournalConfig::default()).unwrap());
    let coordinator = AgentCoordinator::new(journal.clone(), "solve the target").unwrap();
    (dir, journal, coordinator)
}

#[test]
fn workers_spawn_within_depth_limit_and_the_total_is_capped() {
    let (_dir, _journal, coordinator) = coordinator();
    let main = AgentId::main();
    // A depth-1 child may spawn a grandchild (ADR-0004: 3-depth tree)...
    let child = coordinator
        .create_worker(&main, "recon", "map the service")
        .unwrap();
    let grandchild = coordinator
        .create_worker(&child, "nested", "sub-lead")
        .unwrap();
    // ...but a depth-2 grandchild is a leaf and cannot spawn.
    assert!(matches!(
        coordinator.create_worker(&grandchild, "too-deep", "must fail"),
        Err(CoordinatorError::Domain(DomainError::MaxDepthReached))
    ));
    // The total worker pool is still bounded (main + nine workers).
    for index in 1..8 {
        coordinator
            .create_worker(&main, format!("role-{index}"), format!("task-{index}"))
            .unwrap();
    }
    assert!(matches!(
        coordinator.create_worker(&main, "overflow", "must fail"),
        Err(CoordinatorError::Domain(DomainError::TeamFull))
    ));
    assert_eq!(coordinator.active_team_size(), 10);
}

#[test]
fn terminating_or_recalling_an_internal_node_cascades_to_its_whole_subtree() {
    let (_dir, _journal, coordinator) = coordinator();
    let main = AgentId::main();
    // main → child (depth 1) → grandchild (depth 2)
    let child = coordinator.create_worker(&main, "lead", "delegate").unwrap();
    let grandchild = coordinator.create_worker(&child, "leaf", "execute").unwrap();
    assert_eq!(coordinator.active_team_size(), 3);

    // Terminating the internal child tears down the whole subtree so no grandchild
    // is orphaned (ADR-0004 §7). The grandchild's permit is also freed.
    coordinator
        .mark_terminal(&main, &child, AgentState::Stopped, "branch abandoned")
        .unwrap();
    assert_eq!(coordinator.inspect(&child).unwrap().state, AgentState::Stopped);
    assert_eq!(
        coordinator.inspect(&grandchild).unwrap().state,
        AgentState::Stopped
    );
    assert_eq!(coordinator.active_team_size(), 1);
    // The freed permits are reusable — a full flat team can be created afterward.
    for index in 0..9 {
        coordinator
            .create_worker(&main, format!("r-{index}"), format!("t-{index}"))
            .unwrap();
    }
    assert_eq!(coordinator.active_team_size(), 10);
}

#[test]
fn recalling_an_internal_node_cascades_recall_to_its_subtree() {
    let (_dir, _journal, coordinator) = coordinator();
    let main = AgentId::main();
    let child = coordinator.create_worker(&main, "lead", "delegate").unwrap();
    let grandchild = coordinator.create_worker(&child, "leaf", "execute").unwrap();
    let grandchild_cancel = coordinator.cancellation_token(&grandchild).unwrap();

    coordinator.recall(&main, &child, "pivot").unwrap();
    assert_eq!(
        coordinator.inspect(&child).unwrap().state,
        AgentState::Recalling
    );
    assert_eq!(
        coordinator.inspect(&grandchild).unwrap().state,
        AgentState::Recalling
    );
    assert!(grandchild_cancel.is_cancelled());
}

#[test]
fn main_can_reassign_and_recall_a_worker_then_reuse_its_permit() {
    let (_dir, _journal, coordinator) = coordinator();
    let main = AgentId::main();
    let mut workers = Vec::new();
    for index in 0..9 {
        workers.push(
            coordinator
                .create_worker(&main, format!("role-{index}"), format!("task-{index}"))
                .unwrap(),
        );
    }
    let target = workers[0].clone();
    coordinator
        .reassign(&main, &target, "analysis", "follow the new lead")
        .unwrap();
    let snapshot = coordinator.inspect(&target).unwrap();
    assert_eq!(snapshot.role, "analysis");
    assert_eq!(snapshot.task, "follow the new lead");

    let cancellation = coordinator.cancellation_token(&target).unwrap();
    coordinator
        .recall(&main, &target, "task superseded")
        .unwrap();
    assert!(cancellation.is_cancelled());
    assert_eq!(
        coordinator.inspect(&target).unwrap().state,
        AgentState::Recalling
    );
    coordinator
        .mark_terminal(&target, &target, AgentState::Stopped, "recalled")
        .unwrap();
    assert_eq!(coordinator.active_team_size(), 9);
    assert!(
        coordinator
            .create_worker(&main, "replacement", "continue the task")
            .is_ok()
    );
}

#[test]
fn direct_messages_flow_between_main_workers_and_siblings() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let worker_a = coordinator.create_worker(&main, "a", "task a").unwrap();
    let worker_b = coordinator.create_worker(&main, "b", "task b").unwrap();

    coordinator
        .send(
            main.clone(),
            vec![worker_a.clone()],
            MessageKind::Progress,
            "start with HTTP",
            None,
        )
        .unwrap();
    coordinator
        .send(
            worker_a.clone(),
            vec![main.clone()],
            MessageKind::Request,
            "need a second opinion",
            None,
        )
        .unwrap();
    let insight = Insight::new(
        InsightId::new("I-sibling").unwrap(),
        InsightLabel::Direction,
        "inspect the alternate endpoint",
    );
    let receipt = coordinator
        .send(
            worker_a.clone(),
            vec![worker_b.clone()],
            MessageKind::Insight,
            "alternate endpoint looks promising",
            Some(insight),
        )
        .unwrap();

    assert_eq!(coordinator.inbox(&worker_a).unwrap().len(), 1);
    assert_eq!(coordinator.inbox(&worker_b).unwrap().len(), 1);
    // ADR-0004: a sibling insight is NOT auto-copied to main; it reaches only the
    // addressed sibling. Main learns of it later via the parent's synthesis.
    assert_eq!(
        coordinator
            .inbox(&main)
            .unwrap()
            .iter()
            .filter(|delivery| delivery.message.id == receipt.message_id)
            .count(),
        0
    );
    assert_eq!(
        coordinator
            .inspect(&worker_a)
            .unwrap()
            .latest_insight
            .as_deref(),
        Some("inspect the alternate endpoint")
    );
    assert_eq!(coordinator.inspect(&worker_b).unwrap().latest_insight, None);
    assert_eq!(coordinator.inspect(&main).unwrap().latest_insight, None);
    let recovered = AgentCoordinator::recover(journal).unwrap();
    assert_eq!(
        recovered
            .inspect(&worker_a)
            .unwrap()
            .latest_insight
            .as_deref(),
        Some("inspect the alternate endpoint")
    );
    assert_eq!(recovered.inspect(&worker_b).unwrap().latest_insight, None);
    assert!(
        !coordinator
            .cancellation_token(&worker_b)
            .unwrap()
            .is_cancelled()
    );
}

#[tokio::test]
async fn wait_wakes_and_messages_are_consumed_only_after_ack() {
    let (_dir, _journal, coordinator) = coordinator();
    let main = AgentId::main();
    let worker = coordinator.create_worker(&main, "worker", "wait").unwrap();
    let waiting = coordinator.clone();
    let waiting_worker = worker.clone();
    let task = tokio::spawn(async move {
        waiting
            .wait_for_messages(&waiting_worker, Duration::from_secs(2))
            .await
            .unwrap()
    });
    tokio::task::yield_now().await;
    coordinator
        .send(
            main,
            vec![worker.clone()],
            MessageKind::Progress,
            "wake up",
            None,
        )
        .unwrap();
    let deliveries = task.await.unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(coordinator.inbox(&worker).unwrap().len(), 1);
    coordinator
        .consume(&worker, &[deliveries[0].message.id])
        .unwrap();
    assert!(coordinator.inbox(&worker).unwrap().is_empty());
}

#[tokio::test]
async fn message_wait_uses_one_deadline_across_spurious_wakes() {
    let (_dir, _journal, coordinator) = coordinator();
    let worker = coordinator
        .create_worker(&AgentId::main(), "worker", "wait")
        .unwrap();
    let waiting = coordinator.clone();
    let task = tokio::spawn(async move {
        waiting
            .wait_for_messages(&worker, Duration::from_millis(50))
            .await
    });
    let nudging = coordinator.clone();
    let nudger = tokio::spawn(async move {
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            nudging.nudge_active().unwrap();
        }
    });

    let result = tokio::time::timeout(Duration::from_millis(150), task)
        .await
        .expect("spurious wakes extended the requested message deadline")
        .unwrap();
    nudger.abort();
    assert!(matches!(result, Err(CoordinatorError::WaitTimedOut)));
}

#[test]
fn replay_restores_unread_messages_without_duplicate_application() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let worker = coordinator
        .create_worker(&main, "worker", "recover")
        .unwrap();
    let receipt = coordinator
        .send(
            main,
            vec![worker.clone()],
            MessageKind::Progress,
            "persist me",
            None,
        )
        .unwrap();
    drop(coordinator);

    let recovered = AgentCoordinator::recover(journal).unwrap();
    let inbox = recovered.inbox(&worker).unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].message.id, receipt.message_id);
    recovered.consume(&worker, &[receipt.message_id]).unwrap();
    let replayed_again = AgentCoordinator::recover(recovered.journal()).unwrap();
    assert!(replayed_again.inbox(&worker).unwrap().is_empty());
}

#[test]
fn recovery_requires_main_to_be_the_first_created_agent() {
    let dir = tempdir().unwrap();
    let journal = Arc::new(RunJournal::open(dir.path(), JournalConfig::default()).unwrap());
    journal
        .append_sync(JournalEvent::AgentCreated {
            agent_id: AgentId::new("worker-01").unwrap(),
            role: "worker".to_owned(),
            task: "out-of-order task".to_owned(),
        })
        .unwrap();
    journal
        .append_sync(JournalEvent::AgentCreated {
            agent_id: AgentId::main(),
            role: "main coordinator".to_owned(),
            task: "late root".to_owned(),
        })
        .unwrap();

    assert!(matches!(
        AgentCoordinator::recover(journal),
        Err(CoordinatorError::InvalidReplay(_))
    ));
}

#[test]
fn recovery_rejects_duplicate_message_identity() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let worker = coordinator
        .create_worker(&main, "worker", "recover")
        .unwrap();
    drop(coordinator);
    let message = minimal_agent::domain::AgentMessage::new(
        main,
        vec![worker],
        MessageKind::Progress,
        "one durable identity",
        None,
    )
    .unwrap();
    for _ in 0..2 {
        journal
            .append_sync(JournalEvent::AgentMessage {
                message: message.clone(),
            })
            .unwrap();
    }

    assert!(matches!(
        AgentCoordinator::recover(journal),
        Err(CoordinatorError::InvalidReplay(_))
    ));
}

#[test]
fn failed_journal_flush_neither_delivers_nor_wakes() {
    let dir = tempdir().unwrap();
    let config = JournalConfig {
        segment_bytes: 4 * 1024,
        inline_payload_bytes: 4 * 1024,
        max_run_bytes: 1_400,
        reserved_fault_bytes: 512,
    };
    let journal = Arc::new(RunJournal::open(dir.path(), config).unwrap());
    let coordinator = AgentCoordinator::new(journal, "goal").unwrap();
    let main = AgentId::main();
    let worker = coordinator.create_worker(&main, "worker", "task").unwrap();
    let error = coordinator
        .send(
            main,
            vec![worker.clone()],
            MessageKind::Progress,
            "x".repeat(4_000),
            None,
        )
        .unwrap_err();
    assert!(matches!(
        error,
        CoordinatorError::Journal(JournalError::StorageFull)
    ));
    assert!(coordinator.inbox(&worker).unwrap().is_empty());
    assert!(
        !coordinator
            .cancellation_token(&worker)
            .unwrap()
            .is_cancelled()
    );
}

#[test]
fn inbox_backpressure_is_atomic_before_the_journal_append() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let full = coordinator.create_worker(&main, "full", "wait").unwrap();
    let empty = coordinator.create_worker(&main, "empty", "wait").unwrap();
    for index in 0..64 {
        coordinator
            .send(
                main.clone(),
                vec![full.clone()],
                MessageKind::Progress,
                format!("message-{index}"),
                None,
            )
            .unwrap();
    }
    let before = journal.replay().unwrap().len();

    assert!(
        coordinator
            .send(
                main,
                vec![full.clone(), empty.clone()],
                MessageKind::Progress,
                "must not partially deliver",
                None,
            )
            .is_err()
    );
    assert_eq!(journal.replay().unwrap().len(), before);
    assert_eq!(coordinator.inbox(&full).unwrap().len(), 64);
    assert!(coordinator.inbox(&empty).unwrap().is_empty());
}

#[test]
fn oversized_messages_are_rejected_before_the_journal_append() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let worker = coordinator.create_worker(&main, "worker", "wait").unwrap();
    let before = journal.replay().unwrap().len();

    assert!(
        coordinator
            .send(
                main,
                vec![worker.clone()],
                MessageKind::Progress,
                "x".repeat(4_097),
                None,
            )
            .is_err()
    );
    assert_eq!(journal.replay().unwrap().len(), before);
    assert!(coordinator.inbox(&worker).unwrap().is_empty());
}

#[test]
fn messages_to_terminal_agents_are_rejected_without_durable_orphaning() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let worker = coordinator
        .create_worker(&main, "worker", "finish")
        .unwrap();
    coordinator
        .mark_terminal(&worker, &worker, AgentState::Finished, "done")
        .unwrap();
    let before = journal.replay().unwrap().len();

    assert!(
        coordinator
            .send(
                main,
                vec![worker.clone()],
                MessageKind::Progress,
                "no consumer remains",
                None,
            )
            .is_err()
    );
    assert_eq!(journal.replay().unwrap().len(), before);
    assert!(coordinator.inbox(&worker).unwrap().is_empty());
}

#[test]
fn recovery_fails_closed_when_historical_unread_messages_exceed_the_bound() {
    let dir = tempdir().unwrap();
    let journal = Arc::new(RunJournal::open(dir.path(), JournalConfig::default()).unwrap());
    let coordinator = AgentCoordinator::new(journal.clone(), "goal").unwrap();
    let main = AgentId::main();
    let worker = coordinator.create_worker(&main, "worker", "wait").unwrap();
    drop(coordinator);
    for index in 0..65 {
        journal
            .append_sync(JournalEvent::AgentMessage {
                message: minimal_agent::domain::AgentMessage::new(
                    main.clone(),
                    vec![worker.clone()],
                    MessageKind::Progress,
                    format!("historical-{index}"),
                    None,
                )
                .unwrap(),
            })
            .unwrap();
    }

    assert!(AgentCoordinator::recover(journal).is_err());
}

#[test]
fn recovery_restores_why_a_nonterminal_agent_is_waiting() {
    let (_dir, journal, coordinator) = coordinator();
    let worker = coordinator
        .create_worker(&AgentId::main(), "worker", "retry later")
        .unwrap();
    coordinator
        .mark_waiting(&worker, "provider unavailable")
        .unwrap();
    drop(coordinator);

    let recovered = AgentCoordinator::recover(journal).unwrap();
    let snapshot = recovered.inspect(&worker).unwrap();
    assert_eq!(snapshot.state, AgentState::Waiting);
    assert_eq!(snapshot.waiting_on.as_deref(), Some("provider unavailable"));
}

#[test]
fn changing_a_wait_reason_is_durable_even_without_a_state_change() {
    let (_dir, journal, coordinator) = coordinator();
    let worker = coordinator
        .create_worker(&AgentId::main(), "worker", "retry later")
        .unwrap();
    coordinator.mark_waiting(&worker, "rate limited").unwrap();
    coordinator
        .mark_waiting(&worker, "payment required")
        .unwrap();
    drop(coordinator);

    let recovered = AgentCoordinator::recover(journal).unwrap();
    assert_eq!(
        recovered.inspect(&worker).unwrap().waiting_on.as_deref(),
        Some("payment required")
    );
}

#[test]
fn live_team_projection_contains_only_the_current_team() {
    let (_dir, _journal, coordinator) = coordinator();
    let main = AgentId::main();
    let finished = coordinator
        .create_worker(&main, "finished", "old task")
        .unwrap();
    coordinator
        .mark_terminal(&finished, &finished, AgentState::Finished, "done")
        .unwrap();
    let active = coordinator
        .create_worker(&main, "active", "current task")
        .unwrap();

    let live = coordinator.live_team().unwrap();
    assert_eq!(live.len(), 2);
    assert!(live.iter().any(|agent| agent.id == main));
    assert!(live.iter().any(|agent| agent.id == active));
    assert!(!live.iter().any(|agent| agent.id == finished));

    // Durable inspection still retains terminal history even though the hot
    // team projection stays bounded to currently addressable agents.
    assert_eq!(
        coordinator.inspect(&finished).unwrap().state,
        AgentState::Finished
    );
}

#[test]
fn assignment_text_bounds_reject_before_journal_or_state_mutation() {
    let dir = tempdir().unwrap();
    let journal = Arc::new(RunJournal::open(dir.path(), JournalConfig::default()).unwrap());
    assert!(AgentCoordinator::new(journal.clone(), "g".repeat(MAX_GOAL_BYTES + 1)).is_err());
    assert!(journal.replay().unwrap().is_empty());
    drop(journal);

    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let before = journal.replay().unwrap().len();
    assert!(
        coordinator
            .create_worker(&main, "r".repeat(MAX_ROLE_BYTES + 1), "task")
            .is_err()
    );
    assert!(
        coordinator
            .create_worker(&main, "role", "t".repeat(MAX_TASK_BYTES + 1))
            .is_err()
    );
    assert_eq!(journal.replay().unwrap().len(), before);
    assert_eq!(coordinator.active_team_size(), 1);

    let worker = coordinator.create_worker(&main, "role", "task").unwrap();
    let before = journal.replay().unwrap().len();
    assert!(
        coordinator
            .reassign(&main, &worker, "role", "t".repeat(MAX_TASK_BYTES + 1))
            .is_err()
    );
    assert!(
        coordinator
            .set_goal(&main, "g".repeat(MAX_GOAL_BYTES + 1))
            .is_err()
    );
    assert_eq!(journal.replay().unwrap().len(), before);
    assert_eq!(coordinator.inspect(&worker).unwrap().task, "task");
    assert_eq!(coordinator.inspect(&main).unwrap().task, "solve the target");
}

#[test]
fn creating_a_new_coordinator_never_appends_a_second_main_to_an_existing_run() {
    let (_dir, journal, first) = coordinator();
    let before = journal.replay().unwrap().len();

    assert!(matches!(
        AgentCoordinator::new(journal.clone(), "another goal"),
        Err(minimal_agent::coordinator::CoordinatorError::RunNotEmpty)
    ));
    assert_eq!(journal.replay().unwrap().len(), before);
    assert_eq!(
        first.inspect(&AgentId::main()).unwrap().task,
        "solve the target"
    );
}

#[test]
fn main_is_the_stable_team_root_and_cannot_be_marked_terminal() {
    let (_dir, journal, coordinator) = coordinator();
    let main = AgentId::main();
    let before = journal.replay().unwrap().len();
    assert!(matches!(
        coordinator.mark_terminal(&main, &main, AgentState::Finished, "do not remove root"),
        Err(minimal_agent::coordinator::CoordinatorError::CannotTerminateMain)
    ));
    assert_eq!(journal.replay().unwrap().len(), before);
    assert_eq!(
        coordinator.inspect(&main).unwrap().state,
        AgentState::Running
    );
}
