use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use proof_lantern::{DisplayState, VerificationState, evaluate, load_project};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(label: &str, proof_needed: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-receipt-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join(".proof-lantern")).unwrap();
        fs::write(
            root.join(".proof-lantern/project.yml"),
            format!(
                "schema_version: 1\nproject:\n  name: Receipt test\n  promise: Keep evidence current.\ncapabilities:\n  - id: inspect\n    label: Inspect\n    role:\n      kind: core\n      order: 1\n    proof_needed: {proof_needed:?}\n"
            ),
        )
        .unwrap();
        fs::write(
            root.join("evidence.txt"),
            "Inspectable evidence location.\n",
        )
        .unwrap();
        Self { root }
    }

    fn write_file(&self, path: &str, bytes: &[u8]) {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, bytes).unwrap();
    }

    fn write_observation(&self, fact: Value) {
        fs::write(
            self.root.join(".proof-lantern/observations.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "observations": [{
                    "capability_id": "inspect",
                    "source": "imported_test_result",
                    "fact": fact,
                }]
            }))
            .unwrap(),
        )
        .unwrap();
    }

    fn write_manual_fact(&self, fact: Value) {
        fs::write(
            self.root.join(".proof-lantern/manual-evidence.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "records": [{ "capability_id": "inspect", "fact": fact }],
            }))
            .unwrap(),
        )
        .unwrap();
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn receipt(inputs: Vec<Value>, proof_needed: &str) -> Value {
    json!({
        "recorded_at": "2026-09-07T12:34:56Z",
        "proof_needed": proof_needed,
        "inputs": inputs,
    })
}

fn current_fact(receipt: Value) -> Value {
    json!({
        "claim": "verification_passed",
        "freshness": "current",
        "summary": "Observed a passing check.",
        "location": { "path": "evidence.txt" },
        "receipt": receipt,
    })
}

fn input(path: &str, bytes: &[u8]) -> Value {
    json!({ "path": path, "sha256": digest(bytes) })
}

#[test]
fn unknown_observations_keep_the_evaluation_diagnostic_with_or_without_receipts() {
    for tracked in [false, true] {
        let project = TestProject::new("unknown-observation", "Run the check.");
        let mut fact = current_fact(receipt(vec![], "Run the check."));
        if !tracked {
            fact.as_object_mut().unwrap().remove("receipt");
        }
        project.write_file(
            ".proof-lantern/observations.json",
            &serde_json::to_vec(&json!({
                "schema_version": 1,
                "observations": [{
                    "capability_id": "ghost",
                    "source": "imported_test_result",
                    "fact": fact,
                }],
            }))
            .unwrap(),
        );
        let (spec, observations) = load_project(&project.root)
            .expect("receipt refresh must leave unknown references to model validation");
        let error = evaluate(spec, observations).unwrap_err().to_string();
        assert!(
            error.contains("observation references unknown capability ghost"),
            "{error}"
        );
    }
}

#[test]
fn matching_binary_receipt_is_current_and_unrelated_files_do_not_invalidate_it() {
    let project = TestProject::new("matching", "Run the binary check.");
    let binary = [0_u8, 255, 14, 99, 0, 128];
    project.write_file("artifacts/proof.bin", &binary);
    project.write_observation(current_fact(receipt(
        vec![input("artifacts/proof.bin", &binary)],
        "Run the binary check.",
    )));

    project.write_file("unrelated.txt", b"not a receipt input");
    let (spec, observations) = load_project(&project.root).unwrap();
    let evaluated = evaluate(spec, observations).unwrap();
    assert_eq!(
        evaluated.capability("inspect").unwrap().display,
        DisplayState::Proven
    );
}

#[test]
fn same_size_change_and_deleted_input_keep_the_project_loadable_but_stale() {
    for (label, replacement) in [("changed", Some(b"after!".as_slice())), ("deleted", None)] {
        let project = TestProject::new(label, "Run the check.");
        project.write_file("proof.txt", b"before");
        project.write_observation(current_fact(receipt(
            vec![input("proof.txt", b"before")],
            "Run the check.",
        )));
        match replacement {
            Some(bytes) => project.write_file("proof.txt", bytes),
            None => fs::remove_file(project.root.join("proof.txt")).unwrap(),
        }

        let (spec, observations) = load_project(&project.root).unwrap();
        let evaluated = evaluate(spec, observations).unwrap();
        assert_eq!(
            evaluated.capability("inspect").unwrap().verification,
            VerificationState::Stale,
            "{label} receipt should cease being current"
        );
    }
}

#[test]
fn changed_proof_requirement_stales_the_receipt_without_reading_inputs() {
    let project = TestProject::new("proof-change", "Use the newer proof.");
    project.write_observation(current_fact(receipt(
        vec![input("retired.txt", b"old")],
        "Use the older proof.",
    )));

    let (spec, observations) = load_project(&project.root).unwrap();
    let evaluated = evaluate(spec, observations).unwrap();
    assert_eq!(
        evaluated.capability("inspect").unwrap().verification,
        VerificationState::Stale
    );
}

#[test]
fn legacy_facts_remain_current_without_a_receipt() {
    let project = TestProject::new("legacy", "Run the check.");
    project.write_observation(json!({
        "claim": "verification_passed",
        "freshness": "current",
        "summary": "A pre-receipt record.",
        "location": { "path": "evidence.txt" },
    }));

    let (spec, observations) = load_project(&project.root).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("inspect")
            .unwrap()
            .display,
        DisplayState::Proven
    );
}

