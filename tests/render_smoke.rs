use std::path::PathBuf;

use proof_lantern::{
    App, Claim, EvidenceFact, EvidenceLocation, Freshness, evaluate, load_demo, load_project, ui,
};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn recipe_box_renders_the_promise_broken_path_and_current_focus() {
    for (width, height) in [(100, 30), (140, 40)] {
        let screen = render_screen(width, height, false, "reopen");
        assert!(
            screen.contains("PROOF LANTERN"),
            "{width}x{height}: {screen}"
        );
        assert!(
            screen.contains(
                "SAVE A RECIPE AND FIND IT AGAIN TOMORROW"
                    .to_lowercase()
                    .as_str()
            ) || screen.contains("Save a recipe and find it again tomorrow"),
            "{width}x{height}: {screen}"
        );
        assert!(screen.contains("✓ ADD"), "{width}x{height}: {screen}");
        assert!(screen.contains("◐ SAVE"), "{width}x{height}: {screen}");
        assert!(screen.contains("╳ REOPEN"), "{width}x{height}: {screen}");
        assert!(screen.contains("? FIND"), "{width}x{height}: {screen}");
        assert!(
            screen.contains("━━╸"),
            "missing path break absent at {width}x{height}: {screen}"
        );
        assert!(
            screen.contains("┄┄┄"),
            "downstream dashed path absent at {width}x{height}: {screen}"
        );
        assert!(
            screen.contains("LOCAL DATABASE"),
            "{width}x{height}: {screen}"
        );
        assert!(
            screen.contains("JOURNEY BREAK"),
            "{width}x{height}: {screen}"
        );
        assert!(
            screen.contains("Required implementation is recorded absent."),
            "{width}x{height}: {screen}"
        );
        assert!(
            screen.contains("Downstream unresolved: Find a saved recipe."),
            "downstream focus was clipped at {width}x{height}: {screen}"
        );
        assert!(
            !screen.contains("Find a save…"),
            "{width}x{height}: {screen}"
        );
        assert!(
            !screen.contains("The core journey stops here"),
            "generic break language leaked at {width}x{height}: {screen}"
        );
        assert!(
            screen.contains("PROOF NEEDED"),
            "{width}x{height}: {screen}"
        );
        assert!(screen.contains("ID  reopen"), "{width}x{height}: {screen}");
        assert!(
            screen.contains("proof-lantern explain reopen"),
            "{width}x{height}: {screen}"
        );
    }
}

#[test]
fn compact_focus_panel_distinguishes_unproven_from_unknown() {
    let built = render_app_screen(100, 30, false, "save", pinned_recipe_box_app("save"));
    assert!(built.contains("NEEDS PROOF"), "{built}");
    assert!(
        built.contains("Implementation evidence exists, but no current passing proof is recorded."),
        "{built}"
    );
    assert!(built.contains("PROOF NEEDED"), "{built}");
    assert!(!built.contains("JOURNEY BREAK"), "{built}");

    let unknown = render_app_screen(100, 30, false, "find", pinned_recipe_box_app("find"));
    assert!(unknown.contains("NEEDS EVIDENCE"), "{unknown}");
    assert!(
        unknown.contains(
            "No current technical evidence establishes whether this capability exists or works."
        ),
        "{unknown}"
    );
    assert!(unknown.contains("NEXT CHECK"), "{unknown}");
    assert!(!unknown.contains("JOURNEY BREAK"), "{unknown}");
}

#[test]
fn project_warnings_are_visible_without_replacing_the_current_focus() {
    let screen = render_app_screen(100, 30, false, "reopen", pinned_recipe_box_app("add"));
    assert!(screen.contains("JOURNEY BREAK"), "{screen}");
    assert!(screen.contains("Pinned focus"), "{screen}");
    assert!(screen.contains("already proven"), "{screen}");
}

