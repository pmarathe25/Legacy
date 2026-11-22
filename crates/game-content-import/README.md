# production content import

Loads authored object and workshop definitions into the numeric production
catalog. Existing template identities are preserved and new numeric IDs are
assigned in key order for unqualified definitions. New stock endpoints use
the configured capacity. Existing endpoints keep their authored quantities
and capacities when a recipe requires or produces a larger batch.

A workshop recipe specifies one or more named `inputs` and either an
`outputs` table of named goods and quantities or the single-output fields
`output` and `quantity`. Its authored `duration` is in milliseconds. The
importer rounds it up to a whole unit of work. In an interval scenario, that
unit uses `work_rate_period_millis` when configured.

Food definitions retain portions, scaled hunger relief, nutrients, health
effects and shelf life. Unsupported nutrient names are diagnosed when
the content is loaded.

## Namespaced packs

New packs can declare `namespace = "orchard"` and `version = 1` in their TOML
headers. Give each pack a unique `id`, and declare required packs and versions
with `requires = { "objects/orchard" = 1 }`. A namespaced object or recipe uses
its `runtime_id` as its stable numeric ID, and uses names such as
`orchard:pomace` in scenarios. Within a namespaced recipe, an unqualified good
first resolves in that recipe's namespace. The loader rejects duplicate names,
numeric conflicts, missing dependencies, cycles, and unsupported versions.
