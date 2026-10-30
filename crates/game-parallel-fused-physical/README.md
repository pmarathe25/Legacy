# Fused parallel simulation host

Optional `[skills]` rules train personal recipe skills from newly completed jobs. Learned rates apply from the following tick, while waiting output never repeats practice. Skills publish with the rest of the generation, survive save/load and death, and are not inherited by newborns. See [`game-parallel-skills`](../game-parallel-skills/README.md) and [`skills.toml`](../../content/parallel/skills.toml) for the data model and a running fixture. The editable and block reference hosts reject enabled skills rather than silently omitting their effects.

This runtime stores physical state and active production work in person-owned blocks. It stages food and output arbitration, farming, shipments, family life, fatigue, personal skills, property, social encounters, knowledge, autonomy, markets, lineage, guardians, invocation, conflict, care, and environmental exposure before publishing a generation. Travel, work, custody, and title transfers use checked commands. `parallel-block-cli` runs this host, and saved state remains an editable `game-parallel::World`.

The implementation is still an incomplete replacement for the original game: many original mechanics are not yet modeled, and some actions and owner adapters still materialize compatibility views. Benchmarks have not established near-linear full-simulation scaling. The separate block engine remains a comparison host for differential tests.

The concentrated work-request benchmark runs `cargo run --release -p game-parallel-fused-physical --example fused_combat_rounds -- 1 100000 hot work-requests 7`; repeat with other worker counts. It measures admission separately from a full tick in which nearly 100,000 recipients compete for enough materials for half of them. Compilation, fixture creation, and result verification are outside the measured intervals.

Use `work-claims` in place of `work-requests` to include a single contested claim, physical use, and the present owner's individual reactions to successful reservations.

Optional `World.tactics` stores persistent local maps and combat placements. `begin_tactical_combat` validates placements and encounter admission atomically; plain `begin_combat` requires tactics disabled. `tactical_moves` accepts simultaneous swaps and one move per fighter per tick. Movement updates compact geometry points and invalidates outstanding drafts while retaining strike topology and prior receipts. Only membership-changing rounds synchronize placements; resolved encounters retain maps. CLI: `tactical-fight ID REGION A B AX AY BX BY`, then `tactical-move ID PERSON X Y`. Movement stages sparse edits against cached participant slots and occupancy; dense batches validate and publish in parallel. Casualties leave geometry available for strike reads and lazily rebuild the affected battle slots on its next movement command. Initial index construction and post-casualty slot refresh remain visible setup costs.

Optional `[purpose]` rules model a separate personal deprivation need. Successful work-goal progress relieves it, blocked goals increase it, and severe deprivation can damage health. Newborns enrolled by the family owner begin accumulating this need at adulthood. See [`game-parallel-purpose`](../game-parallel-purpose/README.md) and [`purpose.toml`](../../content/parallel/purpose.toml).
Optional `[dreams]` define enduring personal aspirations that periodically form or renew concrete work goals. The [dreams owner](../game-parallel-dreams/README.md) stages reviews after physical and lifecycle outcomes and keeps explicitly disabled goals disabled.

Optional `[personality]` records shape voluntary work and social participation; `inherit_births` can pass parent traits to newborns. See [`game-parallel-personality`](../game-parallel-personality/README.md). Travelers coordinate real expeditions through willing or pressured participation using [`game-parallel-missions`](../game-parallel-missions/README.md). Run `cargo run --release -p game-parallel-fused-physical --example fused_benchmarks -- fused_parties 1 100000 8` with different worker counts to measure this concentrated party workload.

With `[cargo] auto_eat_local = true`, people can eat a carried meal while present in a region. Those meals are reserved before regional food allocation, so the same person cannot consume two meals in one tick. A failed tick consumes neither source.

Optional `[nutrition]` rules track six personal nutrient reserves. Actual scalar meals add the configured blend, while eaten goods supply their authored nutrient values when digested. Reserves are capped, metabolism and deficiency damage occur at the configured interval, and newborns begin at the configured initial reserves. A deficiency can lead to ordinary death and lineage consequences; consumption and its delayed effects survive saves and failed generations.

The concentrated eating benchmark accepts `diet-food` or `diet-nutrition` in place of `local-food` and times both eating and the next digestion boundary, including nutrition when enabled.

Run `cargo run --release -p game-parallel-fused-physical --example fused_combat_rounds -- 1 100000 hot local-food 7` and vary the worker count to measure a concentrated tick with both personal meals and scarce regional food. Generation and result verification are outside the timed interval.


Measure personal food actions using `cargo run --release -p game-parallel-fused-physical --example fused_combat_rounds -- 1 100000 hot acquire-food 5` and replace `acquire-food` with `food-aid` to measure simultaneous aid decisions. Change `hot` to `spread` for independent recipients and vary the worker count. These timings cover the focused tick, not a complete generated campaign.

Use `food-requests` to time admission of many requests addressed to one person and the subsequent tick; use `auto-food-requests` to time autonomous creation and a following tick where the addressed person evaluates all configured relationships.

The replacement host rejects preset institution bodies and civic automation. Cooperation, claims, personal feelings, and physical confrontation remain individual behaviors; broader learned social practices are still unfinished.