#[test]
fn compact_inspector_and_wide_layout_expose_only_the_needed_sections() {
    let compact = render_screen(100, 30, true, "reopen");
    for label in ["WHY", "EVIDENCE", "PROOF NEEDED"] {
        assert!(
            compact.contains(label),
            "compact inspector lost {label}: {compact}"
        );
    }
    assert!(compact.contains("ACCEPTED CORE JOURNEY"), "{compact}");
    assert!(compact.contains("ID  reopen"), "{compact}");
    assert!(
        compact.contains("proof-lantern explain reopen"),
        "{compact}"
    );
    assert_eq!(
        compact.matches("◇ ACCEPTED").count(),
        1,
        "accepted badge should appear only in the compact inspector: {compact}"
    );

    let wide = render_screen(140, 40, false, "save");
    assert!(wide.contains("INSPECTOR"), "{wide}");
    assert_eq!(
        wide.matches("◇ ACCEPTED").count(),
        1,
        "accepted badge should appear only in the wide inspector: {wide}"
    );
    assert!(wide.contains("BUILT / UNPROVEN"), "{wide}");
    assert!(wide.contains("ID  save"), "{wide}");
    assert!(wide.contains("proof-lantern explain save"), "{wide}");
    assert!(wide.contains("proof-lantern explain reopen"), "{wide}");
    assert!(wide.contains("Local save code appears to"), "{wide}");
    assert!(wide.contains("exist."), "{wide}");
    assert!(wide.contains("Save a recipe, close"), "{wide}");
    assert!(wide.contains("the record persists"), "{wide}");
    assert!(
        !wide.contains("STATIC PATH"),
        "graph language leaked into Proof Lantern: {wide}"
    );
    assert!(
        !wide.contains("CONFIDENCE"),
        "confidence score leaked into Proof Lantern: {wide}"
    );
}

#[test]
fn compact_inspector_reserves_proof_space_when_evidence_is_long() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let (mut spec, observations) = load_project(root).unwrap();
    let reopen = spec
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == "reopen")
        .unwrap();
    for (index, summary) in [
        "An earlier reopen experiment passed before the storage format changed.",
        "A second historical check captured a longer evidence trail for review.",
    ]
    .into_iter()
    .enumerate()
    .rev()
    {
        reopen.manual_evidence.insert(
            0,
            EvidenceFact {
                claim: Claim::VerificationPassed,
                freshness: Freshness::Stale,
                summary: summary.into(),
                location: Some(EvidenceLocation {
                    path: "src/storage.rs".into(),
                    line_start: Some(index as u32 + 1),
                    line_end: Some(index as u32 + 2),
                }),
            },
        );
    }
    let app = App::new(evaluate(spec, observations).unwrap()).with_project_command_hints();
    let compact = render_app_screen(100, 30, true, "reopen", app);

    assert!(compact.contains("PROOF NEEDED"), "{compact}");
    assert!(compact.contains("Close app, reopen it"), "{compact}");
    assert!(
        compact.contains("The builder confirmed that no reopen flow has been implemented."),
        "{compact}"
    );
    assert!(compact.contains("(+2 more)"), "{compact}");
    assert!(
        compact.contains("proof-lantern explain reopen"),
        "{compact}"
    );
}

#[test]
fn compact_inspector_pairs_pass_and_fail_before_duplicate_history() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let (mut spec, observations) = load_project(root).unwrap();
    let add = spec
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == "add")
        .unwrap();
    add.manual_evidence.extend([
        EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "A duplicate current add check passed.".into(),
            location: None,
        },
        EvidenceFact {
            claim: Claim::VerificationFailed,
            freshness: Freshness::Current,
            summary: "The current add check failed.".into(),
            location: None,
        },
    ]);
    let app = App::new(evaluate(spec, observations).unwrap()).with_project_command_hints();
    let compact = render_app_screen(100, 30, true, "add", app);

    assert!(
        compact.contains("CURRENT CONFLICT  PASSED ↔ FAILED"),
        "{compact}"
    );
    assert!(compact.contains("PROOF NEEDED"), "{compact}");
}

