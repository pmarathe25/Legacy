# game-realm

Office mandates, jurisdictional scope, delegated budgets, and physical delivery of public goods.

**Why it matters:** Political authority over a wider realm requires delegating offices and budgets—where delegates must still physically move goods and dynastic succession never magically refills an empty treasury or restores a lost office.

## Responsibilities

- **Office Mandates:** Attaches jurisdictional mandates and spending limits to recognized offices in `game-institutions`.
- **Budget & Resource Allocation:** Tracks authorized resource allocations (`game-property`, `game-cargo`) bounded by an office's mandate.
- **Physical Delivery Requirement:** Requires officeholders and delegates to physically transport and deliver goods rather than teleporting realm resources.

## Boundaries

- **Separate from Lineage:** Dynastic succession (`game-lineage`) does not automatically restore an institutional office or reset an exhausted mandate budget.
- **No Automatic Tax Teleportation:** Relies on `game-cargo`, `game-property`, and `game-transport` for actual stock movement.

## How It Works

- **Mandate Validation:** Verifies that an actor holds the required active office in `game-institutions` and remains within their mandate's jurisdiction and spending cap before authorizing public stock disbursements.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers mandate tables and office spending handlers in `game-assembly`.
- **Requests & Events:** Exposes mandate assignment, budget allocation, and delegated disbursement requests.
- **Callers:** Used by `game-institutions` governance flows, `game-missions`, and scenario fixtures.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-institutions`, `game-property`, `game-cargo`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-realm`) cover office mandate bounds, spending limits, physical delivery checks, and separation from lineage succession.
