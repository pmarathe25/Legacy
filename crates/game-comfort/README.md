# game-comfort

`game-comfort` generates autonomous character proposals for equipping/removing clothing, repairing worn garments, seeking indoor shelter, and fueling hearth fires.

**Why it matters:** Separating thermal decision policy from the physical state of [`game-equipment`](../game-equipment/README.md) and [`game-shelter`](../game-shelter/README.md) lets comfort motives compete directly with food, care, and work through [`game-choice`](../game-choice/README.md).

## Responsibilities

- **Heating intentions (`Plans`):** Tracks active fuel-gathering errands for cold shelters (`comfort.intentions`).
- **Clothing offers (`clothing::options`):** Proposes equipping warm garments in cold weather, unequipping heavy coats when overheating, and repairing damaged clothing when `repair-cloth` is available.
- **Shelter & fuel offers (`heating::options`):** Proposes retreating indoors during freezing weather, building a shelter when a kit is available, or gathering/burning fuel for an unheated room.

## Boundaries

- **Decision policy only:** Mutates only its own fuel intention table; physical garment state and indoor rooms stay in [`game-equipment`](../game-equipment/README.md) and [`game-shelter`](../game-shelter/README.md).

## How It Works

- **Choice-driven arbitration:** Subscribes to `game_choice::Review` (`comfort.options`), scores candidate clothing and shelter actions via `game_choice::motive`, and submits them to the active choice collection.
- **Player exclusion:** Never issues implicit clothing or shelter actions for the currently controlled player ruler.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`comfort`, resource `comfort.intentions`).
- **Requests & Events:** Registers `comfort.options` choice provider; subscribes to `game_physiology::Died` to clear fuel intentions.
- **Callers:** Invoked during character choice collection via [`game-choice`](../game-choice/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-choice`](../game-choice/README.md), [`game-storage`](../game-storage/README.md), [`game-equipment`](../game-equipment/README.md), [`game-shelter`](../game-shelter/README.md), [`game-lineage`](../game-lineage/README.md), [`game-physiology`](../game-physiology/README.md), [`game-objects`](../game-objects/README.md), [`game-transport`](../game-transport/README.md), [`game-cargo`](../game-cargo/README.md), [`game-environment`](../game-environment/README.md), [`game-vitality`](../game-vitality/README.md), [`game-property`](../game-property/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-ecology`](../game-ecology/README.md).
- **Testing:** Exercised by `cargo test -p game-equipment -p game-shelter -p game-exposure` and assembly integration tests.
