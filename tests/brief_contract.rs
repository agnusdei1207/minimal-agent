use std::fs;
use std::sync::Arc;

use minimal_agent::brief::{AgentBriefStore, BriefDraft, BriefError};
use minimal_agent::coordinator::AgentCoordinator;
use minimal_agent::domain::{AgentId, CompactionCoverage, ContextBudget, InsightId, SequenceRange};
use minimal_agent::journal::{JournalConfig, RunJournal};
use tempfile::tempdir;

fn valid_worker_markdown() -> String {
    "# Worker Agent Brief\n\
## Assignment\nInspect the service\n\
## Current State\nRunning\n\
## Attempts by Domain\n- HTTP routing family tested\n\
## Curated Knowledge\n\
### Facts & Successes\n- [FACT][I-1] endpoint /health returned 200 (journal:4)\n\
### Hypotheses & Directions\n- [HYPOTHESIS][I-2] alternate host may expose admin\n\
### Dead Ends\n- [DEAD_END][I-3] query payload family blocked; reason: normalized before render\n\
## Integrated Messages\n- sibling suggested host-header comparison\n\
## Blockers\nNone\n\
## Next Move\nCompare virtual hosts\n"
        .into()
}

fn main_markdown(runtime_blocks: usize) -> String {
    let block = "<!-- minimal-agent:runtime:start -->\n\
## Team\n| ID | Role | Current Task | State | Latest Insight | Waiting On |\n\
|---|---|---|---|---|---|\n\
| main | main coordinator | goal | RUNNING | | |\n\
<!-- minimal-agent:runtime:end -->\n";
    format!(
        "# Main Agent Brief\n{}\
## Goal & Constraints\ngoal\n\
## Battlefield\nactive\n\
## Curated Knowledge\n\
### Facts & Successes\nNo durable insight yet\n\
### Hypotheses & Directions\nNo durable insight yet\n\
### Dead Ends\nNo durable insight yet\n\
## Blockers\nNone\n\
## Next Moves\nContinue\n",
        block.repeat(runtime_blocks)
    )
}

fn main_draft(markdown: String) -> BriefDraft {
    let range = SequenceRange::new(1, 1).unwrap();
    BriefDraft {
        markdown,
        source_sha256: "b".repeat(64),
        source_ranges: vec![range],
        coverage: CompactionCoverage {
            covered_ranges: vec![range],
            covered_insight_ids: vec![],
            superseded_insight_ids: vec![],
        },
    }
}

fn draft(markdown: String) -> BriefDraft {
    BriefDraft {
        markdown,
        source_sha256: "a".repeat(64),
        source_ranges: vec![SequenceRange::new(1, 8).unwrap()],
        coverage: CompactionCoverage {
            covered_ranges: vec![SequenceRange::new(1, 8).unwrap()],
            covered_insight_ids: vec![
                InsightId::new("I-1").unwrap(),
                InsightId::new("I-2").unwrap(),
                InsightId::new("I-3").unwrap(),
            ],
            superseded_insight_ids: vec![],
        },
    }
}

fn setup() -> (
    tempfile::TempDir,
    Arc<RunJournal>,
    AgentCoordinator,
    AgentBriefStore,
    AgentId,
) {
    let dir = tempdir().unwrap();
    let journal = Arc::new(RunJournal::open(dir.path(), JournalConfig::default()).unwrap());
    let coordinator = AgentCoordinator::new(journal.clone(), "goal").unwrap();
    let worker = coordinator
        .create_worker(&AgentId::main(), "recon", "inspect service")
        .unwrap();
    let store = AgentBriefStore::new(
        dir.path(),
        journal.clone(),
        ContextBudget::new(20_000, 20_000, 2_000).unwrap(),
    );
    (dir, journal, coordinator, store, worker)
}

