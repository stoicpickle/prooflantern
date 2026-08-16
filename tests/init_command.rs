use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use proof_lantern::{CurrentFocus, DisplayState, FocusKind, evaluate, load_project};

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-init-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("isolated project directory should be created");
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

fn run_init(root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .arg("init")
        .arg(root)
        .output()
        .expect("Proof Lantern should launch")
}

#[test]
fn init_creates_an_honest_beginner_starter_without_machine_evidence() {
    let project = TempProject::new();
    let output = run_init(project.root());
    assert!(output.status.success(), "{:?}", output.status);
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);

    let stdout = String::from_utf8(output.stdout).expect("output should be UTF-8");
    assert!(stdout.contains("Created"), "{stdout}");
    assert!(
        stdout.contains("Seeing UNKNOWN at first is expected"),
        "{stdout}"
    );
    assert!(stdout.contains("`proof-lantern demo`"), "{stdout}");
    assert!(stdout.contains("docs/PROJECT_FORMAT.md"), "{stdout}");

    let project_file = project.root().join(".proof-lantern/project.yml");
    assert!(project_file.is_file());
    assert!(
        !project
            .root()
            .join(".proof-lantern/observations.json")
            .exists()
    );
    let authored = fs::read_to_string(&project_file).expect("starter map should be readable");
    assert!(authored.contains("Start with the experience, not the files."));
    assert!(authored.contains("New capabilities begin UNKNOWN."));

    let (spec, observations) = load_project(project.root()).expect("starter map should load");
    let evaluated = evaluate(spec, observations).expect("starter map should evaluate");
    assert_eq!(evaluated.core_capabilities().count(), 3);
    assert!(
        evaluated
            .core_capabilities()
            .all(|capability| capability.display == DisplayState::Unknown)
    );
    let CurrentFocus::Capability { kind, .. } = evaluated.current_focus() else {
        panic!("the starter should name its first unknown capability as the current focus");
    };
    assert_eq!(kind, FocusKind::NeedsEvidence);
}

#[test]
fn init_refuses_to_overwrite_an_existing_map() {
    let project = TempProject::new();
    let first = run_init(project.root());
    assert!(first.status.success());

    let project_file = project.root().join(".proof-lantern/project.yml");
    let before = fs::read(&project_file).expect("starter map should exist");
    let second = run_init(project.root());
    assert!(!second.status.success());
    let stderr = String::from_utf8(second.stderr).expect("error should be UTF-8");
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains("Nothing was changed"), "{stderr}");
    assert_eq!(
        fs::read(project_file).expect("existing map should remain readable"),
        before
    );
}

#[cfg(unix)]
#[test]
fn init_refuses_a_config_symlink_that_escapes_the_project() {
    use std::os::unix::fs::symlink;

    let project = TempProject::new();
    let outside = TempProject::new();
    symlink(outside.root(), project.root().join(".proof-lantern"))
        .expect("test config symlink should be created");

    let output = run_init(project.root());
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("error should be UTF-8");
    assert!(
        stderr.contains("resolves outside the project root"),
        "{stderr}"
    );
    assert!(!outside.root().join("project.yml").exists());
}
