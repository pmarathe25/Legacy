# Legacy command line

Run Cargo commands from the repository root.

## Play

```text
cargo run -p game-economy-cli -- --player
cargo run -p game-economy-cli -- --person 1 content/scenarios/economy-core-player.toml
```

`--player` starts the campaign. `--person ID` starts from the perspective
of a controlled character in an authored scenario. Pass a scenario path to
choose a different world and provide `--seed N` or
`--population PATH` to apply either override at creation.

Use `help` for available actions and `look` for the character's view.
`wait MILLISECONDS` advances time. `work`, `eat`, `rest`, `walk`,
and `move` are choices checked against actual supplies and reach. A traveler
can review or abandon their own `trip`. Invited travelers make separate
choices about whether to join and whether to return.

Through `word` and `form`, a character can choose personal spoken sounds
and written marks. `inscribe`, `read`, and `recall` work with real held
pages. Other people have their own vocabulary, interpretations, and
willingness to learn.

`campaign` shows the current incarnation and elapsed invocation state.
`invoke` and `recall-life` make selected past-life memories available.
`dismiss` ends the invocation. If a cost requires a choice, time stops at
its deadline until the player accepts or dismisses it.

## Observe

```text
cargo run -p game-economy-cli
```

Without a player option, the characters make their own choices while you
observe their world. `step` requests one simulated minute. Give `step` a
duration in milliseconds to request a different elapsed step. Use
`status`, `people`, `catalog`, `stock REGION`, and `goods PERSON` to
inspect the world. `help` lists the other available observations. Both
modes use the same elapsed checkpoints for `save PATH` and `load PATH`.

## Record a history

Histories record authored aid and work proposals, individual responses, and
personally received claims of readiness when these practices are enabled.
The archive distinguishes outward adoption from private approval. A readiness
statement remains an attributed claim. The statements and responses can be
read alongside actual work and physical deliveries.


```text
cargo run -p game-economy-cli --profile simulation --example history -- content/scenarios/economy-history-integrated-hourly.toml runs/history/world.ndjson 300 1 60000
cargo run -p game-economy-cli --example inspect_history -- summary runs/history/world.ndjson --narrative
cargo run -p game-economy-cli --example inspect_history -- person runs/history/world.ndjson 61 --narrative
```

The final history arguments are simulated years, workers, and milliseconds
per advance. The example writes a fresh archive, selected observations, and
annual checkpoints. An hour remains the physical settlement quantum for
the integrated scenario. Independent deadlines and rates retain elapsed
precision within that quantum. Use `history --resume CHECKPOINT OUTPUT`
with additional years and worker arguments to continue into a fresh file.

Use `history_probe SOURCE OUTPUT STEPS INTERVAL WORKERS STEP_MILLIS` for
shorter runs and more frequent checkpoints. The inspector accepts
`summary`, `person FILE ID`, and `animal FILE ID`. Without
`--narrative`, it returns structured JSON. Pass `--through-year YEAR` to
report through a complete annual snapshot while the archive is still growing.

## Content and implementation

Scenario TOML files live in `content/scenarios`. Object and recipe content
can use namespaced packs with stable identifiers. The normal CLI and history
examples compile authored data directly into the elapsed host. Game state
and validation live with the feature owners.

See the [feature restoration audit](../../docs/research/legacy-feature-audit.md)
for current coverage and the [elapsed-time notes](../../docs/research/elapsed-time-migration.md)
for owner clocks.
