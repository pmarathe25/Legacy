# production content import

Loads authored object and workshop definitions into the numeric production
catalog. Existing template identities are preserved and new numeric IDs are
assigned in key order. Inventory stocks are physical and start empty.

A workshop recipe specifies one or more named `inputs` and either an
`outputs` table of named goods and quantities or the single-output fields
`output` and `quantity`. Its authored `duration` is in milliseconds. The
importer rounds it up to a whole unit of work. In an interval scenario, that
unit uses `work_rate_period_millis` when configured.

Food definitions retain portions, scaled hunger relief, nutrients, health
effects and shelf life. Unsupported nutrient names are diagnosed when
the content is loaded.
