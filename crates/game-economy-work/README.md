# Personal requested work

Composes addressed requests, personal willingness and social pressure with physically admitted work. Completion and refusal memories are recorded only for people who were actually present. Dependent-care plans can stage neglect effects separately from publishing their receipts, so the enclosing host can commit them with physical food transfers.

When local walking is enabled, a witness must share the worker's endpoint. A person may ask for work or decline a received request at a store; beginning site work still requires reaching that site.

Optional, data-defined reusable tools gate each worker before shared input arbitration and throughout a job. Tools remain in personal custody; a missing or expired tool pauses existing work.

The tool owner can stage conditions and allocator updates from identified physical production before the enclosing host commits custody. This stages identity metadata only; ordinary work wear and repairs remain separate decisions.

A person choosing to work may fetch a missing tool from local stock, subject to physical capacity and their choices about claims. A continuing worker may likewise replace an expired tool; a person who abandons work does not.

Authored physical work sites provide finite slots per location and kind. Job bindings begin only after physical input admission and persist across pauses; completion, interruption, and abandonment release them. Admission is deterministic and conservative.

Optional `shared_work.succession` gives unfinished work at a site a bounded retention period after its lead departs or becomes incapacitated. Inputs and the site reservation remain attached to the job during that period. Separate per-person preferences identify a job and specify willingness, subjective fear or felt obligation thresholds, and whether to disregard claims. Helpers do not automatically become successors. Only one eligible person is chosen deterministically, spending the takeover boundary before work resumes; old personal requests do not transfer. The successor reconsiders their own preference and feelings toward the former worker as work continues. Expiry returns the inputs through ordinary physical custody, and site collapse interrupts the job. A successor remains free to stop working later.

For example, a scenario can authorize one future decision boundary and give person 3 an independent preference for job 1:

```toml
[shared_work.succession]
retention = 1

[[shared_work.succession.preferences]]
person = 3
job = 1
willing = true
```

The optional `fear_at_least`, `obligation_at_least`, and `disregard_claims` fields are personal choices; absent thresholds do not impose social obligations. Job IDs are physical identities and the preference is specific to that job.

Action reservations remain sorted and unique even when inputs overlap. Large reservation sets use a parallel merge; smaller sets retain a lower-overhead sort. Run `cargo test --release -p game-economy-work concentrated_reservation_benchmark -- --ignored --nocapture` with `LEGACY_PEOPLE` and `LEGACY_WORKERS` to compare both approaches on empty, disjoint, and overlapping inputs. `concentrated_exclusion_benchmark` compares ordered set subtraction with per-person lookups.
