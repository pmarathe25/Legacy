# Repository Guidelines

## Project Structure & Module Organization

Legacy is a headless simulation game built as a Rust 2024 workspace. `crates/game-*` contains feature owners and infrastructure, each with `src/`, integration tests in `tests/`, and a README. `game-sim` hosts the simulation; `game-assembly` composes plugins; `game-cli` provides the text interface. Consult `crates/README.md` for responsibilities and dependencies.

`content/` holds runtime TOML definitions, scenarios, and rule profiles. `docs/plan.md` and `docs/design/` describe the intended game, including unfinished mechanics. `tools/performance/` contains benchmarks; `tools/compute/` contains CPU/GPU experiments.

## Build, Test, and Development Commands

Run commands from the repository root:

- `cargo build --workspace`: build all workspace packages.
- `cargo run -p game-cli -- --list`: list available fixtures.
- `cargo run -p game-cli -- --scenario kernel-fixture --seed 1`: start an interactive, seeded fixture.
- `cargo test -p game-agriculture`: test one feature during development.
- `cargo test --workspace`: run all workspace tests.
- `cargo fmt --all -- --check`: check Rust formatting.
- `cargo clippy --workspace --all-targets -- -D warnings`: lint all targets, treating warnings as errors.
- `cargo bench -p game-sim --bench kernel`: run kernel benchmarks.

## Coding Style & Naming Conventions

Use standard rustfmt formatting and four-space indentation. Use `snake_case` for modules, functions, and tests; `PascalCase` for types; and `SCREAMING_SNAKE_CASE` for constants. Crate names follow `game-<mechanic>`.

Keep state, validation, requests, and rules with their owning feature. Communicate through receiver-owned typed requests and events. Preserve acyclic dependencies and mutate only plugin-owned resources. Architecture tests enforce dependency direction and source-size budgets; split growing mechanics into named owners.

## Testing Guidelines

Use Rust's built-in `#[test]` framework. Place integration tests in each crate's `tests/`, with descriptive behavior names such as `stale_stock_causes_failure_learning_and_backoff`. Share fixtures through `tests/support/mod.rs` where useful.

Feature tests compose required plugins without importing `game-assembly`. Cover changed behavior, rollback, and save/load consistency where relevant. Use assembly tests for composed behavior and architecture constraints. No numeric coverage threshold is configured.

## Commit & Pull Request Guidelines

History generally uses short, imperative subject lines such as “Use typed pending work and compact transaction logs.” Follow that style and keep commits focused.

PR descriptions should explain the behavior change and list validation commands and results. Link relevant issues or design documents, and update affected crate or content documentation when interfaces or schemas change.
