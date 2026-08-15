# Proof Lantern

Proof Lantern is a local-first terminal prototype that shows what a project's
core user journey has accepted, built, proven, left missing, or not yet
understood. It then names one deterministic current focus and gives an honest,
state-sensitive explanation of what to check next.

![Proof Lantern mapping itself](proof/proof-lantern-self-100x30.png)

## Try the prototype

Run Proof Lantern in this checkout to open its evidence-backed self-map:

```sh
cargo run
```

The built-in Recipe Box fixture remains a compact demonstration of missing,
unknown, built, and proven states:

```sh
cargo run -- demo
```

Inside the TUI:

- `←` / `→` or `h` / `l`: select a capability
- `e` or `Enter`: open or close the compact inspector
- `g`: return to the current focus
- `q` or `Ctrl-C`: exit and restore the terminal

Plain terminal commands expose the same evaluated model:

```sh
cargo run -- next .
cargo run -- explain report-keystone .
cargo run -- next fixtures/recipe_box
cargo run -- explain save fixtures/recipe_box
```

To open another authored project, pass its root directory. If it does not yet
contain `.proof-lantern/project.yml`, Proof Lantern explains the expected path
and points to the built-in demo without creating or overwriting anything.

The TUI requires at least 100×30 cells. At 128×36 and above, the inspector
remains visible beside the journey.

## Evidence boundary

Proof Lantern deliberately separates authority:

- `.proof-lantern/project.yml` is human-owned. It contains the promise,
  accepted capabilities, journey order, proof requirements, notes, and manual
  evidence.
- `.proof-lantern/observations.json` is replaceable machine evidence. Static
  scans may establish only that implementation appears present. Imported test
  results may record passing or failing verification. In this pre-release v1
  schema, every machine observation must declare a syntactically valid,
  repo-relative evidence location. The evaluator does not yet verify that the
  referenced file exists.
- Display states and the current focus are derived. They are never stored as
  editable progress labels.

Missing machine evidence produces `UNKNOWN`, not `MISSING`. Conflicting current
evidence remains visibly `CONFLICTING` rather than being guessed away.
Only explicit current absence produces a `JOURNEY BREAK`; unproven, unknown,
failed, and conflicting states receive their own narrower focus language. A
fully proven core journey is reported positively as `CORE JOURNEY PROVEN`.

## What this prototype proves

- A four-node accepted journey and one supporting branch remain readable at
  100×30 and 140×40.
- Proof Lantern can load and render its own five-node journey from real local
  source and test evidence.
- A missing core capability physically interrupts the path.
- The inspector shows `WHY`, `EVIDENCE`, and `PROOF NEEDED`.
- Current-focus selection is deterministic and excludes supporting and optional
  capabilities.
- Normal exit, returned errors, and panics restore the terminal in PTY tests.

It does not yet generate journeys, refresh repository evidence, execute project
code, or edit project intent. The Recipe Box evidence is synthetic but points
to inspectable fixture source and recorded artifacts.

For the first private dogfood, prepare maps with roughly three to five accepted
core capabilities in one linear journey. That is a facilitation constraint, not
a schema restriction. Larger or dependency-shaped maps may load, but this
prototype still renders core capabilities as one ordered line; branching journey
visualization remains future work.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
git diff --check
```

Rendered proof is generated through the same Ratatui `TestBackend` used by the
snapshot tests:

```sh
cargo run --example render_proof -- proof/recipe-box-100x30.svg 100 30 reopen
cargo run --example render_proof -- proof/recipe-box-save-140x40.svg 140 40 save
cargo run --example render_proof -- proof/proof-lantern-self-100x30.svg 100 30 report-keystone closed .
```

The original product kickoff remains in
`BUILD_MAP_CODEX_KICKOFF.md`; “build map” is now the generic visualization,
while Proof Lantern is the product name.
