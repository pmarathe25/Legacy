# Dated production runtime

A physical production owner executes dated input reservation, person-directed work, interruption, output delivery, expiry, and save/load. The economy composes it with personal decisions and other feature owners. No social agreement obliges a worker to continue.

`prepare_with_growth` accepts harvest actions from a trusted environmental owner. It checks that output is freshly dated and enters regional stock, then arbitrates those actions with all other custody changes. Ordinary external transfers cannot create goods; callers must derive harvest quantities and timing from validated environmental state.

The same typed growth input accepts validated writing actions, which atomically consume paper and ink and create an identified page. The writing owner supplies the page's content separately and commits it only after physical acceptance.

Construction can stage empty `Site` custody with the physical action that fills it. The runtime retires rejected holders within the same draft, and subsequent recovery uses normal lot movements, preserving original expiry and atomic rollback.

`prepare_partitioned_flat_transfers` handles eligible simple actions from different owners in one batch. It assigns the same ordered identities as normal composition before resolving shared endpoints, then returns the original action and movement identifiers in receipts. Active production work and unsupported shapes fall back to the normal runtime.

Movement events own their receipt batch and expose borrowed views through `iter` and `par_iter`; callers can use `into_owned` when receipts must be retained individually. The batch remains usable after the runtime commits or is dropped. Saves contain the committed world rather than transient receipt storage.

Saved work may contain fractional progress from the work owner. Whole-boundary advancement retains that progress and publishes completion through the normal physical transaction. It does not itself advance the rest of the economy by fractional intervals.

The ignored `concentrated_production_benchmark` accepts `LEGACY_PROGRESS_ONLY=1` to time advancement of already-admitted work and `LEGACY_FRACTIONAL=1` to seed fractional progress. In one 100,000-job run (50,000 admitted), the whole and fractional paths respectively took 175 and 178 ms with one worker, and 70 and 69 ms with six workers. All runs produced the same final fingerprint. These measurements characterize this workload, not linear scaling of the complete economy.
