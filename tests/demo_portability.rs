use proof_lantern::{CurrentFocus, evaluate, load_demo};

#[test]
fn embedded_demo_parses_and_evaluates() {
    let (spec, observations) = load_demo().expect("embedded Recipe Box data should parse");
    let project = evaluate(spec, observations).expect("embedded Recipe Box should evaluate");

    assert_eq!(project.project.name, "Recipe Box — Synthetic Demo");
    let CurrentFocus::Capability { capability, .. } = project.current_focus() else {
        panic!("Recipe Box should retain its unresolved focus");
    };
    assert_eq!(capability.intent.id, "reopen");
}
