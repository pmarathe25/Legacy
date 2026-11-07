# Legacy

Legacy is a headless, data-driven simulation game built as a Rust workspace. The current host composes physical goods, autonomous people, social relationships, work, travel, health, combat, and the beginnings of a multigenerational campaign. It remains under development.

People make their own decisions. Social arrangements arise from their actions; promises do not compel compliance, and people can act from fear or felt obligation without privately agreeing.

Run from the repository root:

```sh
cargo run -p game-economy-cli -- --player
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The [runner documentation](crates/game-economy-cli/README.md) describes scenarios and commands. [Content](content/README.md) provides runtime definitions. The [design documents](docs/plan.md) describe intended mechanics, including unfinished work. The CPU simulation is designed for parallel batches; full-game scaling has not yet been established.
