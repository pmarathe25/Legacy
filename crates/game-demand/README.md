# game-demand

Personal memory of trade acceptance and expected resale value without hardcoded currency.

**Why it matters:** Money and liquid trade media should emerge from what people actually accept in exchange, not from a global `gold` field. `game-demand` lets characters learn which goods are widely accepted and acquire them as media of exchange.

## Responsibilities

- **Counterparty Acceptance Memory:** Records which goods counterparties accepted in completed `game-agreements` that the observer personally joined (bounded to 64 counterparties per good).
- **Evidence Decay:** Gradually decays remembered acceptance over time when not reinforced by fresh trades.
- **Expected Resale Valuation:** Combines remembered acceptance frequency with personal foresight (`game-traits`), item mass (`game-cargo`), and shelf life (`game-preservation`) to estimate a good's resale utility.

## Boundaries

- **No Designated Currency:** Contains no currency field, global price table, money balance, or privileged commodity kind.
- **No Trade Execution:** Does not negotiate or settle trades directly (owned by `game-exchange` and `game-agreements`).

## How It Works

- **Replay-Safe Settlement Learning:** Listens to completed trade settlements and records unique counterparty acceptance only for participants, ensuring replayed or historical records never double-count demand.
- **Emergent Medium Concentration:** When a durable, portable good gains broad acceptance across local traders, its expected resale value rises, naturally motivating intermediaries to accept it even when they do not personally consume it.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers per-person acceptance tables and agreement settlement listeners in `game-assembly`.
- **Requests & Events:** Consumes completed agreement events from `game-agreements`; exposes resale value queries to `game-exchange`.
- **Callers:** Queried by `game-exchange` and `game-credit` during offer valuation.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-objects`, `game-cargo`, `game-preservation`, `game-traits`, `game-agreements`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-demand`) verify counterparty acceptance tracking, 64-entry bounds, temporal decay, durability/portability weighting, and emergent medium preference.
