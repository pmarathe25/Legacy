# Consolidated simulation audit

This document collects the crate, content extensibility, and character autonomy
recommendations sent to the main session. It is a source review and a follow-up
list. The current behavior and completion of each item should be verified in
code and tests. References are relative to this repository.

Use the [feature restoration inventory](../../docs/research/legacy-feature-audit.md) to track
historical feature coverage and the [elapsed-time migration notes](../../docs/research/elapsed-time-migration.md)
to track rates, deadlines, and behavior under different advance sizes. The
[workspace and content notes](../../docs/research/workspace-and-content-followups.md) and
[autonomy notes](../../docs/research/autonomy-followups.md) record earlier reviews. Use the
[implementation plan](../../docs/plan.md) for confirmed design intent.

## Verified restoration checkpoints

- Personal meetings, language, signals, written explanations, and recipe
  lessons can share an elapsed encounter opportunity. History records the
  actual times. Irregular advances, save/reload, and different settlement
  intervals and worker counts are covered by composed tests.
- Physical storage projects shelf life from the local ambient calendar or a
  holder's explicit temperature. Exact age survives movement and season
  changes. Preparations can mature in custody and stock can spoil before a
  same-time use. Personal construction and selection of storage conditions
  remain open.
- Mechanical designs use actual identified parts. Each character can rank
  alternative input components or a branched design. The mechanical owner
  computes force and travel in physical connection order and records which
  parts break. A character can mount an apparatus in a reachable workshop.
  Other people can then choose to test those same physical parts after reaching
  it. Simultaneous trials arbitrate per apparatus, use the actual decision
  instant, and publish only changed apparatus. Broader parts selection and
  learning remain open.
- Wildlife selects local mates through personal attraction and inherited
  desire. An authored inherited flexibility can give different animals
  different selectivity within one species. The earlier elapsed history lost
  its grazer population after hunting removed six founders. The last two
  mutually compatible pairs found in the annual checkpoints shared one male.
  The remaining animals died without enough offspring to replace them.
- Wild stands support heritable plant strains, viable seed banks, finite
  adult capacity, and local habitat selection without human activity.
  Competing local variants and physical seed withdrawals are tested
  across worker counts, elapsed partitions, and save/reload.
- Conditional aid and work practices arise through individual proposals and
  responses. History preserves attributed proposals, claimed readiness,
  beliefs, and outward adoption with or without approval. More types of
  personal cooperation and a shared elapsed review cadence remain open.
- Voluntary gathering can publish at the personal decision instant. The
  gathered yield enters physical custody before later decisions. The node
  owner arbitrates competing requests and charges only accepted transfers.
  A composed test covers renewal, competing gatherers, exact receipts, and
  save/resume across worker counts and physical settlement intervals.
- People with a cultivation choice profile can plant or harvest on their
  decision clock when the crop owner uses elapsed growth. Shared-field
  competition, actual seed-batch withdrawal, offspring custody, and the
  freedom to decline are covered across save/reload and worker counts.
- Fallow soil recovers from actual elapsed time. A soil treatment only adds
  fertility when its material leaves physical custody. The player's action
  and ordinary personal choices are exercised across worker counts and
  different physical intervals.
- Watering draws from reachable stock and arbitrates by field on the decision
  clock. Ordinary cultivation and requests have an independent elapsed
  review. An individual can refuse a request. The review still records the
  refusal when it consumes no material. Focused tests cover exact review
  times, conservation, saved rounds, and different worker counts.
- Private lexicons support remembered acquaintances. A speaker can name
  someone from their own experience. A listener needs their own grounding
  and can decline or adapt the form. Annual reports count the living people
  who share exact spoken or written forms and show competing forms. A
  one-year integrated archive records adoption, adaptation, and explanations
  that did not result in an immediate adoption.
- Authored pastime catalogs provide song composition, instrument practice,
  and material-based painting. Characters can take up or stop an activity.
  Completion is dated by elapsed time and people can assign it different
  meaning. Tests cover finite material use, retained instruments, interruption,
  and deterministic decisions for 4,096 individual participants.
- Bounded private episodes come from actual meetings and completed creative
  activities. Attention and memory length can differ across people. Recall
  fades by elapsed time and precise dates can become uncertain. Explicit
  reflection and sufficient chosen rest can reinforce accessibility. Tests
  compare save/reload, irregular time partitions, and independent and
  concentrated observations at different worker counts.
- Reading and joining a reserved crew can compete with other personal
  choices. An individual can decline to help even with free crew space.
  Crew capacity is applied after choices. An accepted helper contributes
  only their actual elapsed effort.
