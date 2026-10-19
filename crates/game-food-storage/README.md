# game-food-storage

Built food stores, root cellars, and thermal spoilage modifiers for stored provisions.

**Why it matters:** Raw harvests spoil quickly in warm weather. Constructing sealed stores and underground cellars lets settlements buffer seasonal harvests across harsh winters and dry spells.

## Responsibilities

- **Storage Structures:** Tracks built sealed food stores and cellars created from crafted kits (`food_storage.state`).
- **Thermal Spoilage Scaling:** Slows spoilage in cold ambient temperatures and accelerates it in heat, while buffering cellars toward the region's time-weighted annual mean temperature.
- **Storage Placement & Use:** Proposes building food storage structures and moving perishable food into protected stores via `game-choice`.

## Boundaries

- **No Batch Ledger:** Does not track individual FIFO food batches or execute spoilage removal (owned by `game-preservation`).
- **No Shelter Exposure:** Does not model human thermal shelter or resting comfort (owned by `game-shelter` and `game-comfort`).

## How It Works

- **Conditions Provider:** Registers as the storage condition provider for `game-preservation`, computing effective shelf-life multipliers from `game-environment` local temperature, cellar depth buffering, and sealed container tags.
- **Storage Proposals:** Evaluates local perishable surpluses and available storage kits during `game-choice::Collect` to propose constructing stores or transferring exposed food indoors.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers food storage state, `game-preservation` condition hooks, and `game-choice` proposals in `game-assembly`.
- **Requests & Events:** Handles storage construction and transfer actions (`Build`, `Store`) and provides thermal storage metrics to `game-preservation` and `game-maturation`.
- **Callers:** Queried by `game-preservation`, `game-maturation`, `game-food-inquiry`, and `game-observations`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-preservation`, `game-cargo`, `game-objects`, `game-environment`, `game-transport`, `game-property`, `game-physiology`, `game-knowledge`, `game-characters`, `game-lineage`, `game-traits`, `game-shelter`, `game-choice`, `game-vitality`.
- **Testing:** Integration tests in `tests/` verify cellar temperature buffering, sealed store shelf-life extension, and autonomous food storage construction.
