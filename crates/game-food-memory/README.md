# game-food-memory

Indexes the stores, edible portions, and carried containers in a character's
personal evidence. It reads knowledge and authored food rules; it cannot inspect
actual inventory contents or teach facts. Provisioning still owns acquisition
plans, routes, consequences, and physical requests.

`with_stores` updates a process-local index from changed immutable fact payloads.
Unchanged historical inventories stay cold. Meal and carried-container scans visit
only relevant entries; fallback gathering can still inspect all remembered stores.
Original key ordering and noncanonical alias behavior are preserved. Source,
confidence, observation time, and other assessments remain in personal facts.

The cache retains at most 1,024 recently queried characters and is never saved.
Old, inactive characters' derived projections can be evicted without deleting
their personal evidence. Evidence rollback, another world's facts,
changed food definitions, or an empty memory all rebuild the appropriate view.
Nested queries release the cache borrow before executing their reader. No game
resource is mutated, so cache population and eviction cannot affect decisions.

Run `cargo test -p game-food-memory`. Tests cover aliases, moving remembered
containers, disappeared evidence, rollback, nested queries, and rule isolation.

The assessment module combines remembered stocks with the actor’s experienced dietary motives and perceived self provision. Its appraisal cache is local to one unchanged planning query. Personal carried supplies and known personal stores remain eligible; a deterministic recall window over other remembered stores expands with expressed intelligence, up to 64 suppliers. This bounds comparison work without deleting memories or supplying unseen stocks.