- Players can assemble actual carried parts and choose mechanical trials. The
  host rejects missing parts and excessive force. History identifies lead
  workers and helpers from accepted crew starts.
- Agreements can use elapsed deadlines and retry periods. A person can
  offer goods they do not hold, refuse terms, or explicitly withdraw.
  Delivery still needs physical custody and can occur at the last valid
  millisecond. Expiration changes the agreement without moving goods. The
  recipient can change their trust using their own view of fulfillment or
  default. Recipients with opposite responses make different choices after
  the same outcome. Elapsed decision rounds consider only open agreements.
- Annual histories follow the calendar even when the annual checkpoint falls
  inside a physical interval. The resumed state matches the uninterrupted
  run when the caller partitions time differently and changes worker count.
- Two 31-day histories with minute-sized advances and one and six workers
  produced byte-identical checkpoint state. The same experiment still needs
  timed measurements under controlled CPU load and on larger mixed worlds.
- The 5,100-person benchmarks give identical ordered receipts across worker
  counts. The original minute observation test staged 306,000 rows per
  physiology owner. The sparse fatigue stage now skips every unchanged
  fatigue row in this idle case. Vitals still stages 306,000 rows with minute
  advances and 5,100 with one hourly advance.

## Working constraints

- Restore missing high-level features through the elapsed host and its normal
  simulation, player and history entry points. Track unsupported authored
  options alongside feature coverage.
- Separate what an actor perceives and chooses from what a physical owner can
  execute. Physical force, scarce supplies, fatigue, illness, and other
  involuntary consequences can constrain choices. Standing plans can express a
  choice, with the ability to reconsider as circumstances change.
- Cooperation can come from affection, fear, obligation, or other motives.
  Approaches and force can be unwanted. A promise creates a record, and its
  participants can break it. Offices, employment, payroll, institutions, and
  other collective patterns emerge from individual actions and recognition.
- Author content in data. Keep typed requests, events, and validation with their
  feature owners, and keep hot simulation state compact. Evaluate bounded
  alternatives concurrently from immutable views. Resolve only real conflicts
  through the owners of the contended resource, with stable ordering where needed.

## Workspace consolidation

The earlier dependency pass found 103 packages reachable through the CLI's
declared normal or optional path dependencies. It found no obvious orphan on
that basis. These 11 changes are candidates for moving code into a clear owner.
The suggested module names are illustrative.

| Current crate | Candidate owner |
| --- | --- |
| `game-skill-model` | `game-skills::model` |
| `game-opinions-model` | `game-opinions::model` |
| `game-missions-model` | `game-missions::model` |
| `game-tactical-map` | `game-tactics::map` |
| `game-matching` | `game-families::matching` |
| `game-social-pressure` | `game-social::pressure` |
| `game-conflict-receipts` | `game-conflict-packed` |
| `game-production-output` | `game-production` |
| `game-lot-work` | `game-lot-runtime` |
| `game-economy-read` | A read/view module in `game-economy` |
| `game-content-import` and `game-skill-catalog` | One content compiler, with separate modules for their responsibilities |

Recheck current dependency edges and public consumers before each move. Keep
genuinely shared production, lot, and family models separate where they prevent
cycles, and retain independent feature owners, typed APIs, receipts, tests, and
performance characteristics. Crate count is a diagnostic, not a target.
Keep the [feature ownership index](../../docs/design/feature-ownership.md) current as
boundaries change.

One execution path handles elapsed simulation, player actions, and history.
The host now handles personal expedition invitations, independently revisable
departure plans, sourcing, collapsing structures, local tactical reinforcement,
guardian bodies, past-life memories and practiced work rates, and unattended
succession. Travelers can choose to wait for companions before their first
leg or depart at their own deadline. Larger formations, coordination on
later legs, and sharing appointments through invitations remain open. Exercise
them through normal authoring, the player interface, history, and saved state.
The removed runner remains available in Git history.

The interval host now loads site demolition, recovery, and construction for a
person's active recipe goal. Follow up on procuring missing construction kits
as intermediate work, comparing demolition and salvage with other personal
opportunities, and recording construction and recovered material in narrative
reports.

## Content extensibility

