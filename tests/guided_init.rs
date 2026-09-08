use proof_lantern::{DisplayState, evaluate, initialize_guided, initialize_project, load_project};
use std::{
    fs,
    io::{self, Cursor, Write},
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);
struct TempProject(PathBuf);
impl TempProject {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-guided-{}-{nonce}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn config(&self) -> PathBuf {
        self.0.join(".proof-lantern")
    }
    fn run(&self, input: &str) -> (bool, String) {
        let mut output = Vec::new();
        let saved = initialize_guided(&self.0, &mut Cursor::new(input), &mut output)
            .unwrap()
            .is_some();
        (saved, String::from_utf8(output).unwrap())
    }
}
impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const ANSWERS: &str = "Recipe Box\nSave a recipe and find it tomorrow.\n3\nAdd a recipe\nThe entered recipe appears.\nSave the recipe\nThe app reports it saved.\nReopen the recipe\nClose and reopen; the recipe remains.\n";

#[test]
fn saves_only_confirmed_intent_and_all_steps_start_unknown() {
    let project = TempProject::new();
    let (saved, output) = project.run(&format!("{ANSWERS}save\n"));
    assert!(saved);
    assert!(output.contains("YOUR JOURNEY / PREVIEW (not saved)"));
    assert!(output.contains("[?] Add a recipe  (step-1 / UNKNOWN)"));
    assert!(output.contains("    |\n    v\n[?] Save the recipe"));
    assert!(output.contains("These checks have not been run"));
    assert_eq!(fs::read_dir(project.config()).unwrap().count(), 1);
    let (spec, observations) = load_project(&project.0).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/guided_setup");
    assert_eq!(
        spec,
        load_project(fixture).unwrap().0,
        "render fixture must match the actual guided output"
    );
    assert_eq!(spec.project.promise, "Save a recipe and find it tomorrow.");
    assert!(spec.project.pinned_keystone.is_none());
    for (index, step) in spec.capabilities.iter().enumerate() {
        assert_eq!(step.id, format!("step-{}", index + 1));
        assert_eq!(step.role.core_order(), Some(index as u16 + 1));
        assert_eq!(
            step.depends_on,
            if index == 0 {
                vec![]
            } else {
                vec![format!("step-{index}")]
            }
        );
        assert!(step.manual_evidence.is_empty());
    }
    assert!(
        evaluate(spec, observations)
            .unwrap()
            .capabilities
            .iter()
            .all(|c| c.display == DisplayState::Unknown)
    );
}

#[test]
fn eof_or_cancel_at_every_prompt_never_creates_a_directory() {
    let lines: Vec<_> = ANSWERS.split_inclusive('\n').collect();
    for index in 0..=lines.len() {
        for suffix in ["", ":cancel\n"] {
            let project = TempProject::new();
            let (saved, output) = project.run(&format!("{}{suffix}", lines[..index].concat()));
            assert!(!saved, "prompt {index}");
            assert!(output.contains("Nothing was saved"));
            assert!(!project.config().exists(), "prompt {index}");
        }
    }
}

#[test]
fn save_requires_a_complete_explicit_confirmation() {
    for choice in ["\n", "cancel\n", "save", "y\n", "save\x1b\n"] {
        let project = TempProject::new();
        assert!(!project.run(&format!("{ANSWERS}{choice}")).0);
        assert!(!project.config().exists());
    }
}

#[test]
fn edit_keeps_defaults_and_can_change_the_promise_and_step_count() {
    let project = TempProject::new();
    let initial = ANSWERS.replacen("\n3\n", "\n5\n", 1);
    let input = format!(
        "{initial}Find a recipe\nSearch finds it.\nShare a recipe\nThe recipient can read it.\nedit\n\nSave and reopen recipes.\n3\n\n\n\n\n\n\nsave\n"
    );
    let (saved, output) = project.run(&input);
    assert!(saved);
    assert_eq!(output.matches("YOUR JOURNEY / PREVIEW").count(), 2);
    let (spec, _) = load_project(&project.0).unwrap();
    assert_eq!(spec.project.name, "Recipe Box");
    assert_eq!(spec.project.promise, "Save and reopen recipes.");
    assert_eq!(spec.capabilities.len(), 3);
    assert_eq!(spec.capabilities[2].label, "Reopen the recipe");
}

