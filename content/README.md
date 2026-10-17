# Content store

Game definitions live in this directory and load at runtime. Editing or adding a definition needs no recompilation when it uses existing mechanics. Each document has an `id`, positive revision `version`, and a `[data]` section. Durations use milliseconds; quantities and masses use integer native units.

## Ownership

The single store keeps shipping, modding, and references together. Feature crates define their own schemas, validate their own rules, and retain resolved definitions in their saved state. `game-content` provides file discovery, typed TOML loading, overlays, document snapshots, and fingerprints; it contains no game rules.

| Choice | Benefit | Cost |
| --- | --- | --- |
| Data shipped inside each crate | Each feature package carries its defaults | A game or mod must coordinate several roots and their references |
| One namespaced content store | One distributable pack, one override order, and stable cross-feature keys | Feature schemas must remain owned by their crates to avoid a central rules module |

Legacy uses the second option. A feature's tests compose only its required crates; they can also supply a small content pack independently.

| Namespace | Definitions | Schema owner |
| --- | --- | --- |
| `objects/` | Object keys, stable IDs, names, mass, tags, edible portions and relief | `game-objects` |
| `traits/` | Trait names, ranges, defaults, weights on existing utility terms | `game-traits` |
| `agriculture/` | Crop types, seed and output objects, yield, growth, labor, climate thresholds | `game-agriculture` |
| `livelihoods/` | Personal stock targets, retry intervals, trait affinities, and learned concepts | `game-livelihoods` |
| `pastimes/` | Activities, consumed inputs, reusable possessions, time, interests, and skills | `game-pastimes` |
| `workshop/` | Recipe inputs, output, quantities, duration, practiced skill | `game-workshop` |
| `tools/fixtures/content/scenarios/` | Areas with optional climates, people with names/traits/beliefs/values, directed bonds, inventories, permissions, plots, deposits, institutions, optional encounters and reincarnation fixtures | `game-scenarios`, bound by `game-openings` |
| `physiology/`, `characters/`, `health/`, `environment/`, `invocation/`, `social-life/` | Named rule profiles selected by scenarios | Respective feature crates |
| `worldgen/` | Population, natural areas, biome resources, initial individual variation, naming fragments, and live-history duration | `game-worldgen`, run by `game-openings` |
| `manifest.toml` | Default scenario selection | `game-scenarios` |

## Extend the game

Add a `.toml` document directly to a catalog namespace. Catalogs discover all documents in stable filename order. Object and trait names are data keys, not Rust enum variants. Crop and recipe definitions reference objects by key. Scenarios reference actors, areas, inventories, and crops by stable keys. Unknown references, duplicate keys or object IDs, invalid quantities, and malformed fields are rejected.

For example, `objects/berries.toml` adds an edible object:

```toml
id = "objects/berries"
version = 1

[data.objects.berries]
id = 6
name = "Berries"
mass = 2
food = { portion = 2, relief = 50 }
```

Existing eating and carrying mechanics use those properties. A crop can reference `output_object = "berries"`, and a recipe can reference `output = "berries"`. Retain an object's numeric ID when editing its label or properties; IDs must remain unique within the pack.

A new trait can weight `need`, `goal`, `value`, `effort`, or `risk`. Scenario person groups assign scores by trait key. All omitted scores use the authored default. Adding a trait needs no trait-specific code branch; adding a new behavioral effect requires its mechanic to be implemented.

Copy a scenario file, change its document ID, and edit its data to add a start. `fixture = false` makes it eligible for seeded `--scenario random` selection. Authored shipped starts remain noncanonical fixtures; the default `generated` start uses simulated history. `foraging-fixture` supplies an autonomous person with an empty carried container and a local edible patch; meals require real gathering and consumption. A group with `count > 1` creates keys such as `citizen-1`; permission selectors accept an exact key, a group key, or `"*"`.

A scenario's `characters` key selects a profile such as `default`. Its review and retry intervals, hunger and rest thresholds, interruptible action kinds, switching margin, gathering quantity, and travel effort can be changed in `characters/default.toml` without rebuilding. Existing saves retain the resolved rules.

## Generated starts and history

The manifest defaults to `generated`. This path reads `worldgen/default.toml`, creates physical areas and independent people from `--seed`, and advances the same authoritative host used during play. It begins without institutions or directed bonds. No player is protected during warmup; every individual uses ordinary autonomous behavior. Player control begins afterward, retaining the resulting people, goods, relationships, groups, actions, and time. A selected person who dies is not replaced to force a successful history.

