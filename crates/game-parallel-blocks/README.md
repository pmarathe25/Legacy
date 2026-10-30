# Compiled block runtime

`Engine::compile` validates an editable `game-parallel::World` and stores people in persistent region-spanning blocks with dense physical columns and private next-state buffers. Typed commands and staged owner updates operate on the compiled state; `export` returns a canonical editable world at a committed boundary. JSON exports can be loaded and compiled again.

The runtime includes the experimental physical, travel, food cargo and shipment, family, lineage and guardian search, agriculture, production, goods cargo, trade, autonomous work, social, knowledge, and travel-decision owners. Food claims use person ID priority within each region, independent of block layout and reloads. Mixed overlapping blocks use a parallel sort when food is contested; other cases use block-prefix allocation. The editable engine may use a different food priority.

Run `cargo run -p game-parallel-block-cli -- content/parallel/workshop.toml` for the administrative interface. Run `cargo run --release -p game-parallel-blocks --example block_benchmarks -- block_throughput` (or select `travel`, `production`, `economy`, `families`, `autowork`, or `bandwidth`) for synthetic measurements with explicit worker counts. They do not establish full-game scaling: the integrated economy and family workloads still need substantial work to approach linear scaling.

This is not yet a complete implementation of the original game. In particular, full player choices and invocation effects, full guardian agency, institutions, property, conflict, and many other intended mechanics remain to be implemented. CPU execution is the current focus; the block layouts do not yet provide a GPU runtime.


`[lineage.invocation]` adds tick-boundary invocation with saved warnings and explicit acceptance. `run` stops at a pending warning; `invocation` shows its revision, and `accept-invocation REVISION` accepts it. `memories` shows source-tagged borrowed observations without copying them into the ruler's knowledge. This is an initial lifecycle mechanism: actual strain consequences, inherited abilities, and autonomous ruler behavior during loss of control are still missing.


Production jobs use [packed blocks](../game-parallel-production-packed/README.md) with direct admission and interned material bundles. Some explicit market and job-edit commands still materialize ordinary jobs. `bandwidth` measures a simple streaming baseline; it helps diagnose memory saturation but does not establish simulation speed.



The block host also runs the experimental stock-property and regional-institution owners. Ballots, property decisions, and market restrictions are staged with the rest of a tick. The CLI provides civic inspection and commands; these are limited models, not a complete political or legal simulation.
