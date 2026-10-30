# Transactional lot custody

Parallel preparation of physical lot movements and expiry. Claims are arbitrated against one boundary snapshot; accepted moves carry the exact earliest-expiring cohorts, and publication is atomic. The owner is under development as the foundation for integrating perishable goods with the fused host; existing aggregate inventories do not yet use it.

`cargo run --release -p game-parallel-lots --example contention -- 1 100000 source 5` times a concentrated source; use `destination` for a concentrated destination and vary the worker count.

`prepare_transactions` stages a person's physical action as an all-or-nothing set of movements and output creations. An output receives its expiry when actually created, and remains in its output custody if later storage is unavailable. Claims are conservative: a rejected leg rejects its entire transaction, and capacity tentatively claimed by a rejected transaction is not reallocated until a later boundary. Callers must record the transaction outcomes and should retry according to each person's own decision; the owner does not create social agreements or enforce promises. Created lots must have a deadline strictly after the boundary. Existing scalar food is outside this owner and has no expiry.
