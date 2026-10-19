# game-fauna

`game-fauna` simulates individual animals with diploid genomes, growth, foraging, climate stress, reproduction, and human familiarity or caregiving.

**Why it matters:** Animals are living entities that graze finite local patches, reproduce with inherited traits, and can be fed, tamed, or cared for by characters rather than acting as static resource nodes.

## Responsibilities

- **Animal population (`Fauna`):** Stores installed [`game-animal-species`](../game-animal-species/README.md) definitions, individual `Animal` records (location, age, health, hunger, diploid genome, pregnancy, familiarity, and assigned caregiver), and daily update schedules (`fauna.state`).
- **Ecological & biological ticks:** Advances animal growth, consumes finite edible patch stock (`game-ecology`), applies seasonal climate stress (`game-environment`), and handles diploid reproduction (`game-genetics`) and natural mortality.
- **Human interaction (`Feed`, `Tame`, `Capture`):** Validates co-location, food availability, and animal familiarity when characters feed or handle animals.

## Boundaries

- **Physical animal state vs. human motives:** Owns animal physiology, genetics, and interaction validation; [`game-animal-species`](../game-animal-species/README.md) owns species catalogs and [`game-attachments`](../game-attachments/README.md) / [`game-sentiment`](../game-sentiment/README.md) own human emotional bonds and autonomous care offers.

## How It Works

- **Real patch grazing:** Animals eat from actual local `game-ecology` patches rather than abstract ambient food, competing with or depending on local plant abundance and human feeding.
- **Diploid trait inheritance:** Offspring inherit growth, size, hardiness, and temperament alleles from their parents via `game-genetics`.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`fauna`, resource `fauna.state`) and re-exports `load`.
- **Requests & Events:** Handles `Install`, `Spawn`, `Feed`, `Tame`, `Capture`, and daily biological tick requests; emits animal birth, death, and interaction events.
- **Callers:** Installed by [`game-scenario-assets`](../game-scenario-assets/README.md) and called by [`game-attachments`](../game-attachments/README.md) and [`game-openings`](../game-openings/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-content`](../game-content/README.md), [`game-animal-species`](../game-animal-species/README.md), [`game-genetics`](../game-genetics/README.md), [`game-ecology`](../game-ecology/README.md), [`game-objects`](../game-objects/README.md), [`game-cargo`](../game-cargo/README.md), [`game-land`](../game-land/README.md), [`game-property`](../game-property/README.md), [`game-physiology`](../game-physiology/README.md), [`game-transport`](../game-transport/README.md), [`game-environment`](../game-environment/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** `cargo test -p game-fauna` verifies grazing, climate stress, diploid breeding, human feeding/taming, and save/load state.

Deliberate animal handling requires expressed motor readiness; pursuit also requires independent locomotion. Hunting invitations check readiness before occupying attention, and physical catch requests validate it again. Passive encounters and attachment can still occur before those capabilities develop.
