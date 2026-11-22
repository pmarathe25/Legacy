# Content

The current simulation reads authored goods and recipes from `objects/` and `workshop/`. Its scenario TOML files live in `scenarios/`; the [runner documentation](../crates/game-economy-cli/README.md) identifies the supported starts and commands. That directory also contains reference scenarios for earlier experimental hosts; those are not necessarily accepted by the current runner. The [content importer](../crates/game-content-import/README.md) documents which object and recipe properties are currently simulated.

The `[writing]` table can list alternate `surfaces`. Each entry names an
output `page` good, a `blank` with a good and quantity, and an optional
`pigment` with its own good and quantity. Use a distinct durable output good
for each surface. A `surface_choices` entry assigns a person's standing
selection by that output name. See [the written works scenario](scenarios/economy-personal-written-works.toml)
for a clay tablet and a charcoal-marked bark sheet.

`cultivation.seed_genetics.pollen_corridors` can link cultivated regions.
List corridors in destination and source order, using `source`,
`destination`, and `strength` from 1 to 1000. A linked donor contributes
pollen from its ripe fields of the same crop. Local pollen has weight 1000
per seed unit. The [pollen corridor scenario](scenarios/economy-pollen-corridor.toml)
uses a one-way link between two regions.

Other content directories are retained as source material for future features; their presence does not mean the current engine loads or implements them.
