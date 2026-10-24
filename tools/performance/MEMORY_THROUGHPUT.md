# Incremental memory updates and bounded inquiry queries

This follow-up preserves the ordinary decision policy on main
`3bb8963d522b87873b4d6e4aa83cbb1f6e8024bb`. It removes work inside the existing
owner transactions. Attention floors and decision frames remain separate opt-in
behavioral experiments.

## Changes

`game-memory-state::Book` uses ordered `Records` with individually shared traces.
Editing one trace copies that trace and the table path; neighboring traces no
longer copy their cue strings and associations. Checkpoints and nested rollback
retain the existing journal/root protocol. The logical trace map and save schema
are unchanged.

Transaction validation checks changed traces. `game-facts::Facts::same_keys` is a
process-local proof that a fact key set is unchanged. Payload and stamp refreshes,
including scene-to-history transfers, retain it; additions and removals invalidate
it. When it differs, an ordered merge verifies the complete fact/trace key join.
Small trace edits use point queries; larger edits use a hash join for association
membership. Deleting traces still checks unchanged traces for dangling links.
Deleted fact rows are tracked separately from changed rows, closing an existing
incremental-validation gap. Full load/save validation checks the complete resource.
No cache or sharing token enters gameplay state or save files.

Observation batches skip crowding work when the entire book is already below the
relevant limit. Rehearsing unchanged encoded memories avoids constructing cues
and interest values that the rehearsal branch discards. Retention rankings,
association order, timestamps and strength calculations are unchanged.

Inquiry applies cheap progress checks before querying empirical evidence. Its
eligibility query needs two valid observations, so it stops after two instead of
decoding the complete history. Actual inference keeps the same bounded, ordered
working set. Lossless empirical observations compile into typed measurements at
ingestion/load; queries borrow them and clone only selected observations. Unknown
JSON extensions retain the opaque decoding path. This is the same principle as
bounded existence queries and incremental view validation in database engines.

## Complete-state comparison and save dictionaries

Typed empirical observations can share save dictionary entries with equal typed
measurements. This changes archive-local indices and duplicate payload counts,
without changing the JSON payload or personal evidence metadata. The initial
strict `canonical-world-v1` comparison correctly rejected this representation
change. Its incomplete report is retained in `memory-delta-exact/`.

The explicit `canonical-evidence-v2` mode normalizes only the version-1 personal
memory dictionaries: it sorts/deduplicates identical keys/JSON values and remaps
only the first two indices in each evidence row. It retains every distinct value,
every row and row order, all source/time/revision/confidence fields, trace metadata,
scheduler IDs, pending jobs, and every other world field. Invalid references and
unknown archive versions are rejected. Events, ledger and people remain byte-exact;
raw file hashes are also retained. Existing studies still default to version 1.
Tests verify that changed payloads, unused distinct values, stamps, row order,
traces and unrelated world fields cannot pass this normalization.

The first seed-19 replay had 2,122 control dictionary entries and 1,981 candidate
entries. Its complete normalized world and all three byte-exact artifacts match.
`memory-delta-dictionary-check.json` records this check against the retained full
control profiling output; the profiling time is not used as timing evidence.

## Reproduce

```sh
python3 tools/history/attention.py \
  --binary target/history/optimization/memory-delta-pgo/game-history \
  --reference-binary target/history/optimization/attention-journal-pgo/game-history \
  --models ordinary-reference ordinary --cases heldout-19 heldout-37 scale \
  --rounds 2 --verify --hash-mode canonical-evidence-v2 \
  --output NEW_RESULT_DIRECTORY
```

Control and candidate alternate order. Each continuation has a fixed unchanged
input checkpoint and end day. Candidate four-worker replays also reload halfway
through the 30-day histories; the one-day large case checks workers without an
integer-day midpoint. Timing includes loading, advancing and exporting. Complete
artifact checks run outside the timed child. Do not overlap timing with builds,
profiling, tests or other simulations.

Both builds use portable O3, thin LTO, one codegen unit and the same transferred
PGO profile. New paths have not received fresh instrumented training. Frozen
source and runtime provenance are `memory-delta-source.tar.gz` and
`memory-delta-runtime.json`; the preceding engine's provenance remains unchanged.
All retained evidence lives under `target/history/optimization/`.

## Measured effect

Two alternating one-worker timings per engine and case gave:

| Continuation | Control median s | Candidate median s | Less wall time |
| --- | ---: | ---: | ---: |
| Seed 19, days 300–330 | 27.592 | 23.253 | 15.7% |
| Seed 37, days 300–330 | 46.194 | 34.935 | 24.4% |
| 1,000 founders, days 1–2 | 69.088 | 57.226 | 17.2% |

CPU reductions were 15.8%, 24.4% and 17.2%, respectively. These are combined
effects of this change set, not per-component attributions. Complete normalized
worlds and byte-exact journals/ledgers/people match across both engines, repeats,
four-worker replays and the long-history midpoint reloads.

The seed-19 wall ranges were 26.151–29.032 s control and 21.960–24.547 s candidate;
seed 37 was 44.501–47.886 s versus 34.656–35.215 s; the large case was
64.922–73.254 s versus 56.242–58.210 s. Two samples establish a useful gain on
these workloads, not a precise universal speed estimate. Verification timings
are excluded from these medians because they change workers and sometimes reload.

Results are in `memory-delta-logical-exact/comparison.json`; audited provenance,
measurements and validation checks are summarized in `memory-delta-findings.json`.
The evaluation sources and tests are archived in `memory-delta-evaluation.tar.gz`.

Peak RSS medians were essentially unchanged: 76.9→75.8 MiB, 90.9→91.0 MiB and
336.3→337.3 MiB. There is no useful memory reduction claim from these samples.

## Validation and remaining costs

The final checks passed 179 targeted Rust tests, five composed care/frame/loading
tests, the architecture/source-budget check, affected-crate/history Clippy with
warnings denied, formatting, and 49 Python history-tool tests. New tests cover
key-token lifecycle, same-count key replacement, corrupt trace fields, association
deletions that affect unchanged traces, large trace edits, deleted fact rows,
rollback, lossless typed promotion and dictionary-normalization boundaries.

A separate post-timing seed-19 profile reproduced the same complete artifacts.
The sampled memory checkpoint-validation share fell from about 9.1% to 2.9%; its
`valid_book` portion fell from about 8.3% to 2.1%. These profiles are diagnostic,
not accepted wall-time comparisons. Broad reconsideration still contains about
85% of sampled CPU work, so further large gains should target decision-provider
work and its batching rather than assuming that an ECS replacement alone fixes
transaction costs. Profile files are `memory-next-profile.er` (control) and
`memory-delta-profile.er` (candidate), with inclusive tables alongside them.
