use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

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
    assert!(
        !project
            .root()
            .join(".proof-lantern/manual-evidence.json")
            .exists()
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
fn concurrent_record_processes_preserve_both_results() {
    let project = TempProject::new("concurrent");
    let spawn = |summary: &str| {
        Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
            .args(["record", "start", "passed", "--summary", summary])
            .arg(project.root())
            .env("PROOF_LANTERN_TEST_RECORD_PAUSE_MS", "250")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("concurrent Proof Lantern process should start")
    };

    let first = spawn("The first concurrent check passed.");
    let second = spawn("The second concurrent check passed.");
    for child in [first, second] {
        let output = child
            .wait_with_output()
            .expect("concurrent Proof Lantern process should finish");
        assert!(output.status.success(), "{:?}", output.stderr);
    }

    let manual: ManualEvidenceSet = serde_json::from_str(
        &fs::read_to_string(project.root().join(".proof-lantern/manual-evidence.json"))
            .expect("manual evidence should be readable"),
    )
    .expect("manual evidence should be valid JSON");
    assert_eq!(manual.records.len(), 2, "{manual:?}");
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
        1
    );
    for summary in [
        "The first concurrent check passed.",
        "The second concurrent check passed.",
    ] {
        assert!(
            manual
                .records
                .iter()
                .any(|record| record.fact.summary == summary),
            "{manual:?}"
        );
    }
}
