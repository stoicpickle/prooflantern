use std::path::PathBuf;

use proof_lantern::{
    CapabilityIntent, CapabilityRole, Claim, CurrentFocus, DisplayState, EvaluatedProject,
    EvidenceFact, EvidenceLocation, FocusKind, Freshness, MachineObservation, MachineSource,
    ModelWarning, ObservationSet, evaluate, load_project,
};

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box")
}

fn assert_focus(
    project: &EvaluatedProject,
    capability_id: &str,
    kind: FocusKind,
    heading: &str,
    action_heading: &str,
    action_marker: &str,
) {
    let CurrentFocus::Capability {
        capability,
        kind: actual_kind,
        summary,
        action,
        ..
    } = project.current_focus()
    else {
        panic!("expected a capability focus");
    };
    assert_eq!(capability.intent.id, capability_id);
    assert_eq!(actual_kind, kind);
    assert_eq!(actual_kind.heading(), heading);
    assert_eq!(action.heading, action_heading);
    assert!(action.instruction.contains(action_marker));
    assert!(action.instruction.contains(&capability.intent.proof_needed));

    if kind != FocusKind::JourneyBreak {
        let summary = summary.to_lowercase();
        for unsupported in ["journey break", "stops here", "blocks the promise"] {
            assert!(
                !summary.contains(unsupported),
                "{kind:?} must not make a missing-specific claim: {summary}"
            );
        }
    }
}

#[test]
fn recipe_box_derives_truthful_states_and_the_current_focus() {
    let (spec, observations) = load_project(fixture_root()).expect("fixture should load");
    let evaluated = evaluate(spec, observations).expect("fixture should evaluate");

    assert_eq!(
        evaluated.capability("add").unwrap().display,
        DisplayState::Proven
    );
    assert_eq!(
        evaluated.capability("save").unwrap().display,
        DisplayState::BuiltUnproven
    );
    assert_eq!(
        evaluated.capability("reopen").unwrap().display,
        DisplayState::Missing
    );
    assert_eq!(
        evaluated.capability("find").unwrap().display,
        DisplayState::Unknown
    );
    assert_eq!(
        evaluated.capability("local_database").unwrap().display,
        DisplayState::Proven
    );

    let CurrentFocus::Capability {
        capability,
        pinned,
        kind,
        summary,
        action,
    } = evaluated.current_focus()
    else {
        panic!("Recipe Box should have a current capability focus");
    };
    assert_eq!(capability.intent.id, "reopen");
    assert_eq!(kind, FocusKind::JourneyBreak);
    assert_eq!(kind.heading(), "JOURNEY BREAK");
    assert!(!pinned);
    assert_eq!(
        summary,
        "Required implementation is recorded absent. Downstream unresolved: Find a saved recipe."
    );
    assert_eq!(action.heading, "PROOF NEEDED");
    assert_eq!(
        action.instruction,
        "Close app, reopen it, and confirm the saved recipe appears."
    );
}

#[test]
fn current_focus_language_tracks_the_selected_evidence_state() {
    let (base_spec, base_observations) = load_project(fixture_root()).unwrap();

    let mut built_spec = base_spec.clone();
    built_spec.project.pinned_keystone = Some("save".into());
    let built = evaluate(built_spec, base_observations.clone()).unwrap();
    assert_focus(
        &built,
        "save",
        FocusKind::NeedsProof,
        "NEEDS PROOF",
        "PROOF NEEDED",
        "Save a recipe",
    );

    let mut unknown_spec = base_spec.clone();
    unknown_spec.project.pinned_keystone = Some("find".into());
    let unknown = evaluate(unknown_spec, base_observations.clone()).unwrap();
    assert_focus(
        &unknown,
        "find",
        FocusKind::NeedsEvidence,
        "NEEDS EVIDENCE",
        "NEXT CHECK",
        "Inspect the implementation or record evidence",
    );

    let mut failed_spec = base_spec.clone();
    failed_spec.project.pinned_keystone = Some("add".into());
    let mut failed_observations = base_observations.clone();
    for observation in &mut failed_observations.observations {
        if observation.capability_id == "add" && observation.fact.claim == Claim::VerificationPassed
        {
            observation.fact.freshness = Freshness::Stale;
        }
    }
    failed_observations.observations.push(MachineObservation {
        capability_id: "add".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The current add check failed.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });
    let failed = evaluate(failed_spec, failed_observations).unwrap();
    assert_focus(
        &failed,
        "add",
        FocusKind::FailedCheck,
        "FAILED CHECK",
        "NEXT CHECK",
        "replace or mark stale the failed record",
    );

    let mut conflicting_spec = base_spec;
    conflicting_spec.project.pinned_keystone = Some("reopen".into());
    let mut conflicting_observations = base_observations;
    conflicting_observations
        .observations
        .push(MachineObservation {
            capability_id: "reopen".into(),
            source: MachineSource::ImportedTestResult,
            fact: EvidenceFact {
                claim: Claim::VerificationPassed,
                freshness: Freshness::Current,
                summary: "A conflicting current result says reopen passed.".into(),
                location: Some(EvidenceLocation {
                    path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                    line_start: None,
                    line_end: None,
                }),
            },
        });
    let conflicting = evaluate(conflicting_spec, conflicting_observations).unwrap();
    assert_focus(
        &conflicting,
        "reopen",
        FocusKind::ResolveConflict,
        "RESOLVE CONFLICT",
        "NEXT CHECK",
        "Reconcile or mark stale the conflicting records",
    );
}

