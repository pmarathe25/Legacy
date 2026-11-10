# Transactional lot custody

Parallel preparation of physical lot movements and expiry. Claims are arbitrated against one boundary snapshot; accepted moves carry the exact earliest-expiring cohorts, and publication is atomic. The current economy composes this owner with its individual decisions.

Lots can optionally carry a stable physical item ID. An identified lot contains exactly one nonexpiring item; IDs are unique among live custody and survive ordinary transfers. The custody owner tracks them through staged changes even when receipt details are omitted. A movement can request a particular item or use normal FEFO selection. Mixed requests at one source reserve cohorts in deterministic order; rejected transactions do not reassign their reserved cohorts during that boundary. Transactions touching an identified item use the general arbiter; unrelated fungible transfers can use the specialized paths. The ignored `independent_identified_transfers_benchmark` in `tests/identified_items.rs` measures batches with `LEGACY_ITEMS` and `LEGACY_WORKERS`; set `LEGACY_EXACT=1` to select each item explicitly.

`stock_view` returns only regional stock at the requested current or next boundary, using the same expiry rules and revision as a full view. Observation systems that need regional totals can avoid materializing every person's custody.

`Draft::take_receipts` moves outcomes and spoilage to a detached consumer without publishing custody. A later commit still checks the draft generation and applies the same physical changes; the drained receipt lists are empty in the commit result.

`cargo run --release -p game-lots --example contention -- 1 100000 source 5` times a concentrated source; use `destination` for a concentrated destination and vary the worker count.

`prepare_transactions` stages a person's physical action as an all-or-nothing set of movements and output creations. An output receives its expiry when actually created, and remains in its output custody if later storage is unavailable. Claims are conservative: a rejected leg rejects its entire transaction, and capacity tentatively claimed by a rejected transaction is not reallocated until a later boundary. Callers must record the transaction outcomes and should retry according to each person's own decision; the owner does not create social agreements or enforce promises. Created lots with an expiry must have a deadline strictly after the boundary.

`prepare_flat_transfers` accepts independently atomic single movements in a flat batch. It returns no fast-path draft when endpoints contend or the batch needs general handling; the caller must then submit normal transactions to the general arbiter. It preserves dated cohorts, boundary spoilage, atomic drafts, and the existing save format.

Prepared outcomes retain dated lots in batch-owned arenas. `take_outcome_batch` moves that storage to a composed owner, whose borrowed receipt views stay valid for the lifetime of the batch. `outcomes` and `take_receipts` materialize independently owned outcomes for callers that need them. Receipt arenas are transient and do not change custody serialization.

When independent source and destination ranges are each ordered and disjoint, changed containers remain in ordered chunks through publication. Custody additions and retirements can materialize the chunks when they need a flat index; empty requests leave them intact. Other layouts continue through the general sorted-change path.

`prepare_partitioned_flat` also separates transfers whose endpoints are exclusive from those that share a source or destination. The shared group uses the general arbiter with the original action identities and seed. Both groups remain one atomic draft, with expiry applied once. Unsupported layouts still return no specialized draft.

The normal transaction path can similarly isolate exclusive single-leg transfers when other actions consume goods, create lots, contain multiple legs, or use newly staged custody. An entire complex transaction stays in the general arbiter, and every source and destination it touches excludes overlapping transfers from the fast group. Transfers involving new custody also stay in the general group. The ignored `mixed_atomic_action_benchmark` compares both paths; `LEGACY_ATTEMPTS` and `LEGACY_WORKERS` select the workload and worker count, and `LEGACY_ADDITIONS` stages new custody.

On the 10,000-person mixed campaign, replacing sorted contested-action IDs with parallel flags reduced the instrumented classification phase from 0.570 to 0.489 ms per boundary on one worker and from 0.352 to 0.306 ms on six in one repeated comparison. Fingerprints matched; whole-run timings overlapped. A mostly independent 100,000-transfer benchmark also showed overlapping timings.

Set `LEGACY_IDLE_HOLDERS` on `mixed_atomic_action_benchmark` to add untouched custody. With 5,000 transfers and 500,000 idle holders, computing expiry-aware occupancy only for destinations reduced general preparation from about 13.1 to 11.5 ms on one worker and from 5.9–6.2 to 5.2–5.3 ms on six in repeated runs. Every run compares receipts with the unpartitioned arbiter; the mixed campaign's whole-run timings overlapped.

Add `LEGACY_IDLE_EXPIRY=1` to make idle custody expire at the boundary. In a 5,000-transfer run with 100,000 expiring idle holders, assigning global expiry to the general partition reduced partitioned preparation from 65–69 to 42–45 ms on one worker and from 23–24 to about 20 ms on six in repeated comparisons. Receipts and spoilage matched the general arbiter, including with added custody. In one 24-boundary mixed campaign repeat, one, two, four and six workers took 622, 502, 424 and 434 ms respectively, with matching fingerprints; whole-world scaling remained sublinear.

Reusing one expiring-container list for spoilage and materialization reduced general preparation in a 5,000-transfer, 500,000-idle-holder run from 11.2 to 8.9 ms on one worker and from 5.2 to 4.7 ms on six. The expiry-heavy run showed similar timings before and after; these measurements are isolated workloads.

Set `LEGACY_REVERSE_TRANSFERS=1` to exercise nonordered endpoints in the mixed atomic benchmark. Reusing the partitioner's validation reduced partitioned preparation of 100,000 reversed transfers from 89–91 to 84–85 ms on one worker and from 34.9–35.7 to 33.4–33.6 ms on six in repeated runs. Receipts matched the general arbiter.

`LEGACY_COMPLEX_STRIDE=3` makes every third transaction multi-leg. For 100,000 reversed actions, borrowing the general partition instead of cloning its transactions reduced preparation from 194–198 to 187–189 ms on one worker in repeated runs; six-worker results overlapped at roughly 56–59 ms. Untouched identified items are preserved when the custody owner validates the batch.

The shared-source path also accepts validated empty new destinations, including unused added holders. It preserves the general arbiter's source-grant order even when a destination lacks capacity, and maps changes into the merged custody order. Unsupported shapes fall back to general arbitration.

For the 49,999-job admission workload, merging already sorted custody runs instead of sorting their concatenation reduced the instrumented one-worker location phase from about 8.7 to 5.9 ms in one repeat. Six-worker total timings overlapped, and fingerprints matched.

Independent transfers keep moved cohorts in immutable chunk-owned receipt arenas. Each outcome exposes only its own exact slice, and serialized outcomes retain their ordinary flat lot arrays. Copies of a receipt remain valid after the source draft is dropped.

Independent transfers merge already ordered dated cohorts in linear time. Work is divided into fixed logical chunks that also expose parallelism in smaller, cohort-heavy batches. The ignored `many_cohort_transfer_benchmark` accepts `LEGACY_ATTEMPTS`, `LEGACY_COHORTS`, and `LEGACY_WORKERS` to measure this path.
