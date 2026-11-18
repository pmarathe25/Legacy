# Purpose need

> Historical note: references below to removed experimental hosts and their fixtures describe the previous implementation. See the [current runner](../game-economy-cli/README.md) for supported scenarios and commands.

Optional individual purpose deprivation is data-defined. Time and failed goal attempts increase deprivation; real goal progress relieves it. At the configured threshold, health damage applies to living people. These rules are separate from hunger, fatigue, and social isolation.
Newborns enrolled by the family owner start this need at biological adulthood; an authored `eligible_at` can set a different first active tick. `content/scenarios/purpose.toml` supplies a runnable example.
An interval scenario can give any enrolled person an `appraisal`. Its signed
`completed_work` value replaces the shared relief from finishing a job. Its
`actions` list is sorted by personal-choice domain. Each entry supplies signed
`accepted`, `denied`, and `stopped` values that the person applies to actual
outcomes. Positive values relieve deprivation, negative values increase it, and
zero leaves it unchanged. For example, someone can find choosing to rest
meaningful (`domain = 21`), while another can find it frustrating. A person's
unlisted activity has zero effect when they have an explicit appraisal.
With `rules.elapsed_period_millis`, the `per_tick` deprivation amount and
configured health consequences use multiples of that elapsed interval. Work
completions still contribute to the work outcome settled by the host.