#[test]
fn a_project_without_an_accepted_core_journey_is_rejected() {
    let (mut spec, observations) = load_project(fixture_root()).unwrap();
    spec.project.pinned_keystone = None;
    for capability in &mut spec.capabilities {
        capability.role = CapabilityRole::Optional;
        capability.depends_on.clear();
    }

    let error = evaluate(spec, observations).expect_err("an empty core journey is not proven");
    assert!(
        error
            .to_string()
            .contains("project must define at least one core capability")
    );
}

#[test]
fn capability_ids_use_a_portable_command_safe_grammar() {
    for invalid_id in [
        "NeedsProof",
        "needs proof",
        "1needs-proof",
        "-needs-proof",
        "needs/proof",
        "réopen",
    ] {
        let (mut spec, observations) = load_project(fixture_root()).unwrap();
        spec.capabilities.push(CapabilityIntent {
            id: invalid_id.into(),
            label: "Invalid command ID".into(),
            map_label: None,
            role: CapabilityRole::Optional,
            depends_on: Vec::new(),
            proof_needed: "Confirm invalid IDs are rejected.".into(),
            notes: None,
            manual_evidence: Vec::new(),
        });

        let error = evaluate(spec, observations).expect_err("unsafe ID should be rejected");
        assert!(
            error.to_string().contains(
                "must start with a lowercase letter and use only lowercase letters, digits, hyphens, or underscores"
            ),
            "unexpected error for {invalid_id}: {error}"
        );
    }
}

#[test]
fn a_static_scan_cannot_claim_runtime_absence_or_proof() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::StaticScan,
        fact: EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "invalid authority".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });

    let error = evaluate(spec, observations).expect_err("invalid source claim must fail");
    assert!(
        error
            .to_string()
            .contains("StaticScan cannot assert VerificationPassed")
    );
}

#[test]
fn machine_observations_must_reference_inspectable_evidence() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::StaticScan,
        fact: EvidenceFact {
            claim: Claim::ImplementationPresent,
            freshness: Freshness::Current,
            summary: "A scanner claims implementation exists without saying where.".into(),
            location: None,
        },
    });

    let error = evaluate(spec, observations).expect_err("opaque machine evidence must fail");
    assert!(
        error
            .to_string()
            .contains("machine evidence for find must have an inspectable location")
    );
}

#[test]
fn contradictory_current_evidence_stays_visible() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "reopen".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "A conflicting imported result claims the flow passed.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });

    let evaluated = evaluate(spec, observations).expect("conflicts are a display state");
    assert_eq!(
        evaluated.capability("reopen").unwrap().display,
        DisplayState::Conflicting
    );
}

#[test]
fn unknown_observations_and_parent_paths_are_rejected() {
    let (spec, _) = load_project(fixture_root()).unwrap();
    let observations = ObservationSet {
        schema_version: 1,
        observations: vec![MachineObservation {
            capability_id: "ghost".into(),
            source: MachineSource::StaticScan,
            fact: EvidenceFact {
                claim: Claim::ImplementationPresent,
                freshness: Freshness::Current,
                summary: "Ghost source".into(),
                location: Some(EvidenceLocation {
                    path: "../outside.rs".into(),
                    line_start: Some(1),
                    line_end: Some(1),
                }),
            },
        }],
    };

    let error = evaluate(spec, observations).unwrap_err().to_string();
    assert!(error.contains("unknown capability ghost"));
    assert!(error.contains("repo-relative path without .."));
}

