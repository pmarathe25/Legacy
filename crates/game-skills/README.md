# Personal production skills

> Historical note: references below to removed experimental hosts and their fixtures describe the previous implementation. See the [current runner](../game-economy-cli/README.md) for supported scenarios and commands.

Optional personal skills improve a person's work rate after actual completed jobs. A rule can apply to one recipe or to several recipes in an authored craft category; related work and lessons contribute to the same personal record. Successive levels require increasing numbers of completed jobs: with `completions_per_level = 2`, levels one, two, and three require cumulative totals of 2, 6, and 12 jobs. Work advances by `1 + level * work_per_level` units per tick, capped by `max_level`.

An authored rule (or `catalog_training`) may instead set `practice_each_tick = true`. In the lot-based economy, each boundary that actually reduces a job's remaining work counts as one practice credit, even if the job is later abandoned or interrupted. A paused job grants no credit, and its completion does not grant a second credit for the same work. With this option, `completions_per_level` measures practice credits instead of completed jobs. Old data defaults to completion-based learning. The older experimental fused host rejects this option until it can account for partial work.

The host reads committed rates before advancing work, credits only newly finished jobs, and publishes the prepared skills state with the rest of the transaction. Learning therefore affects subsequent ticks. Jobs waiting for output storage do not grant additional experience. Each `(person, recipe, finished_at, job)` attribution is idempotent, including after save/load. The completion time distinguishes new work that reuses an external job ID. Credit belongs to the worker finishing the job; partial work before reassignment is not credited in this initial model. Attribution IDs are discarded at mastery, where all further credit is inert. Training is personal. While a ruler actively invokes past lives, the fused host can use their practiced work rate without copying their training into the current person.

`validate` checks authored/imported state. `index` builds an immutable concurrent rate lookup. `prepare` accepts host-validated completion identities and returns a private next state; unknown recipes without training rules are ignored. Untrained people and unconfigured recipes retain rate one. The catalog importer groups recipes by their authored `skill` field using `[skills.catalog_training]` rates, while explicit per-recipe rules remain distinct. This model covers production skills; genetics modifiers, forgetting, and non-production practice remain open.

Lessons are explicit encounters that consume a tick. A teacher with a higher applicable rate grants one lesson credit; lessons and completed work both contribute to the student's own skill. A teacher may decide to help because of their own fear or felt obligation, even when their normal teaching preference is off. Each lesson records its teacher, student, recipe, tick, and (when used) the invoked past life that supplied the teaching ability. Lesson credit survives dismissal and save/load and does not copy the teacher's skill records.

Enabled `teachers` policies let NPCs teach a weaker person during an already-consenting social encounter. Each encounter produces at most one lesson and uses committed skill levels; a controlled ruler is excluded from automatic teaching choices. The composed campaign includes teachable crafts.

When `inherit_teachers = true`, children of opted-in teachers inherit those teaching roles but no skill. An inherited role becomes eligible at the authored adult age; the child must still learn a higher skill before teaching anyone.

Optional `[skills.recipe_knowledge]` is separate from those skill levels. Each
mind has a bounded, sorted set of specifically known recipes, an ordered list
of recipes it may explain, and its own willingness to share or learn. A real
meeting can communicate one known recipe in each direction, subject to those
personal choices; the host can also supply the teacher's response to fear or
felt obligation. Learning a recipe offers it to the interval host's personal
work chooser, but does not start a job, guarantee the person's capability or
supplies, or imply knowledge of other recipes in the same skill category.
An optional `birth_profile` enrolls newborns with empty knowledge and their
own configured learning inclination. The CLI accepts content recipe names in
the `known` and `share` lists. Someone may also opt to share other recipes they
subsequently learn; this can allow knowledge to pass between generations. The
integrated hourly scenario demonstrates the
configuration. Recipe knowledge currently has no invention, forgetting, or
model of how an individual changes its sharing preferences.
