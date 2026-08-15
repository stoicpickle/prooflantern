# Rendered proof

These PNGs are visual-review artifacts generated from Ratatui's deterministic
`TestBackend` buffer:

- `recipe-box-100x30.png`: compact default map with the REOPEN gap selected;
- `recipe-box-reopen-inspector-100x30.png`: compact evidence drawer;
- `recipe-box-save-140x40.png`: wide journey and inspector with SAVE selected.
- `proof-lantern-self-100x30.png`: Proof Lantern's real five-node self-map with
  its completed core journey and NEXT selected.

The SVG sources are reproducible and ignored by Git. On macOS, convert a render
without changing its geometry with:

```sh
sips -s format png proof/recipe-box-100x30.svg \
  --out proof/recipe-box-100x30.png
```
