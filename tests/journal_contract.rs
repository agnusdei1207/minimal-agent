use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Arc;

use minimal_agent::domain::{AgentId, AgentState};
use minimal_agent::journal::{
    EventStorage, JournalConfig, JournalError, JournalEvent, JournalEventKind, RunJournal,
    TranscriptRole,
};
use sha2::{Digest, Sha256};
use tempfile::tempdir;

fn test_config() -> JournalConfig {
    JournalConfig {
        segment_bytes: 512,
        inline_payload_bytes: 96,
        max_run_bytes: 32 * 1024,
        reserved_fault_bytes: 512,
    }
}

#[test]
fn latest_sequence_tracks_only_committed_appends_and_survives_reopen() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    assert_eq!(journal.latest_sequence().unwrap(), 0);
    let ack = journal
        .append_sync(JournalEvent::ToolResult {
            agent_id: AgentId::main(),
            call_id: "fixture".to_owned(),
            content: "evidence".to_owned(),
            success: true,
        })
        .unwrap();
    assert_eq!(journal.latest_sequence().unwrap(), ack.sequence);
    drop(journal);
    let recovered = RunJournal::open(dir.path(), test_config()).unwrap();
    assert_eq!(recovered.latest_sequence().unwrap(), ack.sequence);
    if let EventStorage::Blob { sha256, .. } = ack.storage {
        fs::write(dir.path().join("blobs").join(sha256), "unreadable payload").unwrap();
        assert_eq!(recovered.latest_sequence().unwrap(), ack.sequence);
    }
}

#[test]
fn kind_visitor_skips_unrelated_blobs_and_bounds_each_selected_event_without_accumulating() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    let old = journal
        .append_sync(JournalEvent::ToolResult {
            agent_id: AgentId::main(),
            call_id: "old".to_owned(),
            content: "old evidence".to_owned(),
            success: true,
        })
        .unwrap();
    journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: TranscriptRole::Assistant,
            content: "x".repeat(2048),
            complete: true,
            atomic_group: None,
        })
        .unwrap();
    for index in 0..20 {
        journal
            .append_sync(JournalEvent::ToolResult {
                agent_id: AgentId::main(),
                call_id: format!("new-{index}"),
                content: "x".repeat(128),
                success: true,
            })
            .unwrap();
    }
    let mut ids = Vec::new();
    journal
        .visit_kind_after(old.sequence, JournalEventKind::ToolResult, 1024, |entry| {
            if let JournalEvent::ToolResult { call_id, .. } = entry.event {
                ids.push(call_id);
            }
        })
        .unwrap();
    assert_eq!(ids.len(), 20);
    assert_eq!(ids.first().unwrap(), "new-0");
    assert_eq!(ids.last().unwrap(), "new-19");
}

#[test]
fn kind_visitor_refuses_oversized_selected_payloads_and_checks_selected_blob_integrity() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    let ack = journal
        .append_sync(JournalEvent::ToolResult {
            agent_id: AgentId::main(),
            call_id: "large".to_owned(),
            content: "x".repeat(2048),
            success: true,
        })
        .unwrap();
    let mut visited = false;
    assert!(matches!(
        journal.visit_kind_after(0, JournalEventKind::ToolResult, 1024, |_| visited = true),
        Err(JournalError::ReplayLimit { .. })
    ));
    assert!(!visited);
    let EventStorage::Blob { sha256, .. } = ack.storage else {
        panic!("fixture must use blob storage")
    };
    fs::write(dir.path().join("blobs").join(sha256), "corrupt").unwrap();
    assert!(matches!(
        journal.visit_kind_after(0, JournalEventKind::ToolResult, 4096, |_| visited = true),
        Err(JournalError::BlobDigestMismatch { .. })
    ));
    assert!(!visited);
}

