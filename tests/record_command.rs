use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "terminal-test-hooks")]
use std::process::Stdio;

use proof_lantern::{DisplayState, Freshness, ManualEvidenceSet, evaluate, load_project};

struct TempProject(PathBuf);

impl TempProject {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-record-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("isolated project directory should be created");
        let output = run(&["init"], &root);
        assert!(output.status.success(), "{:?}", output.stderr);
        Self(root)
    }

    fn root(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(arguments: &[&str], root: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .args(arguments)
        .arg(root)
        .output()
        .expect("Proof Lantern should launch")
}

#[test]
fn record_adds_human_evidence_without_rewriting_intent() {
    let project = TempProject::new("passed");
    let project_file = project.root().join(".proof-lantern/project.yml");
    let before = fs::read(&project_file).expect("starter map should be readable");

    let output = run(
        &[
            "record",
            "start",
            "passed",
            "--summary",
            "I completed the visible starting step.",
        ],
        project.root(),
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let stdout = String::from_utf8(output.stdout).expect("output should be UTF-8");
    assert!(stdout.contains("Recorded PROVEN"), "{stdout}");
    assert!(stdout.contains("manual-evidence.json"), "{stdout}");
    assert!(
        project.root().join(".proof-lantern/.record.lock").is_file(),
        "record should keep its coordination lock inside the project boundary"
    );
    assert_eq!(
        fs::read(&project_file).expect("project intent should remain readable"),
        before,
        "record must not reserialize the human-authored project map"
    );

    let (spec, observations) = load_project(project.root()).expect("recorded map should load");
    let evaluated = evaluate(spec, observations).expect("recorded map should evaluate");
    assert_eq!(
        evaluated.capability("start").unwrap().display,
        DisplayState::Proven
    );
}

#[test]
fn generated_path_free_record_command_runs_from_a_map_root_with_spaces() {
    let project = TempProject::new("portable path's map");
    let output = Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .current_dir(project.root())
        .args([
            "record",
            "--summary",
            "I completed the visible starting step.",
            "--",
            "start",
            "passed",
        ])
        .output()
        .expect("generated record command should launch");
    assert!(output.status.success(), "{:?}", output.stderr);
    let stdout = String::from_utf8(output.stdout).expect("output should be UTF-8");

    assert!(
        stdout.contains("Next: run `proof-lantern next .` from this map root."),
        "{stdout}"
    );
    assert!(
        project
            .root()
            .join(".proof-lantern/manual-evidence.json")
            .is_file()
    );
}

#[cfg(unix)]
#[test]
fn record_refuses_a_lock_symlink_that_escapes_the_project() {
    use std::os::unix::fs::symlink;

    let project = TempProject::new("lock-symlink");
    let outside = project.root().with_extension("outside-lock");
    fs::write(&outside, b"outside remains unchanged").expect("outside file should be created");
    symlink(&outside, project.root().join(".proof-lantern/.record.lock"))
        .expect("test lock symlink should be created");

    let output = run(
        &[
            "record",
            "start",
            "passed",
            "--summary",
            "This must not follow the lock symlink.",
        ],
        project.root(),
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error should be UTF-8");
    assert!(
        stderr.contains("could not lock manual evidence"),
        "{stderr}"
    );
    assert_eq!(
        fs::read(&outside).expect("outside file should remain readable"),
        b"outside remains unchanged"
    );
    let _ = fs::remove_file(outside);
}

#[test]
fn record_keeps_superseded_manual_results_as_stale_history() {
    let project = TempProject::new("history");
    let passed = run(
        &[
            "record",
            "start",
            "passed",
            "--summary",
            "The first check passed.",
        ],
        project.root(),
    );
    assert!(passed.status.success(), "{:?}", passed.stderr);

    let failed = run(
        &[
            "record",
            "start",
            "failed",
            "--summary",
            "The current check now fails.",
        ],
        project.root(),
    );
    assert!(failed.status.success(), "{:?}", failed.stderr);
    let stdout = String::from_utf8(failed.stdout).expect("output should be UTF-8");
    assert!(stdout.contains("Recorded PROOF FAILED"), "{stdout}");
    assert!(
        stdout.contains("older manual record(s) as STALE"),
        "{stdout}"
    );

    let manual: ManualEvidenceSet = serde_json::from_str(
        &fs::read_to_string(project.root().join(".proof-lantern/manual-evidence.json"))
            .expect("manual evidence should be readable"),
    )
    .expect("manual evidence should be valid JSON");
    assert_eq!(manual.records.len(), 2);
    assert!(
        manual
            .records
            .iter()
            .all(|record| record.fact.receipt.is_some())
    );
    assert_eq!(manual.records[0].fact.freshness, Freshness::Stale);
    assert_eq!(manual.records[1].fact.freshness, Freshness::Current);

    let (spec, observations) = load_project(project.root()).expect("recorded map should load");
    let evaluated = evaluate(spec, observations).expect("recorded map should evaluate");
    assert_eq!(
        evaluated.capability("start").unwrap().display,
        DisplayState::ProofFailed
    );
}

#[test]
fn receipt_tracks_selected_bytes_and_load_does_not_rewrite_history() {
    use sha2::{Digest, Sha256};

    let project = TempProject::new("receipt changes");
    fs::write(project.root().join("source.txt"), b"before").unwrap();
    fs::write(project.root().join("binary input.bin"), [0, 255, 42]).unwrap();
    let output = run(
        &[
            "record",
            "start",
            "passed",
            "--summary",
            "I checked the starting step.",
            "--input",
            "source.txt",
            "--input",
            "binary input.bin",
            "--input",
            "source.txt",
        ],
        project.root(),
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence_file = project.root().join(".proof-lantern/manual-evidence.json");
    let before = fs::read(&evidence_file).unwrap();
    let manual: ManualEvidenceSet = serde_json::from_slice(&before).unwrap();
    let receipt = manual.records[0].fact.receipt.as_ref().unwrap();
    assert!(
        time::OffsetDateTime::parse(
            &receipt.recorded_at,
            &time::format_description::well_known::Rfc3339
        )
        .is_ok()
    );
    assert_eq!(receipt.inputs.len(), 2);
    assert_eq!(receipt.inputs[0].path, "binary input.bin");
    assert_eq!(
        receipt.inputs[0].sha256,
        format!("{:x}", Sha256::digest([0, 255, 42]))
    );
    assert!(!String::from_utf8_lossy(&before).contains("receipt_check"));

    let (spec, observations) = load_project(project.root()).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("start")
            .unwrap()
            .display,
        DisplayState::Proven
    );
    // Same length: this must detect bytes, not merely file size or line count.
    fs::write(project.root().join("source.txt"), b"after!").unwrap();
    let output = run(&["next"], project.root());
    assert!(output.status.success(), "{:?}", output.stderr);
    let next = String::from_utf8(output.stdout).unwrap();
    assert!(next.contains("RECHECK NEEDED"), "{next}");
    assert!(next.contains("source.txt"), "{next}");
    assert!(!next.contains("CORE JOURNEY PROVEN"), "{next}");
    let output = run(&["explain", "start"], project.root());
    assert!(output.status.success(), "{:?}", output.stderr);
    let explain = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "Source: HUMAN",
        "Freshness: STALE",
        "Recorded at:",
        "NEEDS RECHECK",
        "Tracked inputs (SHA-256)",
    ] {
        assert!(explain.contains(expected), "missing {expected}: {explain}");
    }
    assert_eq!(fs::read(&evidence_file).unwrap(), before);
    // Supersede the old receipt without losing its fingerprint or mutating intent.
    let output = run(
        &[
            "record",
            "start",
            "passed",
            "--summary",
            "I checked the changed starting step.",
            "--input",
            "source.txt",
        ],
        project.root(),
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let manual: ManualEvidenceSet =
        serde_json::from_slice(&fs::read(evidence_file).unwrap()).unwrap();
    assert_eq!(manual.records[0].fact.freshness, Freshness::Stale);
    assert_eq!(manual.records[0].fact.receipt.as_ref(), Some(receipt));
    assert_eq!(manual.records[1].fact.freshness, Freshness::Current);
    let (spec, observations) = load_project(project.root()).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("start")
            .unwrap()
            .display,
        DisplayState::Proven
    );
}

#[test]
fn receipt_without_inputs_tracks_check_text_but_does_not_claim_code_tracking() {
    let project = TempProject::new("receipt check text");
    let output = run(
        &["record", "start", "failed", "--summary", "Starting failed."],
        project.root(),
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("No input files tracked")
    );
    fs::write(project.root().join("untracked.rs"), "changed").unwrap();
    let (spec, observations) = load_project(project.root()).unwrap();
    assert_eq!(
        evaluate(spec, observations)
            .unwrap()
            .capability("start")
            .unwrap()
            .display,
        DisplayState::ProofFailed
    );
    let file = project.root().join(".proof-lantern/project.yml");
    let text = fs::read_to_string(&file).unwrap().replace(
        "Describe one visible check that proves the journey can start.",
        "Use the new starting workflow.",
    );
    fs::write(file, text).unwrap();
    let output = run(&["explain", "start"], project.root());
    assert!(output.status.success(), "{:?}", output.stderr);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Freshness: STALE"), "{text}");
    assert!(text.contains("Proof requirement changed"), "{text}");
    assert!(
        text.contains("Tracked inputs: none; code changes are not detected."),
        "{text}"
    );
}

#[test]
fn invalid_receipt_inputs_never_replace_existing_evidence() {
    let project = TempProject::new("invalid receipt");
    for directory in ["nested/.proof-lantern", "vendor/.GIT"] {
        fs::create_dir_all(project.root().join(directory)).unwrap();
    }
    fs::write(
        project.root().join("nested/.proof-lantern/project.yml"),
        "nested metadata",
    )
    .unwrap();
    fs::write(project.root().join("vendor/.GIT/config"), "nested metadata").unwrap();
    let output = run(
        &[
            "record",
            "start",
            "built",
            "--summary",
            "Implementation exists.",
        ],
        project.root(),
    );
    assert!(output.status.success(), "{:?}", output.stderr);
    let evidence_file = project.root().join(".proof-lantern/manual-evidence.json");
    let before = fs::read(&evidence_file).unwrap();
    for input in [
        "../escape.txt",
        "/absolute.txt",
        ".proof-lantern/manual-evidence.json",
        ".git/config",
        ".PROOF-LANTERN/manual-evidence.json",
        "nested/.proof-lantern/project.yml",
        "vendor/.GIT/config",
        ".proof-lantern./manual-evidence.json",
        "absent.txt",
        "bad:stream",
        "./alias.txt",
    ] {
        let output = run(
            &[
                "record",
                "start",
                "passed",
                "--summary",
                "Must not write.",
                "--input",
                input,
            ],
            project.root(),
        );
        assert!(!output.status.success(), "unexpected success for {input}");
        assert_eq!(fs::read(&evidence_file).unwrap(), before);
    }
}

#[test]
fn record_rejects_unknown_capabilities_without_creating_evidence() {
    let project = TempProject::new("unknown");
    let output = run(
        &[
            "record",
            "not-a-node",
            "passed",
            "--summary",
            "This should not be recorded.",
        ],
        project.root(),
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error should be UTF-8");
    assert!(stderr.contains("unknown capability not-a-node"), "{stderr}");
    assert!(stderr.contains("Valid capability IDs:"), "{stderr}");
    for id in ["start", "outcome", "return"] {
        assert!(stderr.contains(id), "missing {id} in:\n{stderr}");
    }
    assert!(
        !project
            .root()
            .join(".proof-lantern/manual-evidence.json")
            .exists()
    );
}

#[test]
fn record_help_describes_node_and_shows_truthful_examples() {
    let output = Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .args(["record", "--help"])
        .output()
        .expect("Proof Lantern help should launch");
    assert!(output.status.success(), "{:?}", output.stderr);
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    let stdout = String::from_utf8(output.stdout).expect("help should be UTF-8");

    assert!(
        stdout.contains("Capability ID from .proof-lantern/project.yml"),
        "{stdout}"
    );
    assert!(stdout.contains("What you observed"), "{stdout}");
    assert!(stdout.contains("Examples:"), "{stdout}");
    assert!(
        stdout.contains("proof-lantern record reopen passed"),
        "{stdout}"
    );
    assert!(
        stdout.contains("proof-lantern record save failed"),
        "{stdout}"
    );
}

#[test]
fn record_refuses_to_hide_conflicting_project_authored_evidence() {
    let project = TempProject::new("conflict");
    let project_file = project.root().join(".proof-lantern/project.yml");
    let authored = fs::read_to_string(&project_file).expect("starter map should be readable");
    let patched = authored.replace(
        "    proof_needed: Describe one visible check that proves the journey can start.\n",
        concat!(
            "    proof_needed: Describe one visible check that proves the journey can start.\n",
            "    manual_evidence:\n",
            "      - claim: verification_passed\n",
            "        freshness: current\n",
            "        summary: The project author recorded a current passing check.\n",
        ),
    );
    assert_ne!(
        patched, authored,
        "starter map wording changed; update this fixture edit"
    );
    fs::write(&project_file, patched).expect("project-authored evidence should be written");

    let output = run(
        &[
            "record",
            "start",
            "failed",
            "--summary",
            "A newer check failed.",
        ],
        project.root(),
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error should be UTF-8");
    assert!(stderr.contains("would leave start CONFLICTING"), "{stderr}");
    assert!(stderr.contains("reconcile current evidence"), "{stderr}");
    assert!(
        !project
            .root()
            .join(".proof-lantern/manual-evidence.json")
            .exists()
    );
}

#[cfg(feature = "terminal-test-hooks")]
#[test]
fn concurrent_record_processes_preserve_all_results() {
    let project = TempProject::new("concurrent");
    let spawn = |summary: &str| {
        Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
            .args(["record", "start", "passed", "--summary", summary])
            .arg(project.root())
            .env("PROOF_LANTERN_TEST_RECORD_PAUSE_MS", "50")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("concurrent Proof Lantern process should start")
    };

    let summaries: Vec<_> = (1..=8)
        .map(|index| format!("Concurrent check {index} passed."))
        .collect();
    let children: Vec<_> = summaries.iter().map(|summary| spawn(summary)).collect();
    // Reap every process before an assertion can drop the temporary project.
    let outputs: Vec<_> = children
        .into_iter()
        .map(|child| {
            child
                .wait_with_output()
                .expect("concurrent Proof Lantern process should finish")
        })
        .collect();
    for output in outputs {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let manual: ManualEvidenceSet = serde_json::from_str(
        &fs::read_to_string(project.root().join(".proof-lantern/manual-evidence.json"))
            .expect("manual evidence should be readable"),
    )
    .expect("manual evidence should be valid JSON");
    assert_eq!(manual.records.len(), summaries.len(), "{manual:?}");
    assert!(
        manual
            .records
            .iter()
            .all(|record| record.fact.receipt.is_some())
    );
    assert_eq!(
        manual
            .records
            .iter()
            .filter(|record| record.fact.freshness == Freshness::Current)
            .count(),
        1
    );
    assert_eq!(
        manual
            .records
            .iter()
            .filter(|record| record.fact.freshness == Freshness::Stale)
            .count(),
        summaries.len() - 1
    );
    for summary in summaries {
        assert!(
            manual
                .records
                .iter()
                .any(|record| record.fact.summary == summary),
            "{manual:?}"
        );
    }
}
