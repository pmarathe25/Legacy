# Transactional lot custody

Parallel preparation of physical lot movements and expiry. Claims are arbitrated against one boundary snapshot; accepted moves carry the exact earliest-expiring cohorts, and publication is atomic. The owner is under development as the foundation for integrating perishable goods with the fused host; existing aggregate inventories do not yet use it.

`stock_view` returns only regional stock at the requested current or next boundary, using the same expiry rules and revision as a full view. Observation systems that need regional totals can avoid materializing every person's custody.

`Draft::take_receipts` moves outcomes and spoilage to a detached consumer without publishing custody. A later commit still checks the draft generation and applies the same physical changes; the drained receipt lists are empty in the commit result.

`cargo run --release -p game-parallel-lots --example contention -- 1 100000 source 5` times a concentrated source; use `destination` for a concentrated destination and vary the worker count.

`prepare_transactions` stages a person's physical action as an all-or-nothing set of movements and output creations. An output receives its expiry when actually created, and remains in its output custody if later storage is unavailable. Claims are conservative: a rejected leg rejects its entire transaction, and capacity tentatively claimed by a rejected transaction is not reallocated until a later boundary. Callers must record the transaction outcomes and should retry according to each person's own decision; the owner does not create social agreements or enforce promises. Created lots must have a deadline strictly after the boundary. Existing scalar food is outside this owner and has no expiry.

`prepare_flat_transfers` accepts independently atomic single movements in a flat batch. It returns no fast-path draft when endpoints contend or the batch needs general handling; the caller must then submit normal transactions to the general arbiter. It preserves dated cohorts, boundary spoilage, atomic drafts, and the existing save format.

`prepare_partitioned_flat` also separates transfers whose endpoints are exclusive from those that share a source or destination. The shared group uses the general arbiter with the original action identities and seed. Both groups remain one atomic draft, with expiry applied once. Unsupported layouts still return no specialized draft.

Independent transfers keep moved cohorts in immutable chunk-owned receipt arenas. Each outcome exposes only its own exact slice, and serialized outcomes retain their ordinary flat lot arrays. Copies of a receipt remain valid after the source draft is dropped.
