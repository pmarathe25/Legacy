# History experiments

Run the ordinary assembled simulation from physical founding conditions, with no player or prescribed society. Histories do not accelerate aging or invent replacement births.

## Bounded studies

Use `python3 tools/history/chunks.py plan.json` for long runs. A plan supplies a frozen executable, its SHA-256, output and archive directories, seeds, and optional physical content overlays. `chunk_years` defaults to 20.

Keep generated plans, frozen runtimes, checkpoints, journals, progress files, and analysis reports under the ignored `target/history/` directory. Set the plan's `artifacts` and job `output` paths there. `docs/` is for maintained design and usage documentation, rather than transient experiment output.

After every chunk the supervisor:

1. Verifies parentage, ancestry depth, and every recorded human birth and death.
2. Compresses and checks the compact event journal and annual summaries.
3. Retains a compressed individual snapshot, cohort CSV, and analysis.
4. Publishes the matching checkpoint/cursor pair, then removes raw logs and the preceding checkpoint.

The latest checkpoint remains available throughout the next chunk. An interrupted chunk can be replayed from its preceding verified checkpoint. Output ownership markers prevent reusing unrelated directories. Stops from resource limits are censoring, never biological extinctions.

Rerun the same plan command to continue after a stop; completed chunks are reused. Remove the archive directory's `stop-request` file first if you requested the stop. The run horizon can be extended, but the generation target and chunk size must remain the same. Executable changes require the verified optimizer upgrade described below. Progress rows show the last verified chunk, rather than live population estimates.

The production plan uses 20-year chunks. `chunk_days` is available for short developer replay tests. `--generations 20` requires twenty actual reproductive steps through the shallower parental branch; years elapsed and surviving guardians do not count.

## Compact journals

Compact logging is the default. It retains all human births and deaths, social milestones, chosen names and personal forms of address, notable objects and losses, and first occurrences of routine experiences. Repeated meals, trades, symptoms, and disputes update totals without writing every occurrence. Ecological records are sampled annually per patch, with up to four newly observed plant genomes. Ungerminated seed and fruit families are counted without individual entries. Witness lists are capped at four; total witness counts remain available.

During a long chunk, `python3 tools/history/progress.py PLAN.json` reports the
latest verified checkpoint and a separate live clock lower bound from the active
journal. It reads at most 64 KiB, ignores unfinished rows, and keeps population
counts and generation depth labeled as verified. A quiet journal can lag behind
the simulation clock; live progress is never checkpoint or ancestry evidence.

For an intentional behavior change, `fork.py SOURCE_PLAN DESTINATION --executable
BINARY --reason TEXT --validation RESULT_JSON` preserves a stopped study's
population, complete journal prefix, and immutable origin checkpoints in a new
experiment. It records the old and new binary hashes and each phase's starting
day and generation. The original study remains available as a control. Optional
`--days N` extends the horizon. The source must have a stop request and release
its supervisor lease first. Equivalent performance upgrades continue to use the
stricter replay checks in `runtimes.py`; model forks explicitly change behavior.

Event counts, production totals, population and climate snapshots, and the latest feeding context at death remain available. Plant samples describe distributions; they are not a complete plant pedigree. Human parentage and demographic evidence remain complete. Private developer records do not become character knowledge.

New cultural transmissions retain `works.shared` with the speaker, listener, work, and medium. Repeated transmission to the same listener does not produce another record. Snapshot `cultural_reach` counts oral memories, text presentations, works with an audience, and the widest audience. These measure recorded exposure; they do not establish comprehension of an entire work or agreement with it.

Use `--journal full` only for a short diagnostic run requiring individual plant-family records and detailed witnesses. Changing logging does not change simulation decisions.

## Standalone runs

```sh
cargo run --profile simulation -p game-history -- \
  --seed 1 --days 7300 --workers 1 --sample-days 365 \
  --checkpoint-days 7300 --output target/history/seed-1
```

Options take separate names and values, for example `--seed 1`. `--content` selects content; repeated `--mod` arguments overlay it. `--resume previous-directory` loads the saved world and matching journal cursor. The saved seed wins. `--final-save false` avoids making a duplicate of the checkpoint; chunked studies use this option automatically.

The checkpoint includes pending work and individual state. `--reload-day` checks a save/load round trip across worker counts. Every committed transaction is collected, including large flowering updates; `--advance-budget` groups work without skipping collection between commits.

Biological years last 365 days; authored climate years currently last 360. Small scenarios retain their controlled fixture clocks. See the [physical profile guide](profiles/README.md) for experimental habitats and their limits.

