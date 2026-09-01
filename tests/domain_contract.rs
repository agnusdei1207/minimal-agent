use minimal_agent::domain::{
    AgentDepth, AgentId, AgentMessage, AgentState, CompactionCoverage, ContextBudget, DomainError,
    Insight, InsightId, InsightLabel, MessageKind, SequenceRange, TeamLimits, estimate_tokens,
};

#[test]
fn accepts_depths_up_to_the_tree_maximum_and_rejects_deeper() {
    assert_eq!(AgentDepth::try_from(0).unwrap(), AgentDepth::MAIN);
    assert!(AgentDepth::try_from(1).is_ok());
    assert!(AgentDepth::try_from(2).is_ok());
    assert_eq!(AgentDepth::try_from(3), Err(DomainError::InvalidDepth(3)));
    assert!(AgentDepth::MAIN.is_main());
    assert!(AgentDepth::MAIN.can_spawn());
    assert!(AgentDepth::try_from(1).unwrap().can_spawn());
    assert!(!AgentDepth::try_from(2).unwrap().can_spawn());
    assert_eq!(
        AgentDepth::try_from(2),
        AgentDepth::MAIN.child().unwrap().child()
    );
}

#[test]
fn caps_the_team_at_main_plus_nine_active_workers() {
    let limits = TeamLimits::default();
    assert!(limits.validate_total(10).is_ok());
    assert_eq!(limits.validate_total(11), Err(DomainError::TeamFull));
    // A leaf (grandchild, depth 2) cannot spawn; depths below the max can.
    assert!(limits.validate_spawn(AgentDepth::MAIN).is_ok());
    assert!(limits.validate_spawn(AgentDepth::try_from(1).unwrap()).is_ok());
    assert_eq!(
        limits.validate_spawn(AgentDepth::try_from(2).unwrap()),
        Err(DomainError::MaxDepthReached)
    );
}

#[test]
fn rejects_invalid_agent_state_transitions() {
    assert!(
        AgentState::Running
            .transition_to(AgentState::Waiting)
            .is_ok()
    );
    assert!(
        AgentState::Recalling
            .transition_to(AgentState::Stopped)
            .is_ok()
    );
    assert_eq!(
        AgentState::Finished.transition_to(AgentState::Running),
        Err(DomainError::InvalidStateTransition {
            from: AgentState::Finished,
            to: AgentState::Running,
        })
    );
}

#[test]
fn messages_keep_the_given_audience_without_auto_including_main() {
    // ADR-0004: neighbor routing is enforced by the coordinator, so the domain
    // no longer auto-adds main to insight/final messages.
    let worker_a = AgentId::new("worker-a").unwrap();
    let worker_b = AgentId::new("worker-b").unwrap();
    let insight = Insight::new(
        InsightId::new("I-1").unwrap(),
        InsightLabel::Fact,
        "service listens on 8080",
    );
    let message = AgentMessage::new(
        worker_a.clone(),
        vec![worker_b.clone()],
        MessageKind::Insight,
        "port confirmed",
        Some(insight),
    )
    .unwrap();
    assert_eq!(message.audience, vec![worker_b]);

    // A final message no longer gets main appended; an empty audience is rejected.
    assert_eq!(
        AgentMessage::new(worker_a, vec![], MessageKind::Final, "done", None),
        Err(DomainError::EmptyAudience)
    );
}

#[test]
fn one_message_cannot_address_more_agents_than_the_team_can_contain() {
    let audience = (0..11)
        .map(|index| AgentId::new(format!("worker-{index}")).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        AgentMessage::new(
            AgentId::main(),
            audience,
            MessageKind::Progress,
            "bounded fanout",
            None,
        ),
        Err(DomainError::AudienceTooLarge { max: 10 })
    ));
}

#[test]
fn compaction_coverage_rejects_missing_ranges_and_insights() {
    let required_ranges = vec![SequenceRange::new(1, 5).unwrap()];
    let required_insights = vec![InsightId::new("I-1").unwrap()];
    let coverage = CompactionCoverage {
        covered_ranges: vec![SequenceRange::new(1, 4).unwrap()],
        covered_insight_ids: vec![],
        superseded_insight_ids: vec![],
    };
    assert_eq!(
        coverage.validate(&required_ranges, &required_insights),
        Err(DomainError::IncompleteCoverage)
    );
}

#[test]
fn compaction_coverage_rejects_ranges_and_insights_outside_the_source() {
    let required_ranges = vec![
        SequenceRange::new(1, 2).unwrap(),
        SequenceRange::new(5, 6).unwrap(),
    ];
    let required_insights = vec![InsightId::new("I-1").unwrap()];
    let coverage = CompactionCoverage {
        covered_ranges: vec![SequenceRange::new(1, 6).unwrap()],
        covered_insight_ids: vec![
            InsightId::new("I-1").unwrap(),
            InsightId::new("I-invented").unwrap(),
        ],
        superseded_insight_ids: vec![],
    };
    assert_eq!(
        coverage.validate(&required_ranges, &required_insights),
        Err(DomainError::IncompleteCoverage)
    );
}

#[test]
fn context_pressure_triggers_at_eighty_percent_not_seventy_nine() {
    let budget = ContextBudget::new(10_000, 12_000, 1_000).unwrap();
    assert_eq!(budget.effective_tokens(), 9_000);
    assert!(!budget.needs_compaction(7_199));
    assert!(budget.needs_compaction(7_200));
    assert_eq!(budget.target_input_tokens(), 4_500);
    assert_eq!(budget.brief_target_tokens(), 1_800);
    assert_eq!(budget.message_target_tokens(), 180);
}

#[test]
fn multilingual_text_is_not_undercounted_as_ascii_quarter_tokens() {
    assert_eq!(estimate_tokens("abcdefgh"), 2);
    assert_eq!(estimate_tokens("가나다"), 9);
    assert!(estimate_tokens("작업 진행 상황") > 4);
}
