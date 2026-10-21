# game-exchange

Local bilateral trade (`Trade`, `Barter`), marginal utility valuation, and intelligence-bounded offer search.

**Why it matters:** People have different skills, local resources, and surpluses. `game-exchange` lets co-located characters discover mutually beneficial trades using their own marginal needs, sentimental attachments, and resale expectations—without any global price table.

## Responsibilities

- **Marginal Offer Valuation:** Evaluates marginal gains and costs across personal needs (`game-nutrition`, `game-vitality`), sentimental keepsake values (`game-sentiment`), and expected resale demand (`game-demand`).
- **Intelligence-Bounded Offer Search:** Enumerates candidate pack offers within an actor's intelligence-scaled search budget, retaining the top-ranked offers (with deterministic tie-breaking) without sorting the full combinatorial space.
- **Mutual Consent & Atomic Settlement (`Trade`, `Barter`):** Verifies both parties accept the proposed ratio and executes the bidirectional transfer atomically via `game-agreements` and `game-property`.
- **Aid Requests:** Allows wounded characters to ask local caregivers for medical aid (`game-care`).

## Boundaries

- **No Persistent Price Cache:** Discards temporary valuation estimates before settlement and never exposes one participant's private valuation to the other.
- **No Global Market or Mint:** All exchange ratios are local, bilateral proposals between co-located characters.

## How It Works

- **Snapshot-Scoped Reuse:** Reuses per-person marginal unit values while their stock snapshot is unchanged during offer enumeration, then re-validates live inventory and consent at settlement time.
- **Atomic Rollback:** Wraps offer acceptance and both property/cargo transfers in a single transaction so partial or failed fulfillment rolls back cleanly.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers `Trade` and `Barter` handlers, pre-trade hook registries, and autonomous trade proposals in `game-assembly`.
- **Requests & Events:** Exposes `Trade`, `Barter`, and pre-trade decision hooks; dispatches `game-agreements` settlements.
- **Callers:** Invoked by `game-choice`, `game-characters`, `game-credit`, and scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-objects`, `game-cargo`, `game-items`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-nutrition`, `game-preservation`, `game-traits`, `game-knowledge`, `game-food-memory`, `game-relationships`, `game-sentiment`, `game-demand`, `game-agreements`, `game-care`, `game-choice`, `game-utility`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-exchange`) cover asymmetric quantity trades, keepsake retention, resale medium acquisition, proposal budget bounds, and atomic rollback on failed fulfillment.

Both parties must have the expressed ability to converse and manipulate goods. An infant cannot initiate or consent to a barter merely because it holds inventory or has exceptional inherited potential.
