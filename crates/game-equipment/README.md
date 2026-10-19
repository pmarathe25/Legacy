# game-equipment

`game-equipment` manages worn clothing, garment condition wear, and cloth-based repair.

**Why it matters:** Characters need physical garments with `warmth:N` and `weatherproof` tags to buffer cold and wet weather, while wear and repair create ongoing demand for textile production.

## Responsibilities

- **Equipped garments (`Equipment`):** Tracks each person's currently worn garment (`GoodId`, inventory source, and durability condition) and whether equipment wear is enabled (`equipment.state`).
- **Donning & doffing (`Equip`, `Unequip`):** Equips an accessible carried garment or removes worn clothing (e.g., to prevent overheating in warm weather).
- **Wear & repair (`Repair`):** Degrades worn condition over time and consumes one accessible item tagged `repair-cloth` to restore durability.

## Boundaries

- **Physical state vs. choice policy:** Owns equipped state, condition decay, and repair execution; [`game-comfort`](../game-comfort/README.md) generates autonomous clothing/repair offers and [`game-exposure`](../game-exposure/README.md) computes thermal stress on the body.

## How It Works

- **Condition-scaled insulation:** Effective garment warmth scales proportionally with current condition; swapping to a different garment retains the wearer's slot condition so swapping cannot bypass repair.
- **Cargo conservation:** Equipped garments remain counted in the wearer's carried cargo rather than disappearing into a separate inventory space.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`equipment`, resource `equipment.state`).
- **Requests & Events:** Handles `Enable`, `Equip`, `Unequip`, `Repair`.
- **Callers:** Enabled by [`game-campaign`](../game-campaign/README.md), driven autonomously by [`game-comfort`](../game-comfort/README.md), and queried by [`game-exposure`](../game-exposure/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-storage`](../game-storage/README.md), [`game-cargo`](../game-cargo/README.md), [`game-objects`](../game-objects/README.md), [`game-transport`](../game-transport/README.md), [`game-physiology`](../game-physiology/README.md), [`game-environment`](../game-environment/README.md), [`game-property`](../game-property/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** `cargo test -p game-equipment` verifies equipping, overheating removal, durability decay, `repair-cloth` consumption, and save/load state.