The default runs two days with 96 mortals and two guardians across 12 natural areas. The generator chooses among 18 local biome compositions, links them with a seeded spanning tree, and supplies wild food, materials, and individually held provisions. Settings and name fragments can be overlaid like other content. `history = 0` starts before any interactions. No historical prose or society is precomputed.

After repeated conversations, people can share supplies informally, learn to value joint decision rights, or follow an admired leader. Voting is adopted only when the participants develop a preference for shared decisions. Ambitious, aggressive people can also make demands backed by actual conflict; others may comply out of fear or resist. Fear changes no affection and creates no ballot. Names come from the proposing character’s familiar people. These initial two-person arrangements retain distinct personal attachments and real supply permissions; larger societies, household care, regional commerce, and multigenerational history still require further behavior.

Use `--scenario rivermeet`, `--scenario bracken-coast`, or another authored ID for optional controlled starting conditions. `--scenario generated` explicitly selects the default history path. Saved generated worlds retain their resolved definitions, generation settings, seed, and simulated start time.

## Communities and livelihoods

Three community fixtures add 18 places and 27 individually authored citizens alongside the ruler and guardians:

- `rivermeet`: a river green, fertile fields, alder woodland, a warm reed marsh, a cold quarry ridge, and a landing. Mira welcomes company and values sharing; Oren prizes independence and authority. Their mutual distrust differs in strength, while Mira and Tala begin with an affectionate but asymmetric bond.
- `bracken-coast`: a harbor, coastal gardens, damp woodland, tidal reeds, an exposed headland, and a fishing beach. Local stores emphasize fish; flax, reeds, clay, and timber support weaving and pottery.
- `ashridge`: a highland hall, sheltered terraces, pine woodland, a dry plain, an iron-bearing cut, and a warm spring. The terraces and spring have different climates from the surrounding highlands. Ironworking uses finite ore and renewable wood, while low warmth or drought can suspend wild regrowth.

`objects/livelihoods.toml` adds 36 foods, seeds, fibers, minerals, and manufactured goods. Seven new crop definitions produce roots, beans, barley, rice, flax, herbs, and fruit. Sixteen recipes connect the supplies through physical inputs and timed work. For example:

```text
grain or barley -> flour -> bread
roots + beans + timber -> vegetable stew
flax -> cloth; herbs + cloth -> medicine
timber -> charcoal; ore + charcoal -> iron -> iron tools
reeds -> rope -> fishing net
clay + fuel -> pottery or bricks
```

Each occupied area has its own store and explicit permissions. The ruler can use the home store, but permission there does not grant access to distant residents' supplies. Carried packs support foraging. Plots begin fallow; citizens in these authored fixtures autonomously meet food/rest needs and seek company, while sowing and crafting use explicit actions. Generated history additionally enables autonomous production, horticulture, care, and pastimes. Manufactured tools, nets, and mats are real goods and recipe inputs where specified; their tags do not imply equipment bonuses or shelter mechanics.

The five additional climate profiles use four 90-day seasons with different warmth and rain. A scenario area can set `climate = "highland"` to override its default environment. Campaign crops integrate suitable and lethal weather across their growth; controlled fixtures retain their accelerated growth behavior. Patches with `renewal_warmth` and optional `renewal_rain` renew once per day only when both thresholds are met; patches without thresholds retain unconditional renewal. `renew = 0` leaves mineral deposits finite. Climate profiles do not simulate water flow or individual animal populations; preservation and exposure now consume regional temperatures.

Enable ordinary social life with `social_life = "community"` in `[data]`. People accept optional `name`, `beliefs = { sharing = 10, authority = -5, honesty = 8 }`, and `values = { fellowship = 35, independence = 15 }`. Omitted beliefs and values are neutral. Names for groups receive numbered suffixes. Values stay in -100..=100; `fellowship` affects seeking company and `independence` affects existing foraging choices. Add directed ties separately:

```toml
[[data.bonds]]
observer = "mira"
other = "tala"
relationship = { trust = 35, affection = 40, fear = 0, grievance = 0 }
```

Trust and affection range from -100 to 100; fear and grievance range from 0 to 100. Reverse ties require separate entries. Conversation reserves both actors, permits refusal, and changes impressions separately through beliefs and empathy. Sociability and accumulated fellowship change contact frequency. Normal observations show only currently local names and activity. `talk <person-id>` uses the same timed action and cancellation rules as autonomous company.