#[test]
fn kind_visitor_validates_record_checksums_even_for_unrelated_event_kinds() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: TranscriptRole::Assistant,
            content: "x".repeat(2048),
            complete: true,
            atomic_group: None,
        })
        .unwrap();
    let segment = fs::read_dir(dir.path().join("journal"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let original = fs::read_to_string(&segment).unwrap();
    let altered = original.replace("\"kind\":\"transcript\"", "\"kind\":\"fault\"");
    assert_ne!(original, altered);
    fs::write(segment, altered).unwrap();
    assert!(matches!(
        journal.visit_kind_after(0, JournalEventKind::ToolResult, 1024, |_| panic!(
            "unrelated event must not be visited"
        )),
        Err(JournalError::ChecksumMismatch { .. })
    ));
}

#[tokio::test]
async fn concurrent_appends_receive_one_monotonic_sequence() {
    let dir = tempdir().unwrap();
    let journal = Arc::new(RunJournal::open(dir.path(), test_config()).unwrap());
    let mut tasks = Vec::new();
    for index in 0..32 {
        let journal = journal.clone();
        tasks.push(tokio::spawn(async move {
            journal
                .append(JournalEvent::Transcript {
                    agent_id: AgentId::main(),
                    role: TranscriptRole::User,
                    content: format!("message-{index}"),
                    complete: true,
                    atomic_group: None,
                })
                .await
                .unwrap()
                .sequence
        }));
    }
    let mut sequences = Vec::new();
    for task in tasks {
        sequences.push(task.await.unwrap());
    }
    sequences.sort_unstable();
    assert_eq!(sequences, (1..=32).collect::<Vec<_>>());
    assert_eq!(journal.replay().unwrap().len(), 32);
}

#[test]
fn refuses_a_second_writer_for_the_same_run() {
    let dir = tempdir().unwrap();
    let first = RunJournal::open(dir.path(), test_config()).unwrap();
    let error = RunJournal::open(dir.path(), test_config()).unwrap_err();
    assert!(matches!(error, JournalError::WriterActive));
    drop(first);
    assert!(RunJournal::open(dir.path(), test_config()).is_ok());
}

#[tokio::test]
async fn rollover_preserves_logical_replay_order() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    for index in 0..12 {
        journal
            .append(JournalEvent::AgentStateChanged {
                agent_id: AgentId::main(),
                from: AgentState::Running,
                to: AgentState::Waiting,
                reason: format!("cycle-{index}"),
            })
            .await
            .unwrap();
    }
    let segments = fs::read_dir(dir.path().join("journal"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .count();
    assert!(segments > 1);
    let replay = journal.replay().unwrap();
    assert_eq!(replay.first().unwrap().sequence, 1);
    assert_eq!(replay.last().unwrap().sequence, 12);
}

#[tokio::test]
async fn oversized_payload_round_trips_through_a_verified_blob() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    let content = "0123456789abcdef".repeat(64);
    let ack = journal
        .append(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: TranscriptRole::Tool,
            content: content.clone(),
            complete: true,
            atomic_group: Some("call-1".into()),
        })
        .await
        .unwrap();
    let digest = match ack.storage {
        EventStorage::Blob { sha256, bytes } => {
            assert!(bytes > test_config().inline_payload_bytes as u64);
            sha256
        }
        EventStorage::Inline => panic!("large event was stored inline"),
    };
    let replay = journal.replay().unwrap();
    assert!(matches!(
        &replay[0].event,
        JournalEvent::Transcript { content: replayed, .. } if replayed == &content
    ));

    fs::write(dir.path().join("blobs").join(digest), b"tampered").unwrap();
    assert!(matches!(
        journal.replay().unwrap_err(),
        JournalError::BlobDigestMismatch { .. }
    ));
}

#[test]
fn an_existing_blob_is_verified_before_a_new_record_can_reference_it() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    let event = JournalEvent::Transcript {
        agent_id: AgentId::main(),
        role: TranscriptRole::User,
        content: "x".repeat(1_024),
        complete: true,
        atomic_group: None,
    };
    let payload = serde_json::to_vec(&event).unwrap();
    let digest = hex::encode(Sha256::digest(&payload));
    fs::write(dir.path().join("blobs").join(digest), b"corrupt").unwrap();

    assert!(matches!(
        journal.append_sync(event),
        Err(JournalError::BlobLengthMismatch { .. })
    ));
    assert!(journal.replay().unwrap().is_empty());
}

