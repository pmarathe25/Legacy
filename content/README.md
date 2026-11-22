# Content

The current simulation reads authored goods and recipes from `objects/` and `workshop/`. Its scenario TOML files live in `scenarios/`; the [runner documentation](../crates/game-economy-cli/README.md) identifies the supported starts and commands. That directory also contains reference scenarios for earlier experimental hosts; those are not necessarily accepted by the current runner. The [content importer](../crates/game-content-import/README.md) documents which object and recipe properties are currently simulated.

The `[writing]` table can list alternate `surfaces`. Each entry names an
output `page` good, a `blank` with a good and quantity, and an optional
`pigment` with its own good and quantity. Use a distinct durable output good
for each surface. A `surface_choices` entry assigns a person's standing
selection by that output name. See [the written works scenario](scenarios/economy-personal-written-works.toml)
for a clay tablet and a charcoal-marked bark sheet.

A region can declare `storage = { timber = 5 }` to give timber an
explicit capacity. The `stock` table gives initial quantities. Each
initial quantity must fit its declared storage. A region with
`storage = {}` can still contain people and connect to routes. Undeliverable
work remains in job custody. Older region tables without a `storage` entry
use the scenario's `stock_capacity` for catalog goods.

Other content directories are retained as source material for future features; their presence does not mean the current engine loads or implements them.
