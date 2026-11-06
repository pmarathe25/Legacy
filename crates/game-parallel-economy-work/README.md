# Personal requested work

Composes addressed requests, personal willingness and social pressure with physically admitted work. Completion and refusal memories are recorded only for people who were actually present.

Optional, data-defined reusable tools gate each worker before shared input arbitration and throughout a job. Tools remain in personal custody; a missing or expired tool pauses existing work.

A person choosing to work may fetch a missing tool from local stock, subject to physical capacity and their choices about claims. A continuing worker may likewise replace an expired tool; a person who abandons work does not.

Authored physical work sites provide finite slots per location and kind. Job bindings begin only after physical input admission and persist across pauses; completion, interruption, and abandonment release them. Admission is deterministic and conservative.

Action reservations remain sorted and unique even when inputs overlap. Large reservation sets use a parallel merge; smaller sets retain a lower-overhead sort. Run `cargo test --release -p game-parallel-economy-work concentrated_reservation_benchmark -- --ignored --nocapture` with `LEGACY_PEOPLE` and `LEGACY_WORKERS` to compare both approaches on empty, disjoint, and overlapping inputs. `concentrated_exclusion_benchmark` compares ordered set subtraction with per-person lookups.
