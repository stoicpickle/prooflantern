use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn run_command(arguments: &[&str], project_root: &Path) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_proof-lantern"));
    command.args(arguments).arg(project_root);
    command.output().expect("Proof Lantern should launch")
}

fn assert_success(output: &std::process::Output) -> String {
    assert!(output.status.success(), "{:?}", output.status);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {:?}",
        output.stderr
    );
    String::from_utf8(output.stdout.clone()).expect("command output should be UTF-8")
}

struct TempProject(PathBuf);

impl TempProject {
    fn with_proven_pin() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-warning-test-{}-{nonce}",
            std::process::id()
        ));
        let config = root.join(".proof-lantern");
        fs::create_dir_all(&config).expect("temporary project should be created");
        fs::write(
            config.join("project.yml"),
            concat!(
                "schema_version: 1\n",
                "project:\n",
                "  name: Completed Project\n",
                "  promise: Finish one accepted capability.\n",
                "  pinned_keystone: add\n",
                "capabilities:\n",
                "  - id: add\n",
                "    label: Add a recipe\n",
                "    role:\n",
                "      kind: core\n",
                "      order: 1\n",
                "    proof_needed: Confirm the accepted capability still works.\n",
                "    manual_evidence:\n",
                "      - claim: verification_passed\n",
                "        freshness: current\n",
                "        summary: Human verification passed.\n",
            ),
        )
        .expect("temporary project intent should be written");
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

#[test]
fn next_command_reports_the_state_sensitive_focus_and_proof_needed() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let output = run_command(&["next"], &project_root);
    let stdout = assert_success(&output);
    assert_eq!(
        stdout,
        concat!(
            "JOURNEY BREAK\n",
            "╳ Reopen saved recipes — MISSING\n",
            "Required implementation is recorded absent. ",
            "Downstream unresolved: Find a saved recipe.\n",
            "PROOF NEEDED: Close app, reopen it, and confirm the saved recipe appears.\n",
        )
    );
}

#[test]
fn next_command_reports_a_proven_core_journey_positively() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = run_command(&["next"], &project_root);
    let stdout = assert_success(&output);
    assert_eq!(
        stdout,
        concat!(
            "CORE JOURNEY PROVEN\n",
            "All accepted core capabilities have current recorded proof.\n",
        )
    );
}

#[test]
fn explain_command_prints_complete_evidence_metadata() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let output = run_command(&["explain", "add"], &project_root);
    let stdout = assert_success(&output);

    assert_eq!(
        stdout,
        concat!(
            "✓ Add a recipe — PROVEN\n",
            "Why: Current recorded proof supports this capability.\n",
            "Evidence:\n",
            "  - Source: STATIC SCAN\n",
            "    Freshness: CURRENT\n",
            "    Summary: Recipe creation code is present.\n",
            "    Location: src/add_recipe.rs:1-5\n",
            "  - Source: IMPORTED TEST RESULT\n",
            "    Freshness: CURRENT\n",
            "    Summary: The recorded add-recipe check passed.\n",
            "    Location: artifacts/test-results.json\n",
            "Proof needed: Create a recipe and confirm it appears in the current session.\n",
        )
    );
}

#[test]
fn next_and_explain_surface_evaluator_warnings() {
    let project = TempProject::with_proven_pin();
    let warning = concat!(
        "Warnings:\n",
        "  - Pinned focus \"Add a recipe\" is already proven; ",
        "automatic focus selection was used.\n",
    );

    let next = assert_success(&run_command(&["next"], project.root()));
    assert!(
        next.ends_with(warning),
        "warning missing from next:\n{next}"
    );

    let explain = assert_success(&run_command(&["explain", "add"], project.root()));
    assert!(
        explain.ends_with(warning),
        "warning missing from explain:\n{explain}"
    );
}