#[test]
fn a_human_pin_overrides_the_default_severity_order() {
    let (mut spec, observations) = load_project(fixture_root()).unwrap();
    spec.project.pinned_keystone = Some("save".into());

    let evaluated = evaluate(spec, observations).unwrap();
    let CurrentFocus::Capability {
        capability, pinned, ..
    } = evaluated.current_focus()
    else {
        panic!("the human pin should remain the current focus");
    };
    assert_eq!(capability.intent.id, "save");
    assert!(pinned);
}

#[test]
fn stale_proof_remains_visible_without_counting_as_current_proof() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    for observation in &mut observations.observations {
        if observation.capability_id == "add" && observation.fact.claim == Claim::VerificationPassed
        {
            observation.fact.freshness = Freshness::Stale;
        }
    }

    let evaluated = evaluate(spec, observations).unwrap();
    assert_eq!(
        evaluated.capability("add").unwrap().display,
        DisplayState::BuiltUnproven
    );
    assert!(
        evaluated
            .warnings
            .contains(&ModelWarning::StaleEvidenceOnly("add".into()))
    );
}

#[test]
fn current_failure_precedes_the_stale_pass_it_replaced() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    for observation in &mut observations.observations {
        if observation.capability_id == "add" && observation.fact.claim == Claim::VerificationPassed
        {
            observation.fact.freshness = Freshness::Stale;
        }
    }
    observations.observations.push(MachineObservation {
        capability_id: "add".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The replacement add-recipe check failed.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });

    let evaluated = evaluate(spec, observations).unwrap();
    let reasons = &evaluated.capability("add").unwrap().reasons;

    assert_eq!(reasons[0].fact.claim, Claim::VerificationFailed);
    assert_eq!(reasons[0].fact.freshness, Freshness::Current);
    assert_eq!(reasons[1].fact.claim, Claim::ImplementationPresent);
    assert_eq!(reasons[1].fact.freshness, Freshness::Current);
    assert_eq!(reasons[2].fact.claim, Claim::VerificationPassed);
    assert_eq!(reasons[2].fact.freshness, Freshness::Stale);
}

#[test]
fn the_evidence_that_forms_a_conflict_precedes_other_current_history() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "reopen".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "A current check failed before the reopen flow ran.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });
    observations.observations.push(MachineObservation {
        capability_id: "reopen".into(),
        source: MachineSource::StaticScan,
        fact: EvidenceFact {
            claim: Claim::ImplementationPresent,
            freshness: Freshness::Current,
            summary: "A current scan found a reopen implementation.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/src/storage.rs".into(),
                line_start: Some(1),
                line_end: Some(8),
            }),
        },
    });

    let evaluated = evaluate(spec, observations).unwrap();
    let capability = evaluated.capability("reopen").unwrap();

    assert_eq!(capability.display, DisplayState::Conflicting);
    assert_eq!(
        capability.reasons[0].fact.claim,
        Claim::ImplementationPresent
    );
    assert_eq!(
        capability.reasons[1].fact.claim,
        Claim::ImplementationAbsent
    );
    assert_eq!(capability.reasons[2].fact.claim, Claim::VerificationFailed);
}

#[test]
fn opposing_verification_facts_precede_duplicate_current_history() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    for (claim, summary) in [
        (
            Claim::VerificationPassed,
            "A second current add check also passed.",
        ),
        (
            Claim::VerificationFailed,
            "The newest current add check failed.",
        ),
    ] {
        observations.observations.push(MachineObservation {
            capability_id: "add".into(),
            source: MachineSource::ImportedTestResult,
            fact: EvidenceFact {
                claim,
                freshness: Freshness::Current,
                summary: summary.into(),
                location: Some(EvidenceLocation {
                    path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                    line_start: None,
                    line_end: None,
                }),
            },
        });
    }

    let evaluated = evaluate(spec, observations).unwrap();
    let reasons = &evaluated.capability("add").unwrap().reasons;

    assert_eq!(reasons[0].fact.claim, Claim::VerificationPassed);
    assert_eq!(reasons[1].fact.claim, Claim::VerificationFailed);
    assert_eq!(reasons[2].fact.claim, Claim::ImplementationPresent);
    assert_eq!(reasons[3].fact.claim, Claim::VerificationPassed);
}

