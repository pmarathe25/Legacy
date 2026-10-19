# game-agreements

Negotiated multi-party promises, acceptance/refusal, outstanding duties, physical fulfillment, and default.

**Why it matters:** Trade, credit, and social contracts depend on explicit promises between consenting parties—where signing an agreement never magically teleports goods and missing a deadline is a real default rather than an automatic seizure.

## Responsibilities

- **Agreement Lifecycle:** Tracks proposed, accepted, refused, fulfilled, and defaulted multi-party agreements.
- **Explicit Consent:** Requires each named counterparty to accept an agreement before its duties become active.
- **Physical Fulfillment:** Validates and executes promised cargo transfers (`game-cargo`) and property claim updates (`game-property`) when a party performs their duty.
- **Deadline & Default Tracking:** Records unfulfilled obligations past their maturity timestamp as defaults without automatically editing inventories.

## Boundaries

- **No Automatic Inventory Edits:** Neither accepting a promise nor missing a deadline moves physical goods without an explicit fulfillment action.
- **No Offer Valuation:** Does not search for trade partners or score marginal utility (owned by `game-exchange` and `game-credit`).

## How It Works

- **Transactional Fulfillment:** When a party fulfills a term, `game-agreements` verifies current possession and permissions, performs the `game-cargo` and `game-property` mutations atomically, and emits settlement events consumed by `game-demand`.
- **Inspectable Duty Records:** Stores outstanding obligations and past settlements so participants and observers can evaluate reliability.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers agreement state, proposal/acceptance handlers, and fulfillment/default logic in `game-assembly`.
- **Requests & Events:** Exposes agreement creation, acceptance, refusal, and fulfillment requests; emits settlement and default events.
- **Callers:** Used by `game-exchange`, `game-credit`, `game-demand`, and scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-property`, `game-cargo`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-agreements`) cover multi-party consent, partial and full fulfillment, atomic rollback on missing stock, and deadline defaults.
