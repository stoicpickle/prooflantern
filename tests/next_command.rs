use std::{path::PathBuf, process::Command};

#[test]
fn next_command_reports_the_gap_impact_and_proof_needed() {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let output = Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .arg("next")
        .arg(project_root)
        .output()
        .expect("Proof Lantern should launch");

    assert!(output.status.success(), "{:?}", output.status);
    assert!(
        output.stderr.is_empty(),
        "unexpected stderr: {:?}",
        output.stderr
    );
    let stdout = String::from_utf8(output.stdout).expect("next output should be UTF-8");
    assert_eq!(
        stdout,
        concat!(
            "KEYSTONE GAP\n",
            "╳ Reopen saved recipes — MISSING\n",
            "The core journey stops here. Downstream: Find a saved recipe.\n",
            "Proof needed: Close app, reopen it, and confirm the saved recipe appears.\n",
        )
    );
}
