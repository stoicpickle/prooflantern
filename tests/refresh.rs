use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use proof_lantern::app::RefreshStatus;
use proof_lantern::{
    App, Claim, CurrentFocus, DisplayState, evaluate, load_demo, load_project,
    record_manual_evidence_with_inputs,
};

const REFRESH_TIMEOUT: Duration = Duration::from_secs(3);

static NEXT_PROJECT_ID: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(label: &str, capabilities: &[(&str, u16)]) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should follow the Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "proof-lantern-refresh-{label}-{}-{nonce}-{}",
            std::process::id(),
            NEXT_PROJECT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".proof-lantern"))
            .expect("isolated project directory should be created");
        write_project(&root, capabilities);
        Self { root }
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_project(root: &Path, capabilities: &[(&str, u16)]) {
    let mut text = String::from(
        "schema_version: 1\nproject:\n  name: Refresh Test\n  promise: Keep the map current.\ncapabilities:\n",
    );
    for (id, order) in capabilities {
        text.push_str(&format!(
            "  - id: {id}\n    label: {id}\n    role:\n      kind: core\n      order: {order}\n    proof_needed: Confirm {id} works.\n"
        ));
    }
    fs::write(root.join(".proof-lantern/project.yml"), text)
        .expect("test project map should be writable");
}

fn app_for(project: &TempProject) -> App {
    let (spec, observations) = load_project(project.root()).expect("test map should load");
    App::new(evaluate(spec, observations).expect("test map should evaluate"))
        .with_project_root(project.root())
        .expect("test project root should be usable")
}

fn wait_for_refresh(app: &mut App) {
    let deadline = Instant::now() + REFRESH_TIMEOUT;
    loop {
        app.poll_refresh();
        if !matches!(app.refresh_status(), RefreshStatus::Refreshing) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "refresh did not settle within {REFRESH_TIMEOUT:?}"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn assert_updated(app: &App, expected_rechecks: usize) {
    let RefreshStatus::Updated { needs_recheck } = app.refresh_status() else {
        panic!("refresh should have produced an Updated status");
    };
    assert_eq!(*needs_recheck, expected_rechecks);
}

#[test]
fn demo_app_without_a_project_root_cannot_refresh() {
    let (spec, observations) = load_demo().expect("embedded demo should load");
    let mut app = App::new(evaluate(spec, observations).expect("embedded demo should evaluate"));
    assert!(!app.can_refresh());
    assert!(!app.request_refresh());
    assert!(matches!(app.refresh_status(), RefreshStatus::Idle));
}

#[test]
fn manual_record_is_visible_after_an_in_tui_refresh() {
    let project = TempProject::new("record", &[("start", 1), ("outcome", 2)]);
    fs::write(project.root().join("tracked.txt"), "before")
        .expect("tracked input should be writable");
    let mut app = app_for(&project);
    assert_eq!(
        app.project().capability("start").unwrap().display,
        DisplayState::Unknown
    );

    record_manual_evidence_with_inputs(
        project.root(),
        "start",
        Claim::VerificationPassed,
        "The starting check passed.",
        &[String::from("tracked.txt")],
    )
    .expect("manual evidence should be recorded");

    assert!(app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::NONE
    ))));
    wait_for_refresh(&mut app);
    assert_updated(&app, 0);
    assert_eq!(
        app.project().capability("start").unwrap().display,
        DisplayState::Proven
    );
}

#[test]
fn changed_and_deleted_receipt_inputs_need_recheck_after_refresh() {
    let project = TempProject::new("receipt", &[("start", 1)]);
    let tracked = project.root().join("tracked.txt");
    fs::write(&tracked, "before").expect("tracked input should be writable");
    record_manual_evidence_with_inputs(
        project.root(),
        "start",
        Claim::VerificationPassed,
        "The tracked check passed.",
        &[String::from("tracked.txt")],
    )
    .expect("manual evidence should be recorded");
    let mut app = app_for(&project);
    assert!(!app.project().capability("start").unwrap().needs_recheck());

    fs::write(&tracked, "changed").expect("tracked input should be changeable");
    assert!(app.request_refresh());
    wait_for_refresh(&mut app);
    assert_updated(&app, 1);
    assert!(app.project().capability("start").unwrap().needs_recheck());

    fs::write(&tracked, "before").expect("tracked input should be restorable");
    assert!(app.request_refresh());
    wait_for_refresh(&mut app);
    assert_updated(&app, 0);
    assert!(!app.project().capability("start").unwrap().needs_recheck());

    fs::remove_file(&tracked).expect("tracked input should be removable");
    assert!(app.request_refresh());
    wait_for_refresh(&mut app);
    assert_updated(&app, 1);
    assert!(app.project().capability("start").unwrap().needs_recheck());
}