#[test]
fn compact_inspector_pairs_passing_and_missing_evidence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let (mut spec, observations) = load_project(root).unwrap();
    let reopen = spec
        .capabilities
        .iter_mut()
        .find(|capability| capability.id == "reopen")
        .unwrap();
    reopen.manual_evidence.extend([
        EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "A current reopen check passed.".into(),
            location: None,
        },
        EvidenceFact {
            claim: Claim::VerificationPassed,
            freshness: Freshness::Current,
            summary: "A duplicate current reopen check passed.".into(),
            location: None,
        },
    ]);
    let app = App::new(evaluate(spec, observations).unwrap()).with_project_command_hints();
    let compact = render_app_screen(100, 30, true, "reopen", app);

    assert!(
        compact.contains("CURRENT CONFLICT  PASSED ↔ MISSING"),
        "{compact}"
    );
    assert!(compact.contains("PROOF NEEDED"), "{compact}");
}

#[test]
fn synthetic_demo_shows_ids_without_project_commands() {
    let (spec, observations) = load_demo().unwrap();
    let demo = App::new(evaluate(spec, observations).unwrap());
    let screen = render_app_screen(100, 30, false, "reopen", demo);

    assert!(screen.contains("ID  reopen"), "{screen}");
    assert!(!screen.contains("proof-lantern explain"), "{screen}");
}

#[test]
fn fixed_size_layouts_match_reviewed_goldens() {
    insta::assert_snapshot!("recipe_box_100x30", render_screen(100, 30, false, "reopen"));
    insta::assert_snapshot!(
        "recipe_box_reopen_inspector_100x30",
        render_screen(100, 30, true, "reopen")
    );
    insta::assert_snapshot!(
        "recipe_box_save_140x40",
        render_screen(140, 40, false, "save")
    );
}

#[test]
fn undersized_terminal_gets_an_explicit_bail_screen() {
    let screen = render_screen(80, 24, false, "reopen");
    assert!(
        screen.contains("PROOF LANTERN // DISPLAY LIMIT"),
        "{screen}"
    );
    assert!(screen.contains("CURRENT  080 × 24"), "{screen}");
    assert!(screen.contains("REQUIRED 100 × 30"), "{screen}");
}

#[test]
fn repository_self_map_renders_a_complete_core_journey() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let screen = render_app_screen(100, 30, false, "report-keystone", project_app(root));
    assert!(
        screen.contains("PROOF LANTERN // PROOF LANTERN"),
        "{screen}"
    );
    assert!(screen.contains("✓ NEXT"), "{screen}");
    assert!(!screen.contains("BUILT / UNPROVEN"), "{screen}");
    assert!(screen.contains("CORE JOURNEY PROVEN"), "{screen}");
    assert!(
        screen.contains("All accepted core capabilities have current recorded proof."),
        "{screen}"
    );
    assert!(!screen.contains("KEYSTONE GAP"), "{screen}");
}

fn render_screen(width: u16, height: u16, inspector: bool, selected: &str) -> String {
    render_app_screen(width, height, inspector, selected, recipe_box_app())
}

fn render_app_screen(
    width: u16,
    height: u16,
    inspector: bool,
    selected: &str,
    mut app: App,
) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let _ = app.select_id(selected);
    if inspector {
        app.toggle_inspector();
    }
    terminal
        .draw(|frame| ui::render(frame, &app))
        .expect("frame should render");

    terminal
        .backend()
        .buffer()
        .content()
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn recipe_box_app() -> App {
    project_app(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box"))
}

fn pinned_recipe_box_app(capability_id: &str) -> App {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box");
    let (mut spec, observations) = load_project(root).unwrap();
    spec.project.pinned_keystone = Some(capability_id.into());
    App::new(evaluate(spec, observations).unwrap()).with_project_command_hints()
}

fn project_app(root: PathBuf) -> App {
    let (spec, observations) = load_project(root).unwrap();
    App::new(evaluate(spec, observations).unwrap()).with_project_command_hints()
}
