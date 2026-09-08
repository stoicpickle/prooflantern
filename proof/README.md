# Rendered proof

These PNGs are visual-review artifacts generated from Ratatui's deterministic
`TestBackend` buffer:

- `recipe-box-100x30.png`: compact default map with the REOPEN gap selected;
- `recipe-box-reopen-inspector-100x30.png`: compact evidence drawer;
- `recipe-box-save-140x40.png`: wide journey and inspector with SAVE selected.
- `proof-lantern-self-100x30.png`: Proof Lantern's real five-node self-map with
  its completed core journey and NEXT selected.
- `receipt-needs-recheck-100x30.png`: a synthetic changed-input receipt, with
  the former passing result no longer counted as current proof.
- `receipt-needs-recheck-140x40.png`: the same case with its receipt history
  and current focus visible together.
- `guided-setup-100x30.png`: the first map saved through the real guided CLI
  with synthetic Recipe Box answers. All three steps are UNKNOWN; the drawer
  shows the first step's visible check. The matching `fixtures/guided_setup`
  intent is checked against actual interview output in `tests/guided_init.rs`.
- `refresh-rechecked-140x40.png`: a completed manual refresh of the synthetic
  receipt fixture, reporting one step needing recheck.
- `refresh-failed-100x30.png`: the guided fixture loaded successfully, then its
  isolated copy was made invalid before refresh. The outdated-snapshot warning
  stays visible while the selected second step and open inspector are retained.

The exporter accepts a final `refresh` argument. It pauses after the initial
load until Enter, then runs the real background refresh controller before
rendering. Use an isolated copy of a fixture if changing files during the pause;
the exporter never modifies the project. For example:

```sh
cargo run --example render_proof -- proof/refresh-rechecked-140x40.svg 140 40 reopen inspector fixtures/receipt_change refresh
```

The failure image uses the guided fixture copy with `schema_version: [invalid`
substituted during that pause, at 100×30 with `step-2` and `inspector` selected.
These deterministic buffer renders supplement the real PTY refresh/quit,
Ctrl-C, and worker-panic checks in `tests/terminal_restoration.rs`.

Reproduce the guided-map render:

```sh
cargo run --example render_proof -- proof/guided-setup-100x30.svg 100 30 step-1 inspector fixtures/guided_setup
```

Generate the changed-receipt SVGs from the committed fixture:

```sh
cargo run --example render_proof -- proof/receipt-needs-recheck-100x30.svg 100 30 reopen inspector fixtures/receipt_change
cargo run --example render_proof -- proof/receipt-needs-recheck-140x40.svg 140 40 reopen inspector fixtures/receipt_change
```

The SVG sources are reproducible and ignored by Git. Rasterize them with a
browser after its compositor has completed so every terminal cell is present.
For example, on macOS with Chrome installed:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless --hide-scrollbars --force-device-scale-factor=1 \
  --virtual-time-budget=2000 --run-all-compositor-stages-before-draw \
  --window-size=1000,570 \
  --screenshot="$PWD/proof/recipe-box-100x30.png" \
  "file://$PWD/proof/recipe-box-100x30.svg"
```

The exporter uses 10×19 pixels per terminal cell, so a 140×40 render uses a
1400×760 browser viewport.
