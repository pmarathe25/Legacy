# game-confinement

Physical cells, built barriers, active guarding, restraint for forced care, and capability-driven escape.

**Why it matters:** Holding someone captive—or restraining an uncooperative patient—requires real physical barriers and an attentive guard. An unguarded prisoner or a strong, tool-equipped captive can break out.

## Responsibilities

- **Cell Construction & Barriers:** Tracks physical confinement cells and built barriers whose construction consumes real materials (`game-cargo`, `game-objects`) and labor time.
- **Active Guarding & Release:** Requires a custodian to occupy their action clock to actively guard a captive; an absent or busy custodian provides no free resistance.
- **Target Resistance & Escape:** Evaluates confinement attempts against healthy targets using their own consent and relationships, and resolves escape attempts from live strength, agility, bodily condition (`game-vitality`, `game-physiology`), and accessible tools (`game-equipment`, `game-property`).
- **Subdued Guardian & Care Restraint:** Preserves cell records for regenerating `game-guardians` while subdued and exposes physical restraint status to forced-feeding (`game-nurture`) and forced-treatment (`game-care`) paths.

## Boundaries

- **No Caller-Fabricated Consent:** Ignores legacy caller-supplied `consent` or escape-capability flags, computing resistance and breakout strength from live character state.
- **Choice-Driven Guarding:** Building cells, guarding, and attempting escape are proposed via `game-choice` rather than enforced as automatic office scripts.

## How It Works

- **Cancellable Guard Attention:** Guarding uses `game-choice` cancellable attention so urgent personal needs or interruptions can pull a guard away, opening a window for escape.
- **Live Capability Checks:** Escape compares the captive's current physical output and tool leverage against the barrier's integrity and any present guard's active opposition.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers cell/restraint state, construction/guard/escape handlers, and `game-choice` providers in `game-assembly`.
- **Requests & Events:** Exposes cell construction, confinement, guarding, release, and escape requests, plus restraint queries.
- **Callers:** Queried and invoked by `game-care`, `game-nurture`, `game-guardians`, `game-justice`, and `game-choice`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-choice`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-relationships`, `game-guardians`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-confinement`) cover cell construction, active vs. absent guards, tool-assisted escape, healthy target resistance, and subdued guardian retention.