#[tokio::test]
async fn repairs_only_a_partial_final_line() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    journal
        .append(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: TranscriptRole::Assistant,
            content: "complete".into(),
            complete: true,
            atomic_group: None,
        })
        .await
        .unwrap();
    drop(journal);

    let segment = dir.path().join("journal/0000000000000001.jsonl");
    OpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap()
        .write_all(b"{\"partial\":")
        .unwrap();

    let recovered = RunJournal::open(dir.path(), test_config()).unwrap();
    assert_eq!(recovered.replay().unwrap().len(), 1);
    drop(recovered);

    let mut bytes = fs::read(&segment).unwrap();
    let split = bytes.iter().position(|byte| *byte == b'\n').unwrap() + 1;
    bytes.splice(split..split, b"{broken}\n".iter().copied());
    fs::write(&segment, bytes).unwrap();
    assert!(matches!(
        RunJournal::open(dir.path(), test_config()).unwrap_err(),
        JournalError::MiddleCorruption { .. }
    ));
}

#[tokio::test]
async fn storage_cap_records_a_fault_without_deleting_prior_events() {
    let dir = tempdir().unwrap();
    let config = JournalConfig {
        segment_bytes: 4 * 1024,
        inline_payload_bytes: 4 * 1024,
        max_run_bytes: 1_200,
        reserved_fault_bytes: 512,
    };
    let journal = RunJournal::open(dir.path(), config).unwrap();
    journal
        .append(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: TranscriptRole::User,
            content: "kept".into(),
            complete: true,
            atomic_group: None,
        })
        .await
        .unwrap();
    assert!(matches!(
        journal
            .append(JournalEvent::Transcript {
                agent_id: AgentId::main(),
                role: TranscriptRole::Tool,
                content: "x".repeat(2_000),
                complete: true,
                atomic_group: None,
            })
            .await
            .unwrap_err(),
        JournalError::StorageFull
    ));
    let replay = journal.replay().unwrap();
    assert!(matches!(replay[0].event, JournalEvent::Transcript { .. }));
    assert!(matches!(
        replay.last().unwrap().event,
        JournalEvent::Fault { ref code, .. } if code == "storage_cap"
    ));
    let before = replay.len();
    drop(journal);

    let reopened = RunJournal::open(dir.path(), config).unwrap();
    assert!(matches!(
        reopened.append_sync(JournalEvent::Final {
            agent_id: AgentId::main(),
            body: "must not reopen writes".into(),
        }),
        Err(JournalError::StorageFull)
    ));
    assert_eq!(reopened.replay().unwrap().len(), before);
}

#[test]
fn opening_an_existing_run_rejects_storage_above_the_configured_cap_before_replay() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("journal")).unwrap();
    fs::create_dir_all(dir.path().join("blobs")).unwrap();
    fs::write(
        dir.path().join("journal/0000000000000001.jsonl"),
        vec![b'x'; 2_048],
    )
    .unwrap();
    let config = JournalConfig {
        segment_bytes: 512,
        inline_payload_bytes: 128,
        max_run_bytes: 1_024,
        reserved_fault_bytes: 128,
    };
    assert!(matches!(
        RunJournal::open(dir.path(), config),
        Err(JournalError::ExistingStorageTooLarge {
            actual: 2_048,
            max: 1_024
        })
    ));
}

#[test]
fn bounded_range_replay_returns_only_the_requested_window() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    for index in 1..=12 {
        journal
            .append_sync(JournalEvent::Transcript {
                agent_id: AgentId::main(),
                role: TranscriptRole::User,
                content: format!("event-{index}"),
                complete: true,
                atomic_group: None,
            })
            .unwrap();
    }

    let replay = journal.replay_range(5, 7, 4 * 1_024).unwrap();
    assert_eq!(
        replay
            .iter()
            .map(|entry| entry.sequence)
            .collect::<Vec<_>>(),
        vec![5, 6, 7]
    );
}

#[test]
fn bounded_range_replay_rejects_selected_payloads_before_loading_blobs() {
    let dir = tempdir().unwrap();
    let journal = RunJournal::open(dir.path(), test_config()).unwrap();
    journal
        .append_sync(JournalEvent::Transcript {
            agent_id: AgentId::main(),
            role: TranscriptRole::Tool,
            content: "x".repeat(1_024),
            complete: true,
            atomic_group: None,
        })
        .unwrap();

    assert!(matches!(
        journal.replay_range(1, 1, 128).unwrap_err(),
        JournalError::ReplayLimit { max_bytes: 128 }
    ));
}
