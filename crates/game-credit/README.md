# game-credit

Voluntary deferred exchange, transferable debt promises, and personally learned unit conventions.

**Why it matters:** Immediate barter fails when one party's harvest or craft output isn't ready yet. `game-credit` lets characters lend existing stock against future repayment and endorse unpaid claims to third parties without inventing a central bank or fiat money.

## Responsibilities

- **Deferred Loans:** Creates finite deferred-repayment terms in `game-agreements` while immediately transferring existing stock via `game-cargo` and `game-property`.
- **Voluntary Repayment:** Proposes and executes loan repayment as an autonomous character choice (`game-choice`) shaped by honesty, trust, and relationship bonds.
- **Claim Endorsement:** Allows a creditor to assign an unpaid claim to an accepting third party in settlement of another obligation.
- **Learned Quantity Conventions:** Tracks personal unit quantity conventions learned from participating in real trades (`game-exchange`).

## Boundaries

- **No Remote Ledger Omniscience:** Characters remember only loan terms they participated in or were told about—planning never peeks at a global remote agreement ledger.
- **No Fiat Currency or Banking:** Provides coarse interpersonal credit primitives without central banks, interest-bearing deposits, or designated currency goods.

## How It Works

- **Agreement-Backed Settlement:** Loan creation, repayment, and claim endorsement execute atomically through `game-agreements` and `game-property` transfers.
- **Choice-Driven Fulfillment:** Debtors weigh keeping their stock against relationship damage, reputation, and honesty norms when deciding whether to repay a matured promise.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers credit state, learned unit tables, and loan/repayment/endorsement choice handlers in `game-assembly`.
- **Requests & Events:** Exposes loan, repayment, and claim-endorsement actions; coordinates with `game-agreements` and `game-exchange`.
- **Callers:** Driven by `game-choice` and local trade encounters.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-choice`, `game-agreements`, `game-exchange`, `game-objects`, `game-cargo`, `game-property`, `game-relationships`, `game-vitality`, `game-physiology`, `game-transport`, `game-social-life`, `game-nutrition`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-credit -p game-agreements -p game-exchange`) cover loan creation, voluntary repayment, third-party claim endorsement, and learned unit quantities.