## Regional content and daily life

The complete shipping catalogs now contain **238 goods, 37 crops, 171 recipes, 18 biome compositions, 16 climate profiles, and 24 pastimes**. The regional expansion adds 166 goods, 29 crops, 132 recipes, and ten climates. Object IDs remain stable; each catalog document can be independently overlaid.

**Regions:** Generated places include alder woodland, tidal coast, reed fen, iron highland, dry grassland, beech grove, chalk downs, copper foothills, mild estuary, willow floodplain, warm scrub, desert oasis, monsoon thicket, northern bog, fruit vale, red earth basin, rainforest margin, and mountain meadow. Each combines a climate, primary wild food and material, and several independent deposits. Minerals are finite; renewable resources have authored replenishment and optional warmth/rain gates. Shellfish and fish are gatherable resource stocks; individual animals, predation, and reproduction are not simulated.

A biome's optional `food_stock_per_person` and `food_renew_per_person` control primary wild food abundance (600 and 400 by default). Renewal cannot exceed stock capacity. The generation multiplier is the rounded-up founding population per area plus two; it stays fixed as descendants are born. `material_renew_per_person` controls primary material replenishment (zero by default). Additional resources use their own values:

```toml
[[data.biomes.resources]]
object = "nettles"
stock_per_person = 60
renew_per_person = 20
minimum_warmth = 4
minimum_rain = 2
```

These quantities scale with the configured local population estimate. They do not grow with later immigration or promise a fixed supply per person. Growth is capped by each patch's physical capacity. `renew_per_person = 0` makes an additional deposit finite.

**Production:** New chains include spinning and weaving several fibers; seed saving; regional grain, pulse, fruit, and vegetable dishes; copper, tin, and bronze; lime, mortar, glass, roof tiles, and mosaics; dyes, paper, ink, and written pages; lamps, candles, incense, tea, instruments, clothing, soap, and medical supplies. Seed-saving recipes consume harvested produce to make planting stock. Successful crops therefore support another generation of planting through actual work.

Generated history enables five small decision owners:

| Owner | Decision | Consequence |
| --- | --- | --- |
| [Livelihoods](../crates/game-livelihoods/README.md) | Choose individual stock goals from traits, learned interests, and feasible local ingredients | Gather resources and execute bounded recipe chains |
| [Horticulture](../crates/game-horticulture/README.md) | Begin and tend a planting when seed and climate suit the person | Pay sowing and harvesting labor through agriculture |
| [Care](../crates/game-care/README.md) | Help a locally injured person according to empathy, affection, and grievance | Consume medicine and caregiver time; successful care can build gratitude |
| [Wayfaring](../crates/game-wayfaring/README.md) | Choose visits through remembered roads | Physically discover places, spend time there, and attempt to return |
| [Pastimes](../crates/game-pastimes/README.md) | Choose an activity from personal interests and available possessions | Use supplies and time, practice skills, and develop individual interests |

Each uses the existing idle-review signal and common action clock. Stock goals do not assign occupations. Caregivers, cultivators, craftspeople, and performers are recognizable through their accumulated actions, goods, and skills. The current player receives explicit control; ordinary hunger and fatigue still take priority in NPC reviews. Autonomous ingredient plans stay local and use one inventory; interregional production needs travel and trade behavior beyond this expansion.

`livelihoods/daily.toml` supplies per-object targets and priorities. The `affinity` names a trait; `concept` names a learned personal interest. Missing resources make a goal infeasible rather than creating inputs. Cycles, excessive recipe depth, capacity limits, stale observations, revocation, and interrupted work retain physical consequences.

Pastimes load every document under `pastimes/`. For example:

```toml
[data.activities.play-reed-pipe]
name = "Play a reed pipe"
inputs = {}
requires = { reed-pipe = 1 }
duration = 600000
interval = 14400000
affinity = "creativity"
concept = "music"
skill = "music"
```

`inputs` are spent at the beginning; `requires` stay reusable and must remain available through completion. A completed activity changes only its participant's skill, interest, and private participation count. Performance audiences, musical influence, shared rituals, and the contents of written works need further mechanics. Cloth and tools are physical goods and recipe inputs; the equipment owner reads warmth and weatherproof tags, while defense and work-speed bonuses require their own mechanics.

