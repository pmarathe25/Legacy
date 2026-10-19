# game-conflict

Simultaneous-round tactical combat, continuous encounter geometry, lasting bodily damage, and attributed `Struck` evidence.

**Why it matters:** When coercion, grievances, or warfare escalate into violence, outcomes should come from physical positioning, bodily condition, and simultaneous exchanges of blows—producing real wounds and witnessed evidence rather than abstract HP bars.

## Responsibilities

- **Tactical Encounter Space:** Generates continuous 2D encounter geometry for local battles and advances combat in fixed-duration simultaneous rounds.
- **Simultaneous Movement & Attacks:** Resolves participant movement and attack intentions concurrently within each shared round, pausing for player input when the active ruler is engaged.
- **Lasting Bodily Damage:** Applies physical trauma and wounds through `game-physiology` with no artificial damage caps or plot armor.
- **Attributed Strike Evidence (`Struck`):** Emits `Struck` events after each round's simultaneous damage is applied so victims and witnesses know who hit whom.

## Boundaries

- **Tactical Resolver Only:** Does not decide why a fight started or what political submission means (owned by `game-pressure`, `game-grievances`, and `game-communities`).
- **No Hidden Orders:** Social plugins react to emitted `Struck` and completion events without injecting hidden mid-round tactical overrides.

## How It Works

- **Action Clock Reservation:** Occupies the canonical action slot for all combatants during the encounter so fighters cannot simultaneously craft or travel elsewhere unless they execute a withdrawal.
- **Deterministic Round Resolution:** Computes positions, reach, and strike impacts deterministically across all combatants before committing wounds and broadcasting `Struck` notifications.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers battle state, round timers, and tactical command handlers in `game-assembly`.
- **Requests & Events:** Exposes battle start, movement/attack orders, and withdrawal requests; emits `Struck` and encounter completion events.
- **Callers:** Used by `game-pressure`, `game-grievances`, `game-naming`, `game-api`, and `game-cli`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-physiology`, `game-transport`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-conflict`) verify simultaneous round resolution, continuous positioning, lasting wounds, mortality, player input stops, and `Struck` event attribution.