#[test]
fn each_agent_has_exactly_one_brief_at_its_own_path() {
    let (dir, _journal, coordinator, store, worker) = setup();
    store
        .initialize(&coordinator.inspect(&AgentId::main()).unwrap())
        .unwrap();
    store
        .initialize(&coordinator.inspect(&worker).unwrap())
        .unwrap();
    assert!(dir.path().join("agents/main/brief.md").is_file());
    assert!(
        dir.path()
            .join(format!("agents/{worker}/brief.md"))
            .is_file()
    );
    let markdown_count = fs::read_dir(dir.path().join(format!("agents/{worker}")))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
        .count();
    assert_eq!(markdown_count, 1);
}

#[test]
fn only_the_agent_itself_can_commit_semantic_content() {
    let (_dir, _journal, coordinator, store, worker) = setup();
    store
        .initialize(&coordinator.inspect(&worker).unwrap())
        .unwrap();
    let error = store
        .commit(
            &AgentId::main(),
            &worker,
            draft(valid_worker_markdown()),
            &[
                InsightId::new("I-1").unwrap(),
                InsightId::new("I-2").unwrap(),
                InsightId::new("I-3").unwrap(),
            ],
        )
        .unwrap_err();
    assert!(matches!(error, BriefError::Ownership { .. }));
}

#[test]
fn runtime_team_projection_is_immediate_and_preserves_main_semantics() {
    let (_dir, _journal, coordinator, store, _worker) = setup();
    let main = coordinator.inspect(&AgentId::main()).unwrap();
    store.initialize(&main).unwrap();
    let before = store.read(&AgentId::main()).unwrap();
    assert!(before.contains("No durable insight yet"));
    store
        .sync_main_runtime(&AgentId::main(), &coordinator.team().unwrap())
        .unwrap();
    let after = store.read(&AgentId::main()).unwrap();
    assert!(after.contains("| worker-01 | recon | inspect service | RUNNING |"));
    assert!(after.contains("No durable insight yet"));
}

#[test]
fn worker_cannot_write_the_main_runtime_projection() {
    let (_dir, _journal, coordinator, store, worker) = setup();
    store
        .initialize(&coordinator.inspect(&AgentId::main()).unwrap())
        .unwrap();
    let original = store.read(&AgentId::main()).unwrap();

    assert!(matches!(
        store.sync_main_runtime(&worker, &coordinator.team().unwrap()),
        Err(BriefError::Ownership { .. })
    ));
    assert_eq!(store.read(&AgentId::main()).unwrap(), original);
}

#[test]
fn main_brief_requires_exactly_one_replaceable_runtime_projection() {
    let (_dir, _journal, coordinator, store, _worker) = setup();
    let main = AgentId::main();
    store
        .initialize(&coordinator.inspect(&main).unwrap())
        .unwrap();
    let original = store.read(&main).unwrap();

    assert!(
        store
            .commit(&main, &main, main_draft(main_markdown(0)), &[])
            .is_err()
    );
    assert!(
        store
            .commit(&main, &main, main_draft(main_markdown(2)), &[])
            .is_err()
    );
    assert_eq!(store.read(&main).unwrap(), original);
}

#[test]
fn tampered_projection_is_bounded_and_revalidated_on_read() {
    let (dir, _journal, coordinator, store, _worker) = setup();
    let main = AgentId::main();
    store
        .initialize(&coordinator.inspect(&main).unwrap())
        .unwrap();
    let path = dir.path().join("agents/main/brief.md");

    std::fs::write(&path, "x".repeat(60_000)).unwrap();
    assert!(matches!(
        store.read(&main),
        Err(BriefError::TooManyBytes { .. })
    ));

    std::fs::write(&path, "small but structurally invalid").unwrap();
    assert!(matches!(
        store.read(&main),
        Err(BriefError::InvalidStructure)
    ));
}

#[test]
fn the_same_checkpoint_renders_byte_identically() {
    let (_dir, _journal, coordinator, store, worker) = setup();
    store
        .initialize(&coordinator.inspect(&worker).unwrap())
        .unwrap();
    let required = [
        InsightId::new("I-1").unwrap(),
        InsightId::new("I-2").unwrap(),
        InsightId::new("I-3").unwrap(),
    ];
    store
        .commit(&worker, &worker, draft(valid_worker_markdown()), &required)
        .unwrap();
    let first = store.read(&worker).unwrap();
    store
        .commit(&worker, &worker, draft(valid_worker_markdown()), &required)
        .unwrap();
    assert_eq!(store.read(&worker).unwrap().as_bytes(), first.as_bytes());
}

