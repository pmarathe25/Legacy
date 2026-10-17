# game-soma

Owns living body state and validates bodily changes, damage, healing, exertion, age-related development, and death. Callers use typed requests; only this owner mutates bodies. Physiology re-exports the public requests and composes them with eating, feeding, rest, and the needs clock. `game-body` provides values and pure evolution; vitality provides inherited potential and condition.

`living()` traverses an index rebuilt on load. When deceased records dominate, physiology updates only living rows with ordinary transaction undo. Deceased rows retain their physical attributes and a stored clock anchor. `body()` and serialization project their clocks through the pure body rule to the last physiology phase. This preserves the existing logical save format without rewriting every deceased row on each tick. Resource equality uses the same projection; retained snapshots and failed transactions keep their own phase clocks.

Run `cargo test -p game-soma -p game-physiology`. Storage tests compare deferred records with eager updates across partial ticks, rollback, retained readers, empty living populations, and reloads with different worker counts.
