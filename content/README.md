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
| `scenarios/` | Areas, links, people, traits, inventories, permissions, plots, deposits, institutions, optional encounters and reincarnation fixtures | `game-scenarios`, bound by `game-openings` |
| `physiology/`, `characters/`, `health/`, `environment/`, `invocation/` | Named rule profiles selected by scenarios | Respective feature crates |
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

Copy a scenario file, change its document ID, and edit its data to add a start. `fixture = false` makes it eligible for seeded `--scenario random` selection. The shipped starts remain noncanonical fixtures. `foraging-fixture` supplies an autonomous person with an empty carried container and a local edible patch; meals require real gathering and consumption. A group with `count > 1` creates keys such as `citizen-1`; permission selectors accept an exact key, a group key, or `"*"`.

A scenario's `characters` key selects a profile such as `survival`. Its review and retry intervals, hunger and rest thresholds, interruptible action kinds, switching margin, gathering quantity, and travel effort can be changed in `characters/survival.toml` without rebuilding. Existing saves retain the resolved rules.

## Load and override

```sh
cargo run -p game-cli -- --content ./content --list
cargo run -p game-cli -- --content ./content --scenario kernel-fixture
cargo run -p game-cli -- --content ./content --mod ./mods/harbor --list
```

An overlay follows the same directory layout. A matching relative filename replaces the base document; a new filename adds a document. Repeated `--mod` arguments apply in order, with the last matching path winning. Overlays replace whole documents, not individual fields. Add new catalog files to extend a catalog while retaining its base definitions.

A store caches each document on its first read. New games construct a fresh store. Editing files does not change a running game. Saves contain the resolved definitions and content fingerprints, and loading a save requires no source files. No hot-reload or content scripting engine is provided.

The pre-rewrite documents remain in `/tmp/legacy-before-rewrite-1791992308.tar.gz`; the active files here use the current feature schemas.
