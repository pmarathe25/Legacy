# Dated production runtime

A physical production owner executes dated input reservation, person-directed work, interruption, output delivery, expiry, and save/load. The economy composes it with personal decisions and other feature owners. No social agreement obliges a worker to continue.

`prepare_with_growth` accepts harvest actions from a trusted environmental owner. It checks that output is freshly dated and enters regional stock, then arbitrates those actions with all other custody changes. Ordinary external transfers cannot create goods; callers must derive harvest quantities and timing from validated environmental state.

The same typed growth input accepts validated writing actions, which atomically consume paper and ink and create an identified page. The writing owner supplies the page's content separately and commits it only after physical acceptance.

Construction can stage empty `Site` custody with the physical action that fills it. The runtime retires rejected holders within the same draft, and subsequent recovery uses normal lot movements, preserving original expiry and atomic rollback.

`prepare_partitioned_flat_transfers` handles eligible simple actions from different owners in one batch. It assigns the same ordered identities as normal composition before resolving shared endpoints, then returns the original action and movement identifiers in receipts. Active production work and unsupported shapes fall back to the normal runtime.

Movement events own their receipt batch and expose borrowed views through `iter` and `par_iter`; callers can use `into_owned` when receipts must be retained individually. The batch remains usable after the runtime commits or is dropped. Saves contain the committed world rather than transient receipt storage.

Saved work may contain fractional progress from the work owner. Whole-boundary advancement retains that progress and publishes completion through the normal physical transaction. It does not itself advance the rest of the economy by fractional intervals.

`progress_work` advances an already-reserved batch without advancing the physical boundary or publishing completion. Invalid batches are inert, and successful updates invalidate previously prepared physical drafts. The caller must coordinate elapsed time, rates and eventual settlement with the other owners.

For a resumable physical-only interval, `begin_work_interval` freezes the rates and permits only explicitly supported interval mutations; `progress_work_interval` consumes at most the remaining quantum. The interval and its elapsed time survive save/load. `settle_work_interval` publishes a completed interval without applying work twice and retains the interval if settlement fails. A coordinator can stage newborn custody and interrupt deceased workers in the completed interval before committing the physical generation. A coordinator can instead close a complete interval and settle with no additional work rates. Cancellation releases the lock and reports elapsed time while retaining progress. This does not yet coordinate the rest of the economy or expose partial-world time; the economy rejects such snapshots until it has a world-level coordinator.

Within a partial interval, `prepare_early_work` stages completed admitted jobs using no-time custody transactions; `commit_early_work` publishes their outputs into job custody with the opening tick and elapsed offset. Successful and interrupted jobs leave the frozen rates. Capacity-rejected jobs also leave the rates and remain pending for the normal boundary, preventing repeated same-offset retries. Early perishable output is rejected until lot expiry supports finer timestamps. No other physical or economic boundary systems run during this operation.

The ordinary early-work method creates fungible lots. Hosts with identified goods use `prepare_early_work_with_identified` and stage their item-owner metadata from the resulting draft before committing it.

The host can defer completed jobs to boundary settlement without releasing their input custody or publishing their output. Deferred jobs leave the active rate set so other early events can proceed; the host must decide which completions need deferral.

`prepare_interval_settlement` stages a completed physical boundary, including optional identified production, without consuming the interval. Its item updates, produced identities, and next allocator value let an external owner prepare its metadata before validation and commit. A failed or discarded draft leaves the interval resumable.

`start_interval_work` reserves inputs for a batch of host-admitted jobs without advancing the tick. Accepted jobs join the interval with positive frozen rates and zero prior progress; entirely rejected batches leave the state unchanged. The host checks actor availability and resolves other due events before calling this physical-only operation.

The host can stop active work, move goods outside job custody, and stage personal consumption inside an interval. Consumption drafts expose exact staged receipts for other owners to validate before commit; a changed physical generation or elapsed time invalidates the draft. Actor choices, locations, and physiological effects remain the host's responsibility.

Large homogeneous single-input starts use packed transfers when the shared source and new custody support them. Multi-input starts, missing sources, and identity-sensitive transfers fall back to ordinary transactions.

The early operation reports job outcomes rather than individual consumed cohorts. If the no-time custody draft would publish already-due spoilage, preparation fails without mutation; the caller can defer work to ordinary boundary settlement instead.

The ignored `early_work_completion_benchmark` measures completion of 50,000 independent jobs with `LEGACY_WORKERS` controlling concurrency. One run took 81, 51, 35, and 31 ms at one, two, four, and six workers respectively, with identical fingerprints. This isolated workload remains sublinear.

The ignored `concentrated_production_benchmark` accepts `LEGACY_PROGRESS_ONLY=1` to time advancement of already-admitted work and `LEGACY_FRACTIONAL=1` to seed fractional progress. In one 100,000-job run (50,000 admitted), the whole and fractional paths respectively took 175 and 178 ms with one worker, and 70 and 69 ms with six workers. All runs produced the same final fingerprint. `LEGACY_INTERVAL_PROGRESS=1` instead times opening a frozen work interval and progressing it halfway; one run took 3.97 ms with one worker and 1.44 ms with six, with matching fingerprints. These measurements characterize these workloads, not linear scaling of the complete economy.
