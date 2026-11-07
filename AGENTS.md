# Repository Guidelines

## Project Structure & Module Organization

Legacy is a headless simulation game built as a Rust 2024 workspace. `crates/game-*` contains feature owners and infrastructure. `game-economy` composes the simulation; `game-economy-player` provides the player view and `game-economy-cli` the text interface. Consult `crates/README.md` for responsibilities and dependencies.

`content/` holds runtime TOML definitions and scenarios. `docs/plan.md` and `docs/design/` describe the intended game, including unfinished mechanics.

## Build, Test, and Development Commands

Run commands from the repository root:

- `cargo build --workspace`: build all workspace packages.
- `cargo run -p game-economy-cli -- --player`: start the player interface.
- `cargo test -p game-lot-runtime`: test one feature during development.
- `cargo test --workspace`: run all workspace tests.
- `cargo fmt --all -- --check`: check Rust formatting.
- `cargo clippy --workspace --all-targets -- -D warnings`: lint all targets, treating warnings as errors.
- `cargo test -p game-economy concentrated_named_food_benchmark -- --ignored --nocapture`: run the concentrated food benchmark.

## Coding Style & Naming Conventions

Use standard rustfmt formatting and four-space indentation. Use `snake_case` for modules, functions, and tests; `PascalCase` for types; and `SCREAMING_SNAKE_CASE` for constants. Crate names follow `game-<mechanic>`.

Keep state, validation, requests, and rules with their owning feature. Communicate through receiver-owned typed requests and events. Preserve acyclic dependencies and mutate only owner-managed resources.

## Testing Guidelines

Use Rust's built-in `#[test]` framework. Place integration tests in each crate's `tests/`, with descriptive behavior names such as `stale_stock_causes_failure_learning_and_backoff`. Share fixtures through `tests/support/mod.rs` where useful.

Feature tests compose only required owners. Cover changed behavior, rollback, and save/load consistency where relevant. Use economy tests for composed behavior. No numeric coverage threshold is configured.

## Commit & Pull Request Guidelines

History generally uses short, imperative subject lines such as “Use typed pending work and compact transaction logs.” Follow that style and keep commits focused.

PR descriptions should explain the behavior change and list validation commands and results. Link relevant issues or design documents, and update affected crate or content documentation when interfaces or schemas change.