#[test]
fn receipt_check_cannot_be_spoofed_through_serialized_evidence() {
    let project = TestProject::new("spoofed-check", "Run the check.");
    let mut fact = current_fact(receipt(
        vec![input("missing.txt", b"old")],
        "Run the check.",
    ));
    fact["receipt_check"] = json!("matched");
    project.write_observation(fact);

    let message = load_project(&project.root).unwrap_err().to_string();
    assert!(message.contains("receipt_check"), "{message}");
}

#[test]
fn changed_proof_requirement_invalidates_every_claim_kind() {
    for claim in [
        "implementation_present",
        "implementation_absent",
        "verification_passed",
        "verification_failed",
    ] {
        let project = TestProject::new(claim, "Use the newer proof.");
        let mut fact = current_fact(receipt(
            vec![input("retired.txt", b"old")],
            "Use the older proof.",
        ));
        fact["claim"] = json!(claim);
        fact.as_object_mut().unwrap().remove("location");
        project.write_manual_fact(fact);

        let (spec, _) = load_project(&project.root).unwrap();
        assert_eq!(
            spec.capabilities[0].manual_evidence[0].effective_freshness(),
            proof_lantern::Freshness::Stale,
            "{claim} must not retain current status after its proof requirement changes"
        );
    }
}

#[test]
fn authored_stale_receipts_do_not_touch_retired_inputs() {
    let project = TestProject::new("authored-stale", "Run the check.");
    let mut fact = current_fact(receipt(
        vec![input("retired.txt", b"old")],
        "Run the check.",
    ));
    fact["freshness"] = json!("stale");
    project.write_observation(fact);

    load_project(&project.root).expect("stale history must not read removed receipt inputs");
}

#[test]
fn malformed_receipt_fields_are_load_errors() {
    let cases = [
        (
            "bad-time",
            receipt(vec![], "Run the check.")
                .to_string()
                .replace("2026-09-07T12:34:56Z", "yesterday"),
        ),
        (
            "bad-digest",
            receipt(
                vec![json!({"path":"proof.txt", "sha256":"ABC"})],
                "Run the check.",
            )
            .to_string(),
        ),
        (
            "bad-path",
            receipt(
                vec![json!({"path":"../proof.txt", "sha256":digest(b"x")})],
                "Run the check.",
            )
            .to_string(),
        ),
        (
            "duplicate",
            receipt(
                vec![input("a.txt", b"x"), input("a.txt", b"x")],
                "Run the check.",
            )
            .to_string(),
        ),
    ];
    for (label, serialized) in cases {
        let project = TestProject::new(label, "Run the check.");
        let receipt: Value = serde_json::from_str(&serialized).unwrap();
        project.write_observation(current_fact(receipt));
        let message = load_project(&project.root).unwrap_err().to_string();
        assert!(message.contains("receipt"), "{label}: {message}");
    }
}

#[cfg(unix)]
#[test]
fn symlink_alias_to_metadata_cannot_be_used_as_a_current_input() {
    use std::os::unix::fs::symlink;

    let project = TestProject::new("metadata-alias", "Run the check.");
    symlink(
        project.root.join(".proof-lantern"),
        project.root.join("metadata_alias"),
    )
    .unwrap();
    project.write_observation(current_fact(receipt(
        vec![input("metadata_alias/project.yml", b"not the real bytes")],
        "Run the check.",
    )));

    let (spec, observations) = load_project(&project.root).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("inspect")
            .unwrap()
            .verification,
        VerificationState::Stale
    );
}

#[cfg(unix)]
#[test]
fn outside_symlink_and_directory_inputs_need_recheck_without_becoming_current() {
    use std::os::unix::fs::symlink;

    let outside = TestProject::new("outside-source", "Outside only.");
    outside.write_file("outside.txt", b"outside bytes");
    let project = TestProject::new("outside-link", "Run the check.");
    symlink(
        outside.root.join("outside.txt"),
        project.root.join("linked.txt"),
    )
    .unwrap();
    project.write_observation(current_fact(receipt(
        vec![input("linked.txt", b"outside bytes")],
        "Run the check.",
    )));
    let (spec, observations) = load_project(&project.root).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("inspect")
            .unwrap()
            .verification,
        VerificationState::Stale
    );

    let directory = TestProject::new("directory-input", "Run the check.");
    fs::create_dir(directory.root.join("not-a-file")).unwrap();
    directory.write_observation(current_fact(receipt(
        vec![input("not-a-file", b"not a file")],
        "Run the check.",
    )));
    let (spec, observations) = load_project(&directory.root).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("inspect")
            .unwrap()
            .verification,
        VerificationState::Stale
    );
}

#[test]
fn receipt_input_limit_is_rejected_before_any_files_are_read() {
    let project = TestProject::new("too-many", "Run the check.");
    let inputs = (0..33)
        .map(|index| json!({"path": format!("input-{index}.txt"), "sha256": digest(b"x")}))
        .collect();
    project.write_observation(current_fact(receipt(inputs, "Run the check.")));

    let message = load_project(&project.root).unwrap_err().to_string();
    assert!(message.contains("at most 32"), "{message}");
}

#[test]
fn oversized_receipt_input_becomes_stale_without_hashing_the_whole_file() {
    let project = TestProject::new("oversized", "Run the check.");
    let oversized = vec![b'x'; 16 * 1024 * 1024 + 1];
    project.write_file("large.bin", &oversized);
    project.write_observation(current_fact(receipt(
        vec![json!({ "path": "large.bin", "sha256": "0".repeat(64) })],
        "Run the check.",
    )));

    let (spec, observations) = load_project(&project.root).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("inspect")
            .unwrap()
            .verification,
        VerificationState::Stale
    );
}