## Analysis and verification

`python3 tools/history/analyze.py manifest.json output-directory` independently reconstructs human ancestry and reconciles births and deaths. A segment can name a raw directory or a compressed `journal` path, with optional exclusive `after` and inclusive `through` sequence bounds. Chunked studies write the manifest automatically.

`mortality.py` reads compact demographic evidence. `patterns.py` describes recorded social, ecological, and personal-value patterns. Recorded intermediate exchange is not proof that a monetary system formed. Trait/reproductive associations do not separate selection from drift, family preferences, or culture; young cohorts remain censored.

`LEGACY_PROFILE=1` adds inclusive feature timings and failed-plan reasons to summaries. Compare matched conditions when measuring performance. The legacy `study.py` supervisor remains available for older experiment plans; prefer chunked studies for new work.

The profile separates history loading, advancement, journal collection, checkpoints, JSON export, and snapshot construction. Feature spans include character review, barter, food planning, observation, physiology, wild growth, and plant genetics. Nested spans overlap; do not add them as independent costs. The summary cannot include the time spent writing itself. Profiling changes no simulation state.

For a performance comparison, freeze both executables and replay the same completed checkpoint:

```sh
python3 tools/history/compare.py \
  --checkpoint target/history/source --days 1095 \
  --control target/history/control --candidate target/history/candidate \
  --pairs 3 --workers 1 --output target/history/comparison
```

The end day is absolute. The driver disables instrumentation, alternates execution order, records wall time, CPU time, and peak RSS, and checks complete world, ledger, journal, and individual-snapshot hashes. World JSON hashes use canonical object field order; arrays and every value remain significant. Ledger, journal, and individual snapshots require exact bytes. Reports declare the hash mode; older byte-hash evidence remains supported. A mismatch retains both outputs and fails the comparison. It retains the last matching pair and small measurements, pruning earlier duplicate worlds. Run comparisons without concurrent builds or simulations. Keep measured results under `target/history/`; usage guidance belongs here.

Run `cargo test -p game-history` and `python3 -m unittest discover -s tools/history/tests -p 'test_*.py'`. Replay checks compare complete worlds and journals across worker counts, advancement batches, and checkpoint continuation.

For sustained studies, `cargo build --release -p game-history` uses the workspace’s thin LTO profile. The `simulation` profile remains useful for faster iteration. Freeze the selected executable before beginning a chunked study; do not change its binary mid-run.

## Verified optimizer upgrades

A long study can keep its exact population and journal prefix when applying a behavior-preserving optimizer. Stop through `stop-request` and wait for the supervisor to exit after verifying its current checkpoint. Freeze the new executable and run three alternating replay pairs with `compare.py` against every continuing population’s current checkpoint. All four outputs must match: world, ledger, journal, and individual snapshot.

Set `runtime_upgrade` in the revised JSON plan to `{"from_sha256": "<previous binary hash>", "comparisons": {"<job name>": "<comparison.json path>"}}` alongside the new root, executable, and hash. Paths can be absolute or relative to the new root. The supervisor validates the evidence, checkpoint hashes, and journal cursors; backs up the prior study state; and records both runtimes and evidence hashes. It preserves generations, days, checkpoint, and all archived segments. Earlier chunk ownership markers retain their original hashes. Remove `stop-request` before continuing. Rejected evidence leaves the study unchanged.

The supervisor holds an exclusive process lease to prevent two invocations from writing one study. Optimizer replay evidence is specific to the tested checkpoints; it is not a proof for every possible world. Changed decision policies or biology belong in a separately identified experiment.

Set `chunks_per_invocation` to a positive number to return after that many verified chunks, making room for inspection before continuing. Omit it for continuous supervision. Current studies declare `reproductive_window: {"lifespan_fraction": [2, 3]}` so cohort censoring follows expressed natural lifespan. Legacy manifests without this metadata retain their original fixed age-45 interpretation. This analysis metadata describes the frozen mechanics; it does not alter fertility.

`--distress true` writes a separate diagnostic `distress.jsonl` for urgent hunger. It records up to eight samples per continuous episode, at least two days apart, including current intentions, journeys, carried food, bulky goods, local physical food stocks, and caregiver context. It reads developer state and grants characters no information. Keep this mode to short investigations; normal histories leave it disabled.

A study can set `continue_after_extinction: true` to observe the physical world through its requested horizon after all mortals die. Standalone histories accept `--continue-after-extinction true`. Extinction is still recorded in summaries and mortality evidence; no replacement population is introduced. This option extends ecological observation and does not turn extinct worlds into successful human lineages.
