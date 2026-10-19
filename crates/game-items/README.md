# game-items

`game-items` assigns sparse individual identities to notable cargo units and tracks their custody and history across transfers and destruction.

**Why it matters:** Most goods are fungible bulk quantities, but written books, heirlooms, and sentimentally attached objects need distinct identities without turning every grain of barley into an individual entity.

## Responsibilities

- **Sparse item registry (`Items`):** Tracks identified units (`ItemId`), their `GoodId`, current `InventoryId` (or destroyed state), origin metadata, and per-person custom labels (`items.registry`).
- **Custody tracking:** Subscribes to `game-cargo` transfer and consumption events to move or retire identified items alongside actual cargo balances.
- **Targeted movement (`Move`):** Allows moving a specific `ItemId` between inventories while bulk cargo operations automatically consume or transfer plain unidentified units first.

## Boundaries

- **Identity overlay, not extra stock:** An `ItemId` always corresponds to 1 physical unit inside [`game-cargo`](../game-cargo/README.md); it adds no extra mass, capacity, or invulnerability.
- **Subjective attachment:** [`game-sentiment`](../game-sentiment/README.md) and [`game-works`](../game-works/README.md) attach memories or compositions to `ItemId`s.

## How It Works

- **Plain-first bulk rule:** When `game-cargo` transfers or consumes $N$ units of a good, `game-items` leaves identified units untouched as long as enough plain units exist in the source inventory.
- **Invariant validation:** Save and transaction validation ensure the count of live `ItemId`s for a `(InventoryId, GoodId)` never exceeds the inventory's actual cargo balance.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`items`, resource `items.registry`).
- **Requests & Events:** Handles `Identify`, `Label`, `Move`; subscribes to `game_cargo::Transferred` and `game_cargo::Consumed`; emits `Destroyed`.
- **Callers:** Used by [`game-sentiment`](../game-sentiment/README.md), [`game-works`](../game-works/README.md), and [`game-exchange`](../game-exchange/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-cargo`](../game-cargo/README.md), [`game-transport`](../game-transport/README.md), [`game-property`](../game-property/README.md), [`game-physiology`](../game-physiology/README.md).
- **Testing:** `cargo test -p game-items` verifies sparse identification, plain-first bulk transfers, explicit `Move`, destruction tracking, and count invariants.
