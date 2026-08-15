use std::path::PathBuf;

use proof_lantern::{App, evaluate, load_project, ui};
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
fn compact_inspector_and_wide_layout_expose_only_the_needed_sections() {
    let compact = render_screen(100, 30, true, "reopen");
    for label in ["WHY", "EVIDENCE", "PROOF NEEDED"] {
        assert!(
            compact.contains(label),
            "compact inspector lost {label}: {compact}"
        );
    }
    assert!(compact.contains("ACCEPTED CORE JOURNEY"), "{compact}");
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box/.proof-lantern");
    let (mut spec, observations) =
        load_project(root.join("project.yml"), root.join("observations.json")).unwrap();
    spec.project.pinned_keystone = Some(capability_id.into());
    App::new(evaluate(spec, observations).unwrap())
}

fn project_app(root: PathBuf) -> App {
    let root = root.join(".proof-lantern");
    let (spec, observations) =
        load_project(root.join("project.yml"), root.join("observations.json")).unwrap();
    App::new(evaluate(spec, observations).unwrap())
}
