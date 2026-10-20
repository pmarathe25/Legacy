# game-traits

`game-traits` owns the runtime catalog of authored character traits, their bounds and utility-channel weights, and each person's trait profile.

**Why it matters:** Representing personality dispositions and physical/cognitive capacities as data-driven profiles allows new traits in `content/traits/` to influence utility scoring and mechanics without trait-specific code branches.

## Responsibilities

- **Trait catalog & profiles (`Traits`, `Profiles`):** Stores authored `TraitDef` definitions (`traits.catalog`) and per-person trait value maps (`traits.profiles`).
- **Compiled utility weights:** Precomputes each person's 5-channel utility weight vector (`need`, `goal`, `value`, `effort`, `risk`) from their non-default trait deviations.
- **Compiled ability & disposition arrays:** Caches fast numeric lookups for raw physical/cognitive abilities and normalized personality dispositions, rebuilt automatically on load.

## Boundaries

- **Baseline profiles vs. expressed capability:** Stores a person's baseline trait profile; [`game-vitality`](../game-vitality/README.md) modulates those baselines by childhood development, aging, and experienced condition.

## How It Works

- **Deviation-only weighting:** Omitted traits in a person's profile default to the catalog's `default` score and contribute zero delta to utility channel weights.
- **Revision-checked caches:** Derived numeric caches verify the catalog and profile revision before serving fast reads and are excluded from save files.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`traits`, resources `traits.catalog` and `traits.profiles`) and provides `load(&mut Store)`.
- **Requests & Events:** Handles `Install` and `Set`.
- **Callers:** Populated by [`game-scenario-assets`](../game-scenario-assets/README.md), [`game-openings`](../game-openings/README.md), and [`game-heredity`](../game-heredity/README.md); queried by `game-vitality`, `game-characters`, and decision owners.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-content`](../game-content/README.md), [`game-sim`](../game-sim/README.md).
- **Testing:** `cargo test -p game-traits` verifies catalog validation, profile bounds, compiled weight equivalence, and save/load cache reconstruction.

Repeated attribute queries reuse eight recent personal numeric projections per
thread, guarded by weak source and compiled-record versions. Direct profile edits,
retained snapshots, and world changes still receive exact values. The cache holds
only those individual profiles, not a whole traits resource or population.