| Area | Follow-up and acceptance example |
| --- | --- |
| Content identity and compatibility | Namespaced objects and recipes accept stable runtime IDs. The loader validates pack versions and dependencies and rejects duplicate identities. Persist a catalog manifest with versioned saves and define how a resumed world accepts compatible catalog additions. |
| Import and storage | The [catalog importer](../../crates/game-content-import/src/lib.rs) creates empty stock entries across inventories and goods and can enlarge authored capacity to fit recipes. Allow sparse stock or explicit lazy creation, preserve authored capacity, and diagnose incompatible recipes. Adding unused goods should not allocate stock proportional to the number of inventories. |
| Quantities and lot identity | Represent the physical capacity being consumed, such as volume, mass, or slots, where counting unlike raw quantities gives wrong results. Keep stable batch provenance and expiry for goods that also need individual identity. The [current lot model](../../crates/game-lot-model/src/lib.rs) rejects both combinations for identified items. Cover perishability, splitting, merges, transfer, rollback, and conservation. |
| Production, knowledge, and work | Workshop recipes can author several outputs or byproducts. Separate exact recipe knowledge, a worker's preferences, and the work they pick. Multiple recipes per character and specific recipe teaching now have interval slices. Expand content through those paths. Keep specific recipe knowledge separate from practice in a shared skill category. |
| Reach and apparatus | Regional colocation is a coarse default and mechanically usable parts currently often have to be in one actor's custody. Use shared physical reach and access semantics for installed, carried, and jointly used equipment. Broaden bounded design generation beyond the first compatible component mapping. Preserve identity, wear, breakage, loads, and separate judgments about claims, permission, and cooperation. |
| Nutrition, diets, and crops | Authored food rejects unsupported nutrient names, and interval scenarios can set exact metabolism deadlines. The nutrient types are still fixed. Design an extensible nutrient catalog, wider diets and preferences, multiple seed or crop outputs, and meaningful byproducts. Exercise a new food, diet, and crop archetype defined in content. |
| Genetics and expressive traits | Move past the initial fixed-locus assumptions using data-defined trait definitions and expression, with compact storage chosen for the enabled traits. Keep inheritance, mutation, and selection reproducible and preserve provenance. A new trait should not require a hard-coded system enum for every content pack. |
| Time and system registration | Use explicit units and distinguish simulated time, the caller's advance size, physical deadlines, and decision cadence. Follow the [elapsed-time notes](../../docs/research/elapsed-time-migration.md) for save compatibility and hazard clocks. Reduce central enumerations and fixed-width domain masks where they make independent owner additions costly. Keep typed, batched owner boundaries and use efficient declared schedules or dispatch where useful. |

## Autonomy follow-ups

| Area | Behavior to review and intended direction |
| --- | --- |
| Scheduler and ongoing work | Meetings and delivery can reserve an actor before competing proposals are compared. Reading and starting to help have personal-choice paths. Move remaining discretionary actions into shared evaluation, including an option to idle or keep, interrupt, or abandon an activity. Respect prior commitments as motives and account for setup or interruption costs. |
| Rest and capability | Interval rest and resumption are now personal choices, and elapsed fatigue and exhaustion are physical effects in the integrated path. Keep that behavior, and broaden alternatives while occupied. Check remaining shared age or hunger gates on negotiation, delivery, agriculture, and work. Model relevant ability and risk directly. |
| Meetings and pressure | Known local peers can be preferred by directed affinity, while incidental pairing chooses adjacent people from a seeded local order. Extend bounded deliberate approaches to aversion and goals. Separate an incidental encounter, a deliberate approach, and the receiver's response. Preserve refusal, compliance under pressure, and physical coercion. |
| Teaching and language | Personal choice profiles can independently decline or rank practice lessons during encounters. Extend this to competition with other activities, comprehension, and incidental learning. Preserve exact recipe knowledge separately. Keep phoneme mappings and written marks personal and revisable, with choices about whether to explain, adopt, or modify them. |
| Interpretation and relationships | Ordinary and pressured encounters support signed trust and affinity responses by person and directed relationship. Promise outcomes and help have shared fallbacks, with personal and directed overrides in existing paths. Extend signed interpretation and neutral responses to remaining paths. Preserve the ability to enjoy harm, feel no obligation, distrust a helpful person, or help from real affection, fear, or obligation. |
| Family and learned inclinations | In subjective mating, inherited desire and directed attraction already matter and conception need not require an exclusive relationship. Revisit mandatory kin exclusions and legacy automatic pairing. Separate personally learned aversion, biological consequences, and relationship choices. Review the single reciprocal partner field and the feature policies copied from a parent, retaining genetic dispositions while allowing experience and upbringing to change behavior. |
| Dependent care | Directed personal preferences let parents and non-parents choose care based on affinity, fear, felt obligation, and hunger. Visits, food transfers, and neglect can run on independent elapsed deadlines. Each child can develop a different disposition to feeling obligated from the care they experience. Add discovery of other local children through experience and put visits in the same evaluation as other opportunities. |
| Trade and agreements | Recipients rank physically feasible offers using their own values and social incentives before arbitration. Personal incentives can assign signed weights to affinity, fear, trust, and obligation. Expand bounded known counterparties and decisions based on perceived feasibility. Refusal and withdrawal are available. An actor can claim to have goods or make a promise without verified custody. Actual settlement requires the goods and respects capacity and conservation. |
| Observation and testimony | The stock observer can grant exact regional totals from local access. Missing food-report policy defaults to volunteering, including the default on newborn minds. Record attributable observations and estimates, and let speakers choose what to share and listeners decide how to use it. Preserve purposeful withholding, deception, provenance, and involuntary perception. |
| Meals, search, and travel | The personal choice round ranks individual foods. Optional on-site meals let people choose locally accessible stock portions that their cargo cannot fit. Legacy meal selection takes the first qualifying food. Food search favors reported quantity before distance, may use exact local stock to abandon a search, and can use the authored map directly. Ordinary legs then progress without a travel reconsideration option. Extend personal destination, exposure, and information preferences. Reconsider at real decision points, including between legs. Keep physical progress, route closures, and observation of failures. |
| Purpose | Signed appraisals can interpret completed work and the outcomes of individual choice domains. Deprivation and its physical effects support an elapsed clock. Expand receipt coverage to care, discoveries, and social activities as those move into personal evaluation. Add appraisal development through experience. |
| Animals | Keep individual handling, affinity, restraint, and escape responses. Animal mate selection uses inherited desire, individual preferences, and circumstances. Age, hunger, grazing, and births can follow a separate elapsed ecological cadence. Model physical separation within regions. Preserve ecological pressure, conception, and inheritance. |
| Expeditions, campaign, and tactical reactions | During migration, replace general sociability as the proxy for joining a mission with each participant's mission-specific judgment. Preserve choices to coordinate, split, stop, stay, or return. Keep authored guardian loyalty, supernatural sensing, and player reincarnation mechanics. Make pursuing, mentoring, judging readiness, and responding to requests individual actions. Player lineage does not confer political authority. Review default tactical defense on being targeted as an active decision, while preserving physical defenses, surrender, and withdrawal. |

