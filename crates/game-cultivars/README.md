# game-cultivars

`game-cultivars` tracks diploid plant strains, seed-lot genetic provenance across cargo transfers and seed-saving recipes, and grower seed selection.

**Why it matters:** Domestication and local crop adaptation emerge because harvested produce and saved seeds carry their actual parent strain genotypes, and farmers preferentially replant lots that yielded well in their own experience.

## Responsibilities

- **Strains & cargo lots (`Cultivars`):** Stores diploid plant `Strain` records (parentage, genome, expressed yield, growth speed, thermal/moisture tolerance, and lifespan) and per-inventory seed/produce lot compositions (`cultivars.state`).
- **Propagation:** Recombines parental alleles during sexual seed production (`game-genetics`) or preserves parental genotypes with bounded mutation for `clonal` cuttings.
- **Cargo & workshop provenance:** Subscribes to `game-cargo` transfers/consumption and `game-workshop` seed-saving crafts so genetic lot identities follow physical seeds from harvest to storage to resowing.
- **Experiential lot selection:** Lets growers pick among available seed lots using their own observed harvest productivity and expressed intelligence—never by inspecting raw allele integers.

## Boundaries

- **Plant genetics & lot tracking:** Owns strain records and cargo lot provenance; [`game-botany`](../game-botany/README.md) manages wild stands and [`game-agriculture`](../game-agriculture/README.md) manages cultivated plots.

## How It Works

- **Growth-yield trade-off:** Genetic expression couples higher harvest yield potential to longer development duration, preventing a single trivial super-strain from dominating every climate.
- **Pruned live state:** Extinct strains with no remaining live plants or cargo lots do not bloat active simulation tables while their lineage events remain in history journals.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`cultivars`, resource `cultivars.state`).
- **Requests & Events:** Handles strain registration, lot splitting/merging, and offspring generation; subscribes to `game_cargo::Transferred` and `game_cargo::Consumed`.
- **Callers:** Used by [`game-botany`](../game-botany/README.md), [`game-agriculture`](../game-agriculture/README.md), [`game-crop-cohorts`](../game-crop-cohorts/README.md), and [`game-workshop`](../game-workshop/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-genetics`](../game-genetics/README.md), [`game-cargo`](../game-cargo/README.md), [`game-objects`](../game-objects/README.md), [`game-transport`](../game-transport/README.md), [`game-traits`](../game-traits/README.md), [`game-vitality`](../game-vitality/README.md), [`game-diagnostics`](../game-diagnostics/README.md).
- **Testing:** `cargo test -p game-cultivars` verifies sexual vs. clonal inheritance, cargo lot transfer tracking, seed-saving provenance, and save/load validation.
