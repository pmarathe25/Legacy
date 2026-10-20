# game-core

`game-core` defines the shared value types, entity IDs, millisecond timestamps, error primitives, and deterministic counter-based random number generator used across the entire workspace.

**Why it matters:** Every simulation crate needs a common vocabulary for identities, checked integer time arithmetic, and reproducible randomness without pulling in host state or gameplay rules.

## Responsibilities

- **Typed identities:** Strongly typed IDs (`PersonId`, `AreaId`, `RoadId`, `InventoryId`, `GoodId`, `PlotId`, `AgreementId`, `InstitutionId`, `ProjectId`, `MissionId`, `ArmyId`, `LifeId`, `JobId`) with numeric conversions for storage indexing.
- **Simulation time:** `SimTime` and `SimDuration` in integer milliseconds, with checked addition, subtraction, and scaling helpers.
- **Error & validation helpers:** `Error` (`Rejected`, `NotFound`, `Conflict`, `InvalidSave`, `InvalidContent`, `Overflow`), `Result<T>`, and the `require(condition, message)` guard.
- **Deterministic RNG:** Stateless counter-based `random_u64` and `random_range` functions keyed by seed, domain string, and call coordinates.

## Boundaries

- **Pure value layer:** Owns no simulation state, resources, or host scheduling (`game-sim` owns execution and state).
- **Domain-agnostic:** Contains no gameplay formulas or content schemas; feature crates define their own domain rules.

## How It Works

- **Checked time math:** All timestamp and duration operations return `Result` and reject integer overflow before state can corrupt.
- **Counter-based hashing:** Random values are computed purely from `(seed, domain, a, b, c)` via SplitMix64 mixing, so evaluation order across threads never alters random outcomes.
- **Zero-cost IDs:** Wrapper structs serialize transparently and implement `Ord` and `Hash` for deterministic collection ordering.

## Game Integration

- **Plugin / Entry Point:** Pure library crate; registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Imported by almost every crate in `crates/` and `tools/`.

## Dependencies & Testing

- **Dependencies:** None (leaf workspace crate).
- **Testing:** `cargo test -p game-core` verifies checked time arithmetic, overflow rejection, and deterministic RNG output.

`Fingerprint` implements `fmt::Write` for stable FNV-1a fingerprints without a
temporary formatted string. `fingerprint` handles an existing string. Proposal
identities and theory keys retain their previous bytes and hash values.
