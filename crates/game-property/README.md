# game-property

Conflicting ownership claims, recognition, revocable per-person permission, and physical taking. Unauthorized taking conserves goods and only informs explicit witnesses.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-cargo](../game-cargo/README.md), [game-transport](../game-transport/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-property`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

The reverse per-person permission index uses dense rows and journals; claims and inventory access sets keep persistent snapshots. Loading rebuilds the reverse index. Authorization and discovery consult the same derived membership, and rollback restores both directions together.

Physical use and standing permission are separate. `Use` checks actual locality and records an interference when an actor defies a claim; it does not prohibit the action. Awake, living local observers come from the physiology-owned `Observers` query. `reluctance` estimates the actor's own conscience, recognition of claims, learned values, fear, and grievance. It is a choice cost, not a command veto.

`Take::unauthorized` permits an actor to deliberately proceed against a claim. The resulting event records whether the transfer actually lacked permission. Cargo still checks locality, quantities, mass, and capacity.
