# Contributing to Proof Lantern

Proof Lantern is an experimental public preview. Small issues, focused pull
requests, beginner observations, and examples of confusing status language are
especially useful.

Before starting a broad feature, open an issue so the product model can stay
small: the user journey is the map, while files and tests are evidence attached
to it.

## Development setup

Install Rust 1.88 or newer, clone the repository, then run:

```sh
cargo run -- demo
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
git diff --check
```

Changes to terminal layout should include an updated snapshot or rendered proof
when the visible output intentionally changes. Do not treat static code presence
as runtime proof, infer missing implementation from absent evidence, or rewrite
human-owned project intent automatically.

By contributing, you agree that your contribution may be licensed under either
the MIT License or Apache License 2.0, at the user's option.
