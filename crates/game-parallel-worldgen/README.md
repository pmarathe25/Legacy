# Parallel world generation

An authored settings file selects a rules template, seed, region and population counts, route durations, founding food and logs, field counts, and optional infection and traders. `trading_percent` chooses opted-in traders, each starting with `trader_items` units of one physical good and valuations favoring the other. The generator creates deterministic people, genomes, dispositions and ambitions, a connected bidirectional region ring, local production and farms, and compatible owner state. It validates the resulting world before returning it. See `content/parallel/worldgen.toml` and the template it names.

`requesting_percent` seeds a personal willingness to ask for help among non-traders, and `helpful_percent` gives a seeded fraction of people a positive value for helping with an uncompensated request. Actual requests, refusals, custody, and learned responses still arise through simulation actions. Both percentages default to zero.

`work_requesting_percent` and `work_helpful_percent` independently seed personal dispositions to ask for or perform work for others. `work_request_interval`, `work_request_lifetime`, and `work_effort` configure the cadence, expiry, and subjective cost. An ask requires an actual active goal; the recipient decides based on their own values, fear, and obligation, and production still consumes real materials. All dispositions default to zero percent and the controlled person is excluded from automatic decisions.

`work_disregard_percent` independently seeds a willingness to use claimed inputs when responding to a request. Such use remains a real physical reservation, and a present claimant can witness it; neither a claim nor a request forces the recipient's choice.

`claim_tolerance_percent` seeds people who do not lose trust merely because they witness use of their own claimed stock. Other people's reaction uses the template's default; these are personal assessments, not physical barriers.

Run `cargo run -p game-parallel-block-cli -- --player --generate content/parallel/worldgen.toml --seed 42`. Omitting `--seed` uses the seed in the settings file. The template path is relative to that file.

To measure the generated scenario with multiple owners enabled, set `LEGACY_WORKERS`, `LEGACY_PEOPLE`, and `LEGACY_TICKS`, then run `cargo test --release -p game-parallel-worldgen generated_world_benchmark -- --ignored --nocapture`. It reports tick preparation and commit separately; generation, import, and export are outside the timed interval.
Set `LEGACY_HOT=1` to place the founding population in one region and exercise concentrated interactions and births. Set `LEGACY_FARMING=1` to benchmark explicit farm volunteering at 50%. Set `LEGACY_DEPARTURES=1` and `LEGACY_TICKS=1` to measure a concentrated wave of autonomous departures competing for scarce food provisions. Add `LEGACY_HELD=1` to start travelers with existing carried food.

For rough profiling, `LEGACY_PROFILE_OFF` can disable one of `social`, `market`, `farming`, `disease`, `knowledge`, or `goals`. These runs produce different worlds and are diagnostic comparisons, not exact attribution of the full run's time.

The headless client supports `--list`, `--scenario ID`, and `--random --seed N` using `content/parallel/scenarios.toml`. A selected seed also controls subsequent simulation choices. `--player` defaults to the composed campaign when no source is provided.



Enabled teacher recipes in the template are assigned to generated workers, who may teach only after actually developing stronger skills. Existing skill records and lessons are cleared for a new world.

With a template whose agriculture owner has `volunteers = []`, `farming_percent` assigns a seeded fraction of founding people to volunteer for farm work. It defaults to zero, and the player-controlled founder is excluded. Template-person consent is not copied to generated people.

When the template enables autonomy, generated citizens receive staggered personal review policies; the player-controlled founder is excluded. `reporting_percent` seeds a deterministic fraction of citizens with a witnessed founding report about an adjacent region (default zero). These are observations of the initial food amount and can become stale; later remote changes are not revealed automatically.

The default `content/parallel/worldgen.toml` uses individual food access, physical stores, dependent feeding, and individually configured asking, sharing, claims, and confrontation tendencies. `content/parallel/legacy_food_worldgen.toml` retains the regional-food comparison profile.

Set `LEGACY_PERSONAL_FOOD=1` when running the ignored generated-world benchmark to include this experimental food path in integrated timings.
Set `LEGACY_NUTRITION=1` to include the default nutrition rules in the same benchmark.
Set `LEGACY_CATALOG_SKILLS=1` with `LEGACY_PERSONAL_FOOD=1` to include the catalog's shared craft skills.

Preset bodies and preset civic organization settings are rejected. Individuals may cooperate, assert claims, share resources, and react to experiences without a predefined institution.

`social_pressure_percent` seeds individual preferences for responding to an approach when their own fear or felt obligation reaches `social_fear_threshold` or `social_obligation_threshold`. They may disable or enable the preference; children may inherit an initial preference but never the parent's experiences. A pressured encounter records the affected person and does not itself increase that person's trust.

`store_builders_percent` seeds independent personal intentions to build once using `store_builder_blueprint`, an authored material good, quantity, and capacity; `store_build_interval` sets how often they consider the intention. Only a person with the actual carried material and an available action can complete construction. No organization, ownership authority, or permission is created.

`reconsider_partnership_percent` seeds personal inclinations to leave when a reproductive partner is no longer interested or fertile. Each person's choice is independent, does not require the other person's approval, and can be changed; newborns inherit an initial inclination from their mother. A controlled person does not make this decision automatically.

When `objects_catalog` and `recipes_catalog` are set, the file loader reads those paths relative to the profile and adds the authored object and workshop catalogs to the generated world's production definitions. `catalog_tick_millis` converts recipe durations to ticks, and `catalog_stock_capacity` sets the capacity of new empty stocks. The imported subset models physical ingredients, outputs, duration, and named skill categories. With `[skills.catalog_training]`, practice on related recipes contributes to the same personal craft skill. Edible goods preserve nutrients, health effects, and shelf-life metadata, and their authored portions and scaled hunger relief can be consumed. Food nutrients and health effects take effect on the next tick. The default profile enables personal nutrient reserves and deficiency damage. Spoilage, tools, and special work requirements are not yet simulated. The imported definitions are saved with the world.
