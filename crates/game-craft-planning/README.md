# game-craft-planning

Pure, bounded backward search over remembered gathering patches, stock, and
workshop recipes. `game-livelihoods` chooses goals; this crate finds executable
`Step` sequences with the lowest labor cost explored within the caller's depth,
step, and node limits. Equal-cost alternatives retain their original order.

A speculative journal updates one working stock and rolls back rejected branches.
Winning branches retain only changed quantities and appended steps, avoiding a
complete stock-map copy for every alternative. Capacity checks track mass changes alongside the journal, including checked
products and a wide total that can recover after excess stock is consumed. They
still include all carried goods. Cyclic recipes cannot promise more output than remains after their
own inputs are consumed. Failure restores the caller's stock, steps, and cost;
exploration still spends the shared node budget.

This crate owns no world resources or plugin. It depends on `game-core`,
`game-objects`, `game-workshop`, and `game-ecology`. `cargo test -p
game-craft-planning` compares against the earlier copying algorithm across 1,200
seeded recipe graphs, including cycles, stock reuse, capacity rejection, and
cognitive bounds, with separate overflow and incomplete-output regressions.
