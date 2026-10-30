# Parallel history runner

This is an independent runner around the current `game-parallel` engine. It does
not supply missing mechanics or import Legacy checkpoints. Existing scenario,
engine and old history artifacts are preserved.

```
cargo run --release -p game-parallel --example history -- --scenario content/parallel/history.toml --workers 4 --output target/history/parallel-history-example
cargo run --release -p game-parallel --example history -- --resume target/history/parallel-history-example --workers 1 --output target/history/parallel-history-resumed
```

The second command resumes an interrupted run's saved target, not an additional
duration. Output directories must be new. `--ticks N` overrides a new scenario's
duration; it is rejected on resume. `--stop-file PATH` commits and stops at the
first completed tick where the file exists. The file is not deleted by the runner.

## Time, scenarios and reproducibility

The wrapper scenario names an engine TOML file relative to its own directory,
specifies the deterministic seed and explicitly defines minutes per tick, days
per year, and epoch year. Rates in the engine scenario remain **per tick**: changing
the calendar changes their physical interpretation; it does not rescale them.
The provided scenario labels a tick as one hour and runs ten days. It is a tiny
smoke fixture, not a realistic population or a completed 300-year experiment.

The runner uses a dedicated Rayon pool. Its worker count is recorded in output
summaries and may change on resume; it does not enter the saved simulation or
sample state. Exact target, cadence, sample cursor, calendar and complete engine
World are saved together. Optional engine-owner fields are preserved by World
serialization, not reconstructed from a selected subset.

## Output and recovery

Each checkpoint is an immutable `checkpoint-<tick>` directory containing:

- `state.json`: complete engine and runner checkpoint.
- `samples.jsonl`: bounded diagnostic snapshots since the preceding checkpoint.
- `events.jsonl`: bounded actual family-owner receipts since the preceding checkpoint.
- `summary.json`: current metrics, calendar, workers and stop/completion status.
- `COMMITTED.json`: schema, tick, lengths and FNV-1a checksums.

Diagnostic snapshot differences are **not** gameplay action events: population
and living deltas do not distinguish births, deaths, migrations or resurrection.
Individual family receipts come from the family owner's event API. Other action
categories currently have no exported receipt stream. Total snapshots never
exceed `max_samples`; receipts never exceed `max_events`, with omitted receipts
counted explicitly. Final metrics and family totals are still written when either
limit is exhausted. Receipt and snapshot cursors persist across resume.

Files are synced into a `.pending-<tick>` directory and published by a directory
rename after the commit marker is synced. Resume scans committed generations,
ignores incomplete pending directories, checks lengths/checksums and validates
the complete state. A corrupted newest committed generation is an error rather
than an implicit fallback to older history. Checksums detect accidental corruption;
they are not cryptographic authentication. Process-crash recovery is supported;
cross-platform guarantees against power loss or unusual network filesystems are
not claimed. Output from an interrupted run and its resumed run forms the same
sample and bounded receipt sequences as uninterrupted execution.

## Feature limits

The initial engine provides regional food growth, individual hunger/health,
deterministic contested meals, shipments and travel. Optional family/genetics
provides aging, partnerships, pregnancy, births, inheritance of diploid alleles,
and deaths; consult its owner documentation for its deliberately simplified
policies. `content/parallel/families.toml` is an accelerated test fixture with
ages measured in ticks, not a calibrated human demographic model. A wrapper
may select it using `source = "families.toml"`. This runner adds no family,
work, social, culture, institutional, political or richer ecology rules. No full
Legacy equivalence or 300-year throughput is claimed. The earlier settlement
prototype remains separately preserved under `game-history-*`.

Run `cargo test -p game-parallel --example history` for exact resume across worker
counts, bounded output, immediate stop, preserved existing output, calendar and
corrupt/incomplete checkpoint tests. Do not mix production timing with builds.