Use `craft <recipe> <inventory>`, `plant <crop> <inventory>`, and `pursue <activity> <inventory>` in the CLI. `plant` atomically creates a plot and begins sowing; a failed attempt leaves no plot behind. `status` includes the player's own completed pastimes.

## Load and override

```sh
cargo run -p game-cli -- --content ./content --list
cargo run -p game-cli -- --content ./content --scenario kernel-fixture
cargo run -p game-cli -- --content ./content --mod ./mods/harbor --list
```

An overlay follows the same directory layout. A matching relative filename replaces the base document; a new filename adds a document. Repeated `--mod` arguments apply in order, with the last matching path winning. Overlays replace whole documents, not individual fields. Add new catalog files to extend a catalog while retaining its base definitions.

A store caches each document on its first read. New games construct a fresh store. Editing files does not change a running game. Saves contain the resolved definitions and content fingerprints, and loading a save requires no source files. No hot-reload or content scripting engine is provided.

The pre-rewrite documents remain in `/tmp/legacy-before-rewrite-1791992308.tar.gz`; the active files here use the current feature schemas.

## Woodland, tide, and longer histories

`objects/woodland-and-tide.toml` and `workshop/woodland-and-tide.toml` add 30 goods and 22 recipes: woodland fruits and preserves, lichen and kelp foods, maple sap and syrup, pinecone fuel, moss paper, berry ink, small sculptures, masks, dolls, and other crafts. Matching biomes supply the raw resources through the existing patch rules. Four more pastimes use the new writing and decorative materials.

[The history runner](../tools/history/README.md) records daily summaries and a journal of significant committed events. Generated starts enable voluntary wayfaring. When familiar food sources fail, people may search along remembered roads and learn about supplies on arrival. Cultivators pause sowing while enough of that crop remains in local accessible stores. Finite personal ground caches can free carried capacity through physical transfers; these preserve goods and permissions.

Campaign profiles coordinate physiology and deliberation with crop and life-history time. Trait definitions distinguish ordinary baselines from high ceilings; the shipping ability traits have a baseline of 1000 and a maximum of 16000. Personality preferences retain their authored ranges. `satisfaction_span` in pastimes controls how recent satisfaction fades, rather than making the activity unavailable. `shelf-days:N`, `warmth:N`, and `repair-cloth` object tags connect food and clothing to their owning systems.

## Seasonal biology and storage

The catalog now contains 321 object definitions, 67 crops, 219 recipes, 18 biomes, and 16 climate profiles. `agriculture/diverse.toml` adds greens, roots, legumes, dryland grains, tropical plants, fruit trees, and nut trees. Profiles vary in development time, yield, heat/cold/rain tolerance, regrowth, and lifespan. Campaigns use `natural_growth` and `natural_yield`; old accelerated fixture growth remains separately authored. Cuttings carry the `clonal` tag. Seed saving preserves the actual plant lineage, including raw cotton bolls before ginning.

Food values are simulation units per portion: `nutrients`, `shelf_days`, and `effects` distinguish satiation, dietary composition, health, mood, alertness, and dependence. First experience records coarse personal estimates; other characters do not receive that knowledge automatically. Regional temperatures and built stores rescale remaining shelf life; moving old food does not reset its age. Salt curing and fermentation use inedible prepared batches with `matures`, `maturation-days`, and temperature tags. Preparation takes labor; maturation proceeds while the character does other things. The current fermentation approximation checks temperature at completion.

`warmth`, `weatherproof`, `shelter-insulation`, `hearth-heat`, and `fuel-hours` tags compose clothing, built indoor areas, and finite fuel. Heavy coats can overheat their wearers. A cellar uses its outdoor annual mean climate, rather than gaining refrigeration from a magical tag. Room fires consume real wood or charcoal and eventually go out. Food storage, clothing, and shelter decisions still use the prototype decision scheduling described in the autonomy plan.

No good is designated money. Local offers can propose unequal quantities; actors learn resale expectations from completed exchanges they personally experienced. Particular objects can also acquire sentimental identity and value. These identities refer to real cargo units, not extra stock or a permission that makes the object untouchable.

The generated game uses `physiology/default.toml`, `characters/default.toml`, and `social-life/default.toml` for ordinary human-scale timing and needs. Experimental physical overlays belong under `tools/history/profiles/`.

Accelerated physiology and decision profiles, authored test starts, and fixture invocation costs live in `tools/fixtures/content/`. Production content contains one normal rule profile per feature. `Store::fixtures()` or an explicit overlay opts into the developer pack.
