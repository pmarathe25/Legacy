# game-world

`game-world` composes the simulation's people, bodies, stocks, and domain states into a shared world. It supplies domain facts to their owners and applies cross-system effects within that world, so changes in one area can affect others.

The crate builds playable openings and debug scenarios, applies player commands, and produces views that reflect the ruler's knowledge. It also saves and restores world state. `game-sim` supplies the generic scheduler and executor; `game-session` owns the session host.

The migration target is a feature-agnostic host. Feature modules such as `health.rs` and `genetics.rs` belong in their domain crates and will leave this crate. A separate game assembly crate will register components and systems, connect typed feature interfaces, and own playable scenarios and public command/view bindings. The host will retain shared storage, clock and identity management, execution, and transaction infrastructure without depending on individual game features.