#[test]
fn absent_and_passing_facts_precede_duplicate_current_history() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    for summary in [
        "One current reopen check passed.",
        "A duplicate current reopen check also passed.",
    ] {
        observations.observations.push(MachineObservation {
            capability_id: "reopen".into(),
            source: MachineSource::ImportedTestResult,
            fact: EvidenceFact {
                claim: Claim::VerificationPassed,
                freshness: Freshness::Current,
                summary: summary.into(),
                location: Some(EvidenceLocation {
                    path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                    line_start: None,
                    line_end: None,
                }),
            },
        });
    }

    let evaluated = evaluate(spec, observations).unwrap();
    let reasons = &evaluated.capability("reopen").unwrap().reasons;

    assert_eq!(reasons[0].fact.claim, Claim::VerificationPassed);
    assert_eq!(reasons[1].fact.claim, Claim::ImplementationAbsent);
    assert_eq!(reasons[2].fact.claim, Claim::VerificationPassed);
}

#[test]
fn blocked_descendants_are_found_through_a_proven_intermediate() {
    let (mut spec, mut observations) = load_project(fixture_root()).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "A recorded search check passed.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });
    spec.capabilities.push(CapabilityIntent {
        id: "use".into(),
        label: "Use the found recipe".into(),
        map_label: Some("Use".into()),
        role: CapabilityRole::Core { order: 5 },
        depends_on: vec!["find".into()],
        proof_needed: "Open the found recipe and follow one instruction.".into(),
        notes: None,
        manual_evidence: Vec::new(),
    });

    let evaluated = evaluate(spec, observations).unwrap();
    let CurrentFocus::Capability {
        capability,
        summary,
        ..
    } = evaluated.current_focus()
    else {
        panic!("the missing capability should remain the current focus");
    };
    assert_eq!(capability.intent.id, "reopen");
    assert!(summary.ends_with("Downstream unresolved: Use the found recipe."));
}

#[test]
fn blocked_core_descendants_are_found_through_a_supporting_bridge() {
    let (mut spec, observations) = load_project(fixture_root()).unwrap();
    spec.capabilities.push(CapabilityIntent {
        id: "reopen_bridge".into(),
        label: "Reopen bridge".into(),
        map_label: Some("Bridge".into()),
        role: CapabilityRole::Supporting {
            supports: "find".into(),
        },
        depends_on: vec!["reopen".into()],
        proof_needed: "Connect reopening to a follow-on journey step.".into(),
        notes: None,
        manual_evidence: Vec::new(),
    });
    spec.capabilities.push(CapabilityIntent {
        id: "use".into(),
        label: "Use the reopened recipe".into(),
        map_label: Some("Use".into()),
        role: CapabilityRole::Core { order: 5 },
        depends_on: vec!["reopen_bridge".into()],
        proof_needed: "Follow one instruction in the reopened recipe.".into(),
        notes: None,
        manual_evidence: Vec::new(),
    });

    let evaluated = evaluate(spec, observations).unwrap();
    let CurrentFocus::Capability {
        capability,
        summary,
        ..
    } = evaluated.current_focus()
    else {
        panic!("the missing capability should remain the current focus");
    };

    assert_eq!(capability.intent.id, "reopen");
    assert!(summary.contains("Use the reopened recipe"));
}

#[test]
fn a_failing_proof_does_not_claim_that_implementation_exists() {
    let (spec, mut observations) = load_project(fixture_root()).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The recorded find check failed before reaching implementation.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });
    observations.observations.push(MachineObservation {
        capability_id: "reopen".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The recorded reopen check also failed.".into(),
            location: Some(EvidenceLocation {
                path: "fixtures/recipe_box/artifacts/test-results.json".into(),
                line_start: None,
                line_end: None,
            }),
        },
    });

    let evaluated = evaluate(spec, observations).unwrap();
    let find = evaluated.capability("find").unwrap();
    assert_eq!(
        find.implementation,
        proof_lantern::ImplementationState::Unknown
    );
    assert_eq!(find.display, DisplayState::ProofFailed);
    assert_eq!(
        evaluated.capability("reopen").unwrap().display,
        DisplayState::Missing
    );
}
