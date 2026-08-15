use std::path::PathBuf;

use proof_lantern::{App, evaluate, load_project, ui};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn recipe_box_renders_the_promise_broken_path_and_keystone() {
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
            screen.contains("KEYSTONE GAP"),
            "{width}x{height}: {screen}"
        );
        assert!(
            screen.contains("PROOF NEEDED"),
            "{width}x{height}: {screen}"
        );
    }
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

    let wide = render_screen(140, 40, false, "save");
    assert!(wide.contains("INSPECTOR"), "{wide}");
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

fn render_screen(width: u16, height: u16, inspector: bool, selected: &str) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal should initialize");
    let mut app = recipe_box_app();
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
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/recipe_box/.proof-lantern");
    let (spec, observations) =
        load_project(root.join("project.yml"), root.join("observations.json")).unwrap();
    App::new(evaluate(spec, observations).unwrap())
}
