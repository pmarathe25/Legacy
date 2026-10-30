# Persistent social edges

Identity-hashed shards own fixed relationship pages, pair-to-slot indexes, and
exact expiry sets. Preparation reads committed pages and stages only changed,
new, or expired edges. Commit applies disjoint shard patches; export alone
materializes canonical relationships. Original imported order is preserved until
the first scheduled encounter boundary, including intervening expiry-only ticks.

Expiry occurs before encounter renewal. Expired pairs restart their encounter
count, even if their former count was saturated. Renewals replace expiry entries;
stale deadlines cannot accumulate. Removed slots are reused without invalidating
other shards. Generation tokens reject foreign and superseded drafts.

Measure indexed social selection with `LEGACY_PEOPLE=200000`, `LEGACY_WORKERS=1` (or another worker count), and `cargo test -p game-parallel-social-edges busy_selection_benchmark -- --ignored --nocapture`. The timed workload includes busy-person filtering, encounter selection, and edge preparation.