#[test]
fn invalid_semantics_coverage_and_size_never_replace_the_valid_brief() {
    let (_dir, _journal, coordinator, store, worker) = setup();
    store
        .initialize(&coordinator.inspect(&worker).unwrap())
        .unwrap();
    let original = store.read(&worker).unwrap();

    let mut wrong_label = valid_worker_markdown();
    wrong_label = wrong_label.replace(
        "### Hypotheses & Directions\n- [HYPOTHESIS][I-2]",
        "### Hypotheses & Directions\n- [FACT][I-2]",
    );
    assert!(matches!(
        store.commit(
            &worker,
            &worker,
            draft(wrong_label),
            &[InsightId::new("I-2").unwrap()]
        ),
        Err(BriefError::InvalidKnowledge(_))
    ));

    let mut missing = draft(valid_worker_markdown());
    missing.coverage.covered_insight_ids.pop();
    assert!(matches!(
        store.commit(
            &worker,
            &worker,
            missing,
            &[
                InsightId::new("I-1").unwrap(),
                InsightId::new("I-2").unwrap(),
                InsightId::new("I-3").unwrap(),
            ]
        ),
        Err(BriefError::Coverage(_))
    ));

    let too_large = valid_worker_markdown() + &" token".repeat(4_000);
    assert!(matches!(
        store.commit(&worker, &worker, draft(too_large), &[]),
        Err(BriefError::TooLarge { .. })
    ));
    assert_eq!(store.read(&worker).unwrap(), original);
}

#[test]
fn failed_checkpoint_append_preserves_the_previous_projection() {
    let dir = tempdir().unwrap();
    let config = JournalConfig {
        segment_bytes: 8 * 1024,
        inline_payload_bytes: 8 * 1024,
        max_run_bytes: 1_300,
        reserved_fault_bytes: 512,
    };
    let journal = Arc::new(RunJournal::open(dir.path(), config).unwrap());
    let coordinator = AgentCoordinator::new(journal.clone(), "goal").unwrap();
    let worker = coordinator
        .create_worker(&AgentId::main(), "recon", "task")
        .unwrap();
    let store = AgentBriefStore::new(
        dir.path(),
        journal,
        ContextBudget::new(20_000, 20_000, 2_000).unwrap(),
    );
    store
        .initialize(&coordinator.inspect(&worker).unwrap())
        .unwrap();
    let original = store.read(&worker).unwrap();
    assert!(matches!(
        store.commit(
            &worker,
            &worker,
            draft(valid_worker_markdown() + &" detail".repeat(500)),
            &[
                InsightId::new("I-1").unwrap(),
                InsightId::new("I-2").unwrap(),
                InsightId::new("I-3").unwrap(),
            ]
        ),
        Err(BriefError::Journal(_))
    ));
    assert_eq!(store.read(&worker).unwrap(), original);
}

#[test]
fn latest_checkpoint_restores_the_brief_projection() {
    let (dir, journal, coordinator, store, worker) = setup();
    store
        .initialize(&coordinator.inspect(&worker).unwrap())
        .unwrap();
    let required = [
        InsightId::new("I-1").unwrap(),
        InsightId::new("I-2").unwrap(),
        InsightId::new("I-3").unwrap(),
    ];
    store
        .commit(&worker, &worker, draft(valid_worker_markdown()), &required)
        .unwrap();
    fs::remove_file(dir.path().join(format!("agents/{worker}/brief.md"))).unwrap();
    let restored = AgentBriefStore::new(
        dir.path(),
        journal,
        ContextBudget::new(20_000, 20_000, 2_000).unwrap(),
    );
    restored.recover().unwrap();
    assert_eq!(restored.read(&worker).unwrap(), valid_worker_markdown());
}
