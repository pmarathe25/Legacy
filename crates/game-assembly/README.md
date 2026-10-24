# game-assembly

`game-assembly` composes all feature plugins into a playable `Simulation`, routes public player commands to the current ruler, and constructs `PlayerView` snapshots.

**Why it matters:** Keeping plugin wiring and client command binding in a thin composition layer ensures feature crates remain independent of one another and never import the top-level application.

## Responsibilities

- **Ordered plugin installation (`plugins::builder`):** Registers all hosted feature plugins in dependency order on `game_sim::Builder`.
- **Ruler-bound command routing (`binding`):** Registers the `binding.command` request, resolves the active ruler from [`game-lineage`](../game-lineage/README.md), and dispatches `game_api::Command` variants to their owning feature requests.
- **View composition (`binding::observe`):** Queries committed feature resources for the current ruler to build an immutable `PlayerView`.
- **Simulation lifecycle (`Simulation`):** Exposes `new`, `with_content`, `uncontrolled`, `load`, `save`, `submit`, `observe`, and `advance`.
- **Backward-compatible save loading (`plugins::load`):** Injects empty default state for newly introduced plugins when loading older save manifests.

## Boundaries

- **Zero gameplay logic:** Architecture tests restrict `src/` to `lib.rs`, `plugins.rs`, and `binding.rs` and forbid gameplay loops, thresholds, or scoring in this crate.
- **Privileged debug access:** `debug_host` and `debug_host_mut` are explicitly marked for developer fixtures and tests, never normal player commands.

## How It Works

- **Sequence-keyed command deduplication:** `Simulation::submit(sequence, command)` wraps the public command in an `Execute` request so retries across ruler death or save/load remain idempotent.
- **Witness-filtered events:** `observe` filters `host.notices()` so `PlayerView::events` includes only notices where `witnesses` contains the current ruler.

## Game Integration

- **Plugin / Entry Point:** Application composition crate (`Simulation`, `scenarios`, `scenarios_with_content`, `default_scenario`); registers internal `client-binding` plugin.
- **Requests & Events:** Handles `Execute` (`binding.command`).
- **Callers:** Used by [`game-session`](../game-session/README.md), [`game-cli`](../game-cli/README.md), `tools/history`, and `tools/performance`.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-api`](../game-api/README.md), [`game-openings`](../game-openings/README.md), [`game-content`](../game-content/README.md), and all hosted `game-*` feature crates.
- **Testing:** `cargo test -p game-assembly` runs architecture enforcement (`tests/architecture.rs`), command retry tests, content validation, and full save/load integration checks.

`Simulation::load_from` accepts a reader that can seek. Current checkpoints decode without retaining the entire JSON text, while older manifests rewind through the existing migration path.

`Simulation::set_preparation_window(0..=128)` configures owner-validated entity
preparation inside `Simulation::advance`, including calls with budgets greater
than one. The default is disabled. `preparation_statistics` exposes runtime counts.
Preparation occurs between the same canonical commits; stops, budget accounting,
clock progress and notice collection retain their ordinary semantics. Settings
and advisory caches are not serialized; reapply settings after loading.

Small provider callbacks execute in order by default. The bounded pool remains
available to entity preparation and bulk owner operations.
`set_parallel_listeners(true)` restores provider-level parallel drafting for
controlled comparisons and sufficiently rich custom providers.


Assembly also binds the resource-free `game-autonomy-frames` callback. It adds no
plugin manifest entry or activity to ordinary histories. Frame configuration is
an explicit typed choice request; preparation and action rules remain with their
owners.
