# Game content

TOML files describe authored definitions. Each file has a stable `id`, a positive
`version`, and a `[data]` section validated by the feature that uses it. Keep IDs
stable when editing a definition; increase the version when publishing a changed
content pack.

`human/` defines physical predispositions, temperament influences, experience
effects, newborn dispositions, reproductive allele interpretation, and genetic
transmission probabilities. `environment/temperate.toml` defines the calendar
and climate. `agriculture/crops.toml` defines crop types under stable IDs. Genes use opaque locus names; their owning features supply their
meaning. Removing a physical or temperament influence also removes its locus
from new worlds.

Run `game-cli --content PATH` to use another content directory with the same
layout. `--environment PATH.toml` selects a particular calendar/climate document.
Files are read when constructing the world; editing them needs no rebuild.
Saves retain the definitions and fingerprints used for that world, and loading a
save does not require the original files.

Adding a definition supported by an existing mechanic is content work. Adding a
new mechanic still requires Rust code. Other authored goods, recipes, and
opening populations are still being moved out of their current code definitions.
