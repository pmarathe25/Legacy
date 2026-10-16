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
| `workshop/` | Recipe inputs, output, quantities, duration, practiced skill | `game-workshop` |
| `scenarios/` | Areas with optional climates, people with names/traits/beliefs/values, directed bonds, inventories, permissions, plots, deposits, institutions, optional encounters and reincarnation fixtures | `game-scenarios`, bound by `game-openings` |
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

A scenario's `characters` key selects a profile such as `survival`. Its review and retry intervals, hunger and rest thresholds, interruptible action kinds, switching margin, gathering quantity, and travel effort can be changed in `characters/survival.toml` without rebuilding. Existing saves retain the resolved rules.

## Generated starts and history

The manifest defaults to `generated`. This path reads `worldgen/default.toml`, creates physical areas and independent people from `--seed`, and advances the same authoritative host used during play. It begins without institutions or directed bonds. No player is protected during warmup; every individual uses ordinary autonomous behavior. Player control begins afterward, retaining the resulting people, goods, relationships, groups, actions, and time. A selected person who dies is not replaced to force a successful history.

The default runs two days with 96 mortals and two guardians across 12 natural areas. The generator chooses local biomes from woodland, coast, wetland, highland, and plain definitions, links them with a seeded spanning tree, and supplies wild food, materials, and individually held provisions. Settings and name fragments can be overlaid like other content. `history = 0` starts before any interactions. No historical prose or society is precomputed.

After repeated conversations, people can share supplies informally, learn to value joint decision rights, or follow an admired leader. Voting is adopted only when the participants develop a preference for shared decisions. Ambitious, aggressive people can also make demands backed by actual conflict; others may comply out of fear or resist. Fear changes no affection and creates no ballot. Names come from the proposing character’s familiar people. These initial two-person arrangements retain distinct personal attachments and real supply permissions; larger societies, household care, autonomous enterprises, and multigenerational history still require further behavior.

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

Each occupied area has its own store and explicit permissions. The ruler can use the home store, but permission there does not grant access to distant residents' supplies. Carried packs support foraging. Plots begin fallow; citizens autonomously meet food/rest needs and seek company, while sowing and crafting require explicit actions. Manufactured tools, nets, and mats are real goods and recipe inputs where specified; their tags do not imply equipment bonuses or shelter mechanics.

The five additional climate profiles use four 90-day seasons with different warmth and rain. A scenario area can set `climate = "highland"` to override its default environment. Crops evaluate their area's climate when growth completes. Patches with `renewal_warmth` and optional `renewal_rain` renew once per day only when both thresholds are met; patches without thresholds retain unconditional renewal. `renew = 0` leaves mineral deposits finite. Climate gates do not create weather, water flow, animal populations, or spoilage.

Enable ordinary social life with `social_life = "community"` in `[data]`. People accept optional `name`, `beliefs = { sharing = 10, authority = -5, honesty = 8 }`, and `values = { fellowship = 35, independence = 15 }`. Omitted beliefs and values are neutral. Names for groups receive numbered suffixes. Values stay in -100..=100; `fellowship` affects seeking company and `independence` affects existing foraging choices. Add directed ties separately:

```toml
[[data.bonds]]
observer = "mira"
other = "tala"
relationship = { trust = 35, affection = 40, fear = 0, grievance = 0 }
```

Trust and affection range from -100 to 100; fear and grievance range from 0 to 100. Reverse ties require separate entries. Conversation reserves both actors, permits refusal, and changes impressions separately through beliefs and empathy. Sociability and accumulated fellowship change contact frequency. Normal observations show only currently local names and activity. `talk <person-id>` uses the same timed action and cancellation rules as autonomous company.

## Load and override

```sh
cargo run -p game-cli -- --content ./content --list
cargo run -p game-cli -- --content ./content --scenario kernel-fixture
cargo run -p game-cli -- --content ./content --mod ./mods/harbor --list
```

An overlay follows the same directory layout. A matching relative filename replaces the base document; a new filename adds a document. Repeated `--mod` arguments apply in order, with the last matching path winning. Overlays replace whole documents, not individual fields. Add new catalog files to extend a catalog while retaining its base definitions.

A store caches each document on its first read. New games construct a fresh store. Editing files does not change a running game. Saves contain the resolved definitions and content fingerprints, and loading a save requires no source files. No hot-reload or content scripting engine is provided.

The pre-rewrite documents remain in `/tmp/legacy-before-rewrite-1791992308.tar.gz`; the active files here use the current feature schemas.
