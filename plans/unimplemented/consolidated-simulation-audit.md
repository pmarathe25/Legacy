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

## Working constraints

- Preserve high-level features, content, and access through the normal simulation
  and player entry points. Restore missing behavior ahead of retiring the
  whole-boundary runner. The interval runner still rejects some authored owners.
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

The larger simplification is one execution path. Migrate campaign, missions,
combat, confinement, and the [other paths rejected by the interval loader](../../crates/game-economy/src/core_interval/host/scenario.rs)
before removing the whole-boundary runner. Include authoring, normal CLI/player
access, history, and saved state in the migration. Exercise the feature through
the interval host before deleting its old driver.

## Content extensibility

| Area | Follow-up and acceptance example |
| --- | --- |
| Content identity and compatibility | Give content definitions stable namespaced keys and explicit dependency, override, and schema-version rules. Compile to compact runtime IDs. Persist a catalog manifest and stable ID mapping with versioned saves. Adding an unrelated pack must preserve existing IDs in a resumed world, or produce an explicit compatibility/migration result. Reject unknown versions and ambiguous overrides. |
| Import and storage | The [catalog importer](../../crates/game-content-import/src/lib.rs) creates empty stock entries across inventories and goods and can enlarge authored capacity to fit recipes. Allow sparse stock or explicit lazy creation, preserve authored capacity, and diagnose incompatible recipes. Adding unused goods should not allocate stock proportional to the number of inventories. |
| Quantities and lot identity | Represent the physical capacity being consumed, such as volume, mass, or slots, where counting unlike raw quantities gives wrong results. Keep stable batch provenance and expiry for goods that also need individual identity. The [current lot model](../../crates/game-lot-model/src/lib.rs) rejects both combinations for identified items. Cover perishability, splitting, merges, transfer, rollback, and conservation. |
| Production, knowledge, and work | Allow authored multiple outputs and byproducts where the runtime already supports them. Separate exact recipe knowledge, a worker's preferences, and the work they pick. Multiple recipes per character and specific recipe teaching now have interval slices. Expand content through those paths. A lesson in a shared skill category must not grant knowledge of every recipe in that category. A learned recipe must not create a job by itself. |
| Reach and apparatus | Regional colocation is a coarse default and mechanically usable parts currently often have to be in one actor's custody. Use shared physical reach and access semantics for installed, carried, and jointly used equipment. Broaden bounded design generation beyond the first compatible component mapping. Preserve identity, wear, breakage, loads, and separate judgments about claims, permission, and cooperation. |
| Nutrition, diets, and crops | The current fixed nutrient names can silently discard unrecognized authored entries. Validate unsupported definitions and design for an extensible nutrient catalog. Permit wider diets and preferences, multiple seed or crop outputs, and meaningful byproducts. Test a new food, diet, and crop archetype defined in content. |
| Genetics and expressive traits | Move past the initial fixed-locus assumptions using data-defined trait definitions and expression, with compact storage chosen for the enabled traits. Keep inheritance, mutation, and selection reproducible and preserve provenance. A new trait should not require a hard-coded system enum for every content pack. |
| Time and system registration | Use explicit units and distinguish simulated time, the caller's advance size, physical deadlines, and decision cadence. Follow the [elapsed-time notes](../../docs/research/elapsed-time-migration.md) for save compatibility and hazard clocks. Reduce central enumerations and fixed-width domain masks where they make independent owner additions costly. Keep typed, batched owner boundaries and use efficient declared schedules or dispatch where useful. |

## Autonomy follow-ups

| Area | Behavior to review and intended direction |
| --- | --- |
| Scheduler and ongoing work | Early phases such as meetings, delivery, reading, and help can reserve an actor before competing proposals are compared. Move discretionary actions into personal evaluation, including the option to idle or keep, interrupt, or abandon an activity. Respect prior commitments as motives and account for setup or interruption costs. Ordinary work and shared work already have some continuation or withdrawal support. |
| Rest and capability | Interval rest and resumption are now personal choices, and elapsed fatigue and exhaustion are physical effects in the integrated path. Keep that behavior, broaden alternatives while occupied, and retire the automatic rest thresholds with the whole-boundary path. Check remaining shared age or hunger gates on negotiation, delivery, agriculture, and work. Model relevant ability and risk directly. |
| Meetings and pressure | Ordinary pairing chooses adjacent people from a seeded local order once they are generally willing. Use bounded targets that reflect personal affinity, aversion, fear, and goals. Separate an incidental encounter, a deliberate approach, and the receiver's response. Preserve refusal, compliance under pressure, and physical coercion. |
| Teaching and language | Skill lessons currently award the learner credit based on the teacher's policy and a skill gap. Include the learner's attention, interest, comprehension, or an incidental-learning path. Preserve exact recipe knowledge separately. Keep phoneme mappings and written marks personal and revisable, with choices about whether to explain, adopt, or modify them. |
| Interpretation and relationships | Ordinary encounters can grant shared positive trust. Promise outcomes and help have shared fallbacks, with personal and directed overrides in existing paths. Extend signed interpretation and neutral responses to remaining paths. Preserve the ability to enjoy harm, feel no obligation, distrust a helpful person, or help from real affection, fear, or obligation. |
| Family and learned inclinations | In subjective mating, inherited desire and directed attraction already matter and conception need not require an exclusive relationship. Revisit mandatory kin exclusions and legacy automatic pairing. Separate personally learned aversion, biological consequences, and relationship choices. Review the single reciprocal partner field and the feature policies copied from a parent, retaining genetic dispositions while allowing experience and upbringing to change behavior. |
| Dependent care | The dedicated care path considers only recorded parents, and food fetching treats claim compliance as required. Allow personally motivated non-parent caregivers and choices about contested food where physically accessible. Preserve finite supplies, harm from neglect, and pressure-motivated care. |
| Trade and agreements | Automatic trade qualifies offers using personal value, then selects the lowest feasible offer ID on conflicts. Rank options by the actor's preference before physical arbitration. Let fear or obligation have individual weights, use bounded known counterparties, and provide practical refusal and withdrawal. An actor can claim to have goods or make a promise without verified custody. Actual settlement still requires the goods and respects capacity and conservation. |
| Observation and testimony | The stock observer can grant exact regional totals from local access. Missing food-report policy defaults to volunteering, including the default on newborn minds. Record attributable observations and estimates, and let speakers choose what to share and listeners decide how to use it. Preserve purposeful withholding, deception, provenance, and involuntary perception. |
| Meals, search, and travel | Legacy meal selection takes the first qualifying food. Food search favors reported quantity before distance, may use exact local stock to abandon a search, and can use the authored map directly. Ordinary legs then progress without a travel reconsideration option. Extend personal meal, destination, exposure, and information preferences. Reconsider at real decision points, including between legs. Keep physical progress, route closures, and observation of failures. |
| Purpose | Work completion counts as progress for every enrolled worker, and deprivation can cause health damage. Let personally meaningful work, care, discovery, social activity, or resisting an unwanted commitment affect purpose according to the character's own appraisals. Preserve consequences and allow neutral responses. |
| Animals | Keep individual handling, affinity, restraint, and escape responses. Breeding currently samples an eligible mate by region and species. Use bounded encounters and individual or species dispositions where animal behavior is modeled. Respect physical separation, and retain ecological pressure, conception, inheritance, and matching behavior across both fauna execution paths. |
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

