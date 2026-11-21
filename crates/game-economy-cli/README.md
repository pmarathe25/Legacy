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

### Choosing to read

A personal choice profile can declare domain 25 to compare reading with
other available activities. The item ID is the action key for an individual
page. A person can read a text they actually hold and understand. An
`auto_read` literacy setting provides a standing inclination for people
without that personal choice domain.

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

A person can coin a name for someone they remember meeting. A spoken name can
be interpreted using the listener's existing vocabulary. To learn a new
mapping for someone outside the encounter, the listener needs their own
remembered contact with that person and must choose to adopt the form.

With elapsed gathering enabled, `look` lists reachable resource nodes.
`cultivate FIELD` plants from accessible regional stock or harvests a ripe
field. To plant from an identified seed batch in the character's own cargo,
use `cultivate FIELD CARRIED_SEED_BATCH`. `look` shows accessible fields, their
known fertility and moisture, and the batch identities of carried seed. With elapsed soil
amendments enabled, `amend FIELD` applies the configured material from
reachable stock.
With elapsed irrigation enabled, `water FIELD QUANTITY` draws water
from reachable regional stock.

In a scenario with a pastime catalog, `look` lists activities the character
has the materials and instruments to do. Use `pastime ACTIVITY` to start
and `pastime stop` to stop. The activity takes its authored elapsed duration,
uses the carried materials when it starts, and leaves reusable instruments in
custody. The [pastime scenario](../../content/scenarios/economy-pastimes.toml)
includes song composition, instrument practice, and painting.

`compose SOUNDS SOUNDS ...` writes an ordered sequence of personal forms on
one carried page. Each form must have marks the character already chose.
Paper and ink are consumed only when the physical page can be created.
`read-work PAGE` shows the actual marks and the meanings this character
can recognize while carrying the page. The same marks can have different
meanings for different readers.

In a scenario with personal episodes, `memories` recalls the controlled
character's experiences. `memories PERSON` limits the view to encounters
with that person. Faint recollections can lose their precise dates.
`reflect-memory PERSON` or `reflect-memory activity ACTIVITY` deliberately
rehearses a surviving recollection without changing when it happened.
The [episodic-memory scenario](../../content/scenarios/economy-episodic-memory.toml)
includes a musician whose memory can consolidate after sufficient chosen rest.

With a ready personal itinerary, `trip meet GROUP MINIMUM WAIT_MILLISECONDS`
chooses an appointment for the first leg. The character waits for their
minimum number of nearby travelers on that appointment and route. After
the wait expires they can depart on their next review even if fewer travel.
Use `trip meet off` to clear the appointment or `trip go` to attempt the
leg yourself. Each traveler can separately change their plans.

`gather NODE` takes one yield into personal custody if stock and carrying space
permit. For an engineering scenario, `look` lists carried items and apparatus.
`assemble 7 1001:1:0:1000 1002:2:1001:1000` connects item 1002 to item
1001. The fields are item ID, component ID, parent item ID, and force share
per thousand. A parent ID of zero marks the input. `experiment 7 1002 100 100
10` tests apparatus 7 at part 1002 using 100 units of input force, 100 units
of input travel, and 10 units of resistance.
In the [empirical mechanics scenario](../../content/scenarios/economy-empirical-mechanics.toml),
`pastime 1` is a reflection on previously observed trials. After it completes,
`findings` shows what the controlled character inferred under the recorded
conditions, including comparisons with contradictory results.
Use `mount APPARATUS STORE` to place all parts at a reachable store and `unmount APPARATUS STORE`
to pick them back up. People at that store can choose to use the same
assembled apparatus. A machine serves one trial at a time, and a move
commits only if the destination can take every part.


With elapsed agreements enabled, `promise PERSON GOOD QUANTITY DUE_IN_MILLISECONDS`
offers a future transfer. Parties can see their promises with `look`.
`respond PROMISE accept` and `respond PROMISE decline` are recipient choices.
The person who made the promise can use `deliver PROMISE QUANTITY` or
`respond PROMISE withdraw`. Custody changes when a delivery succeeds.
