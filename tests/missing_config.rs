use std::{fs, path::Path, process::Command, time::SystemTime};

#[test]
fn missing_project_config_points_beginners_to_the_demo_and_expected_path() {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "proof-lantern-missing-config-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir(&root).expect("isolated test directory should be created");

    let output = Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .current_dir(&root)
        .output()
        .expect("Proof Lantern should launch");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    let expected_path = Path::new(".")
        .join(".proof-lantern/project.yml")
        .display()
        .to_string();
    assert!(
        stderr.contains(&format!(
            "no Proof Lantern project found at {expected_path}"
        )),
        "{stderr}"
    );
    assert!(stderr.contains("`proof-lantern demo`"), "{stderr}");
    assert!(
        stderr.contains(&format!("create {expected_path} for this project")),
        "{stderr}"
    );
    assert!(!root.join(".proof-lantern").exists());
    fs::remove_dir(&root).expect("isolated test directory should be removed");
}