#[test]
fn five_step_map_preserves_unicode_and_yaml_punctuation() {
    let project = TempProject::new();
    let input = "Recettes: \"家\" #1\nSave: \"今日\" # not a comment\n5\nAdd: 家\nCheck #1\nSave\nCheck: yes\nReopen\nSee \"家\"\nFind\nCheck: null\nShare\nCheck: [ok]\nsave\n";
    assert!(project.run(input).0);
    let (spec, _) = load_project(&project.0).unwrap();
    assert_eq!(spec.project.name, "Recettes: \"家\" #1");
    assert_eq!(spec.project.promise, "Save: \"今日\" # not a comment");
    assert_eq!(spec.capabilities.len(), 5);
    assert_eq!(spec.capabilities[4].proof_needed, "Check: [ok]");
}

#[test]
fn invalid_answers_are_reprompted_without_saving_them() {
    let project = TempProject::new();
    let input = format!(
        "{}\n\x1b[31mBad\n{}",
        "x".repeat(5000),
        ANSWERS.replacen("\n3\n", "\n2\n6\nx\n3\n", 1)
    );
    let (saved, output) = project.run(&format!("{input}save\n"));
    assert!(saved);
    assert!(output.contains("Answer too long"));
    assert!(output.contains("without control characters"));
    assert!(output.contains("Choose 3, 4, or 5"));
    let (spec, _) = load_project(&project.0).unwrap();
    assert_eq!(spec.project.name, "Recipe Box");
}

#[test]
fn existing_map_and_orphan_evidence_fail_before_questions() {
    for name in ["project.yml", "manual-evidence.json", "observations.json"] {
        let project = TempProject::new();
        fs::create_dir(project.config()).unwrap();
        let file = project.config().join(name);
        fs::write(&file, "keep me").unwrap();
        let mut output = Vec::new();
        let result = initialize_guided(&project.0, &mut Cursor::new(ANSWERS), &mut output);
        assert!(result.is_err());
        assert!(output.is_empty());
        assert_eq!(fs::read_to_string(file).unwrap(), "keep me");
        assert_eq!(fs::read_dir(project.config()).unwrap().count(), 1);
    }
}

#[test]
fn redirected_guided_cli_fails_without_writing_but_plain_init_still_works() {
    let project = TempProject::new();
    let output = Command::new(env!("CARGO_BIN_EXE_proof-lantern"))
        .args(["init", "--guided"])
        .arg(&project.0)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("use `proof-lantern init .` for a noninteractive starter")
    );
    assert!(output.stdout.is_empty());
    assert!(!project.config().exists());
    initialize_project(&project.0).unwrap();
    assert!(project.config().join("project.yml").exists());
}

#[test]
fn a_file_created_during_the_preview_is_not_overwritten() {
    struct ConcurrentWriter<'a> {
        project: &'a TempProject,
        output: Vec<u8>,
        inserted: bool,
    }
    impl Write for ConcurrentWriter<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.output.extend_from_slice(bytes);
            if !self.inserted
                && String::from_utf8_lossy(&self.output).contains("YOUR JOURNEY / PREVIEW")
            {
                fs::create_dir(self.project.config())?;
                fs::write(
                    self.project.config().join("project.yml"),
                    "another writer's map",
                )?;
                self.inserted = true;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let project = TempProject::new();
    let mut output = ConcurrentWriter {
        project: &project,
        output: vec![],
        inserted: false,
    };
    let error = initialize_guided(
        &project.0,
        &mut Cursor::new(format!("{ANSWERS}save\n")),
        &mut output,
    )
    .unwrap_err();
    assert!(error.to_string().contains("already exists"));
    assert_eq!(
        fs::read_to_string(project.config().join("project.yml")).unwrap(),
        "another writer's map"
    );
}

#[cfg(unix)]
#[test]
fn guided_setup_refuses_escaping_config_symlinks_without_prompting() {
    let project = TempProject::new();
    let outside = TempProject::new();
    std::os::unix::fs::symlink(&outside.0, project.config()).unwrap();
    let mut output = Vec::new();
    let error = initialize_guided(&project.0, &mut Cursor::new(ANSWERS), &mut output).unwrap_err();
    assert!(error.to_string().contains("outside the project root"));
    assert!(output.is_empty());
    assert!(!outside.0.join("project.yml").exists());
}