Source entry points for rechecking these behaviors include the
[interval scheduler](../../crates/game-economy/src/core_interval/host/scheduler.rs),
[rest and fatigue](../../crates/game-economy/src/core_interval/host/fatigue.rs),
[social pairing](../../crates/game-social/src/lib.rs),
[skills](../../crates/game-skills/src/automatic.rs),
[opinions](../../crates/game-opinions/src/lib.rs),
[family phases](../../crates/game-families/src/phase.rs),
[dependent care](../../crates/game-dependent-care/src/lib.rs),
[trade selection](../../crates/game-market-auto/src/lib.rs),
[reporting and memory](../../crates/game-knowledge/src/lib.rs),
[food search](../../crates/game-economy/src/food_search.rs),
[purpose](../../crates/game-economy/src/core_interval/host/purpose.rs),
[fauna](../../crates/game-fauna/src/lib.rs),
[expeditions](../../crates/game-economy-expeditions/src/lib.rs),
[guardians](../../crates/game-economy-campaign/src/embodied.rs), and
[tactical confrontation](../../crates/game-economy-combat/src/confrontation.rs).

## Integration and evidence

1. Migrate by feature slice. Follow the path from authored data through the
   normal host and player/history surface to saved state. Compare with the
   historical feature inventory before removing a runner or owner.
2. Exercise decisions where two individuals in the same circumstances choose
   differently. Include a preferred later offer, an unrelated caregiver,
   opting out of a specific expedition, a recipient refusing an explanation,
   and a character finding purpose outside production. Include a refusal, a
   pressure-motivated action, and a physically prevented action.
3. Check rollback, conservation, save/resume, and deterministic state and event
   ordering across worker counts. For elapsed owners, compare one advance with
   irregular partitions of the same duration, including deadlines and a save
   between them. Test compatibility with older saves where a migration is
   supported.
4. Measure representative mixed worlds as well as independent and concentrated
   workloads. Keep proposals and transient buffers bounded, partition by actual
   contention, and avoid all-pairs searches or shared mutation in actor loops.
   Prefer near-linear scaling in the parallel work and report the remaining
   serial costs or noisy measurements. Keep numeric kernels separable where
   that helps optional future acceleration.
5. Keep the long-running milestone integrated. Run a representative scenario
   with the available features and suitable tick granularity before interpreting
   centuries of output. Stream causal events for narrative histories of people,
   leadership, collective patterns, discovery, and ecological change. Attribute
   observations and claims and preserve uncertainty in the historical view.