#[test]
fn failed_refresh_keeps_snapshot_and_selection_until_a_valid_retry() {
    #[derive(Clone, Copy)]
    enum Failure {
        Yaml,
        Json,
        ManualJson,
        MissingMap,
        Evaluation,
    }

    for failure in [
        Failure::Yaml,
        Failure::Json,
        Failure::ManualJson,
        Failure::MissingMap,
        Failure::Evaluation,
    ] {
        let project = TempProject::new("failure", &[("alpha", 1), ("beta", 2), ("gamma", 3)]);
        let mut app = app_for(&project);
        assert!(app.project_command_hints());
        assert!(app.select_id("beta"));
        assert!(app.toggle_inspector());
        let old_project = app.project().clone();

        match failure {
            Failure::Yaml => fs::write(
                project.root().join(".proof-lantern/project.yml"),
                "project: [",
            )
            .expect("malformed YAML should be writable"),
            Failure::Json => fs::write(
                project.root().join(".proof-lantern/observations.json"),
                "{",
            )
            .expect("malformed JSON should be writable"),
            Failure::ManualJson => fs::write(project.root().join(".proof-lantern/manual-evidence.json"), "{").unwrap(),
            Failure::MissingMap => fs::remove_file(project.root().join(".proof-lantern/project.yml")).unwrap(),
            Failure::Evaluation => fs::write(
                project.root().join(".proof-lantern/project.yml"),
                "schema_version: 1\nproject:\n  name: Invalid\n  promise: Invalid\ncapabilities: []\n",
            )
            .expect("invalid evaluated model should be writable"),
        }

        assert!(app.request_refresh());
        wait_for_refresh(&mut app);
        match app.refresh_status() {
            RefreshStatus::Failed { message } => assert!(!message.is_empty()),
            _ => panic!("invalid refresh should report failure"),
        }
        assert_eq!(app.project(), &old_project);
        assert_eq!(app.selected().unwrap().intent.id, "beta");
        assert!(app.inspector_open());
        assert!(app.project_command_hints());

        if matches!(failure, Failure::Json) {
            fs::remove_file(project.root().join(".proof-lantern/observations.json"))
                .expect("malformed observations should be removable");
        }
        if matches!(failure, Failure::ManualJson) {
            fs::remove_file(project.root().join(".proof-lantern/manual-evidence.json")).unwrap();
        }
        write_project(project.root(), &[("alpha", 1), ("beta", 2), ("gamma", 3)]);
        assert!(app.request_refresh());
        wait_for_refresh(&mut app);
        assert_updated(&app, 0);
        assert_eq!(app.selected().unwrap().intent.id, "beta");
        assert!(app.inspector_open());
        assert!(app.project_command_hints());
    }
}

#[test]
fn refresh_retains_selection_by_id_and_falls_back_when_it_is_removed() {
    let project = TempProject::new("selection", &[("alpha", 1), ("beta", 2), ("gamma", 3)]);
    let mut app = app_for(&project);
    assert!(app.select_id("beta"));

    write_project(project.root(), &[("gamma", 1), ("alpha", 2), ("beta", 3)]);
    assert!(app.request_refresh());
    wait_for_refresh(&mut app);
    assert_updated(&app, 0);
    assert_eq!(app.selected().unwrap().intent.id, "beta");

    write_project(project.root(), &[("gamma", 1), ("alpha", 2)]);
    assert!(app.request_refresh());
    wait_for_refresh(&mut app);
    assert_updated(&app, 0);
    let expected = match app.project().current_focus() {
        CurrentFocus::Capability { capability, .. } => capability.intent.id.clone(),
        CurrentFocus::Complete { .. } => app
            .project()
            .capabilities
            .first()
            .expect("replacement map should have a capability")
            .intent
            .id
            .clone(),
    };
    assert_eq!(app.selected().unwrap().intent.id, expected);
}

#[test]
fn refresh_is_single_flight_and_selection_changes_during_pending_refresh_survive() {
    let project = TempProject::new("single-flight", &[("alpha", 1), ("beta", 2), ("gamma", 3)]);
    let mut app = app_for(&project);
    assert!(app.select_id("beta"));
    assert!(app.can_refresh());
    assert!(app.request_refresh());
    assert!(
        app.can_refresh(),
        "refresh remains available but repeated requests are ignored while busy"
    );
    assert!(
        !app.request_refresh(),
        "a pending refresh must not queue another worker"
    );
    assert!(matches!(app.refresh_status(), RefreshStatus::Refreshing));

    assert!(app.select_id("gamma"));
    wait_for_refresh(&mut app);
    assert_updated(&app, 0);
    assert_eq!(app.selected().unwrap().intent.id, "gamma");
    assert!(app.can_refresh());
}

#[cfg(unix)]
#[test]
fn refresh_stays_on_the_original_root_when_the_cli_symlink_changes() {
    let original = TempProject::new("original-root", &[("original", 1)]);
    let replacement = TempProject::new("other-root", &[("other", 1)]);
    let alias = original.root().join("project-alias");
    std::os::unix::fs::symlink(original.root(), &alias).unwrap();
    let (spec, observations) = load_project(&alias).unwrap();
    let mut app = App::new(evaluate(spec, observations).unwrap())
        .with_project_root(&alias)
        .unwrap();
    fs::remove_file(&alias).unwrap();
    std::os::unix::fs::symlink(replacement.root(), &alias).unwrap();
    assert!(app.request_refresh());
    wait_for_refresh(&mut app);
    assert_updated(&app, 0);
    assert!(app.project().capability("original").is_some());
    assert!(app.project().capability("other").is_none());
}
