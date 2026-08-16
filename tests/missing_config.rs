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
        stderr.contains(&format!("no Proof Lantern map found at {expected_path}")),
        "{stderr}"
    );
    assert!(stderr.contains("`proof-lantern init .`"), "{stderr}");
    assert!(stderr.contains("`proof-lantern demo`"), "{stderr}");
    assert!(!root.join(".proof-lantern").exists());

    let explicit_root = root.join("another project");
    fs::create_dir(&explicit_root).expect("explicit project directory should be created");
    let explicit = Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .arg(&explicit_root)
        .output()
        .expect("Proof Lantern should launch for an explicit path");
    assert!(!explicit.status.success());
    let explicit_stderr =
        String::from_utf8(explicit.stderr).expect("explicit-path error should be UTF-8");
    assert!(
        explicit_stderr.contains(&format!("project directory ({})", explicit_root.display())),
        "{explicit_stderr}"
    );
    assert!(
        explicit_stderr.contains("`proof-lantern init .`"),
        "{explicit_stderr}"
    );
    assert!(!explicit_root.join(".proof-lantern").exists());

    fs::remove_dir_all(&root).expect("isolated test directory should be removed");
}
