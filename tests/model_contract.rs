use std::path::PathBuf;

use proof_lantern::{
    CapabilityIntent, CapabilityRole, Claim, DisplayState, EvidenceFact, EvidenceLocation,
    Freshness, MachineObservation, MachineSource, ModelWarning, ObservationSet, evaluate,
    load_project,
};

fn fixture_paths() -> (PathBuf, PathBuf) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box/.proof-lantern");
    (root.join("project.yml"), root.join("observations.json"))
}

#[test]
fn recipe_box_derives_truthful_states_and_the_keystone_gap() {
    let (project_path, observations_path) = fixture_paths();
    let (spec, observations) =
        load_project(project_path, observations_path).expect("fixture should load");
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

    let gap = evaluated.keystone.expect("a keystone gap should exist");
    assert_eq!(gap.capability_id, "reopen");
    assert_eq!(gap.state, DisplayState::Missing);
    assert_eq!(gap.blocked_core_ids, ["find"]);
    assert!(!gap.pinned);
}

#[test]
fn a_static_scan_cannot_claim_runtime_absence_or_proof() {
    let (project_path, observations_path) = fixture_paths();
    let (spec, mut observations) = load_project(project_path, observations_path).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::StaticScan,
        fact: EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "invalid authority".into(),
            location: None,
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
fn contradictory_current_evidence_stays_visible() {
    let (project_path, observations_path) = fixture_paths();
    let (spec, mut observations) = load_project(project_path, observations_path).unwrap();
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
    let (project_path, observations_path) = fixture_paths();
    let (spec, _) = load_project(project_path, observations_path).unwrap();
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
    let (project_path, observations_path) = fixture_paths();
    let (mut spec, observations) = load_project(project_path, observations_path).unwrap();
    spec.project.pinned_keystone = Some("save".into());

    let evaluated = evaluate(spec, observations).unwrap();
    let gap = evaluated.keystone.unwrap();
    assert_eq!(gap.capability_id, "save");
    assert!(gap.pinned);
}

#[test]
fn stale_proof_remains_visible_without_counting_as_current_proof() {
    let (project_path, observations_path) = fixture_paths();
    let (spec, mut observations) = load_project(project_path, observations_path).unwrap();
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
fn blocked_descendants_are_found_through_a_proven_intermediate() {
    let (project_path, observations_path) = fixture_paths();
    let (mut spec, mut observations) = load_project(project_path, observations_path).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "A recorded search check passed.".into(),
            location: None,
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
    let gap = evaluated.keystone.unwrap();
    assert_eq!(gap.capability_id, "reopen");
    assert_eq!(gap.blocked_core_ids, ["use"]);
}

#[test]
fn a_failing_proof_does_not_claim_that_implementation_exists() {
    let (project_path, observations_path) = fixture_paths();
    let (spec, mut observations) = load_project(project_path, observations_path).unwrap();
    observations.observations.push(MachineObservation {
        capability_id: "find".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The recorded find check failed before reaching implementation.".into(),
            location: None,
        },
    });
    observations.observations.push(MachineObservation {
        capability_id: "reopen".into(),
        source: MachineSource::ImportedTestResult,
        fact: EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The recorded reopen check also failed.".into(),
            location: None,
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
