# Rendered proof

These PNGs are visual-review artifacts generated from Ratatui's deterministic
`TestBackend` buffer:

- `recipe-box-100x30.png`: compact default map with the REOPEN gap selected;
- `recipe-box-reopen-inspector-100x30.png`: compact evidence drawer;
- `recipe-box-save-140x40.png`: wide journey and inspector with SAVE selected.
- `proof-lantern-self-100x30.png`: Proof Lantern's real five-node self-map with
  its completed core journey and NEXT selected.

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
