# Characters, needs, and autonomous decisions

**Status:** Individual citizens with needs, personality, relationships, skills, dreams, goals, and plans are confirmed. The behavioral machinery below is a proposed design. The ruler and guardians have the exceptions defined in [their design](ruler-and-guardians.md).

## Character state

Every character has a stable identity, birth date, life stage, physiology, location or transit state, health, needs, skills, personality, beliefs, relationships, knowledge, possessions and claims, memberships, obligations, goals, plans, and current action.

Keep enduring traits separate from temporary states. Hunger is not personality; fear of a guard is not support for the government; knowing a farming technique is not owning tools that make it possible.

Named people remain individual records at the target late-game population of approximately 10,000. Use sparse social data, indexed opportunities, due reviews, and parallel batches as defined in [performance](performance.md). A household or military unit can coordinate its members without erasing them. Do not silently reduce people's decision quality or replace them with anonymous totals when the CPU is busy.

## Needs and physiology

Support hunger, fatigue, physical safety, health, shelter or environmental comfort, social connection, and a sense of agency or purpose. Introduce their effects in stages, but reserve separate concepts rather than combining everything into happiness.

Each need specifies its units, baseline rate, action and environmental modifiers, decision thresholds, impairment thresholds, recovery rules, and possible health consequences. Use a central rules record and integer arithmetic. Exact rates are balance parameters, not scattered literals.

Hunger is relieved by actual consumption. Fatigue is relieved by suitable rest. Shelter modifies exposure and sleep quality. Work, injury, illness, equipment, age, and species can change these rates. Guardians have no hunger need but can become tired and injured.

Severe deprivation first changes priorities and capability, then damages health and can cause death in mortal characters. Define interruption and recovery thresholds separately to prevent repeatedly stopping and resuming at the same value. Food unavailable through an easy lawful action must trigger a search for alternatives rather than infinite futile eating attempts.

Medical care uses people, knowledge, supplies, access, and time. Recovery can occur naturally or through treatment. A healer cannot cure someone merely because a treatment task exists in another area.

## Development, aging, and death

Birth produces a new character with caregivers, a household or equivalent social context, inherited and developing traits, and dependency needs. Parents, guardians, communities, or institutions must actually provide care and supplies. The reincarnating ruler uses this same physical childhood model.

Life stages change available actions, physical capability, learning, dependency, and how others treat the person. Legal adulthood can differ between institutions; physical maturity is a biological progression. The precise ages and species lifespan depend on the setting decision.

For the ruler, the guardians' judgment determines the transition out of mentorship. That assessment is separate from biological maturity and legal status; neither a birthday nor a government decree automatically makes a guardian obey.

Mortals can die through deprivation, illness, injury, or age. Death stops actions, releases reservations, leaves physical possessions where they are, and creates social and institutional consequences. Estates, dependents, positions, debts, and enterprise continuity are resolved through existing relationships, terms, and institutions.

Do not apply those mortality rules to guardians. Ruler death additionally starts the reincarnation process, but does not undo the ordinary person's death or estate consequences.

## Skills and learning

Skills describe practiced capability; knowledge describes learned facts, recipes, places, or techniques. Training requires relevant practice or instruction and has diminishing returns appropriate to its difficulty. Fatigue, tools, injury, teacher ability, and attention affect results.

Ordinary work can improve a skill while also producing output. Use explicit attribution to avoid counting a minute twice as exclusive work and full-time study. Apprentices can trade labor for teaching and support through [agreements](economy.md#labor-arrangements).

Keep personal learned skill separate from temporary capability supplied by a past incarnation. Accessing an old mastery should not silently overwrite the new person's development.

## Personality, values, and relationships

Retain discipline, ambition, sociability, risk tolerance, and hedonism from the original plan. Add the minimum further traits needed to model empathy, aggression, honesty, and patience without giving each desired behavior its own bespoke trait.

Values cover social beliefs, aspirations, loyalties, and obligations. Relationships are directed and can be asymmetric. A citizen can fear a ruler, distrust their promises, and still rely on their protection.

People can acquire values attached to new practices, groups, and ideals through modeled experience and social learning. Store shared concept definitions separately from each person's attachment, confidence, and evidence. Acquiring a value must affect later goals or decisions; a new label alone is insufficient. Use the [bounded social invention model](society.md#bounded-social-invention).

Use authored variation for initial scenarios. Personality affects tradeoffs; it does not prescribe one action forever. A risk-tolerant person still prefers a safe good option over a pointless dangerous one.

## Dreams, goals, and plans

Use three levels:

| Level | Function | Example |
| --- | --- | --- |
| Dream | Enduring aspiration that weights outcomes. | Achieve independence. |
| Goal | Concrete desired state with progress and a reason. | Secure a livelihood the character controls. |
| Plan | Current feasible approach, with dependencies. | Negotiate land use, obtain tools, recruit help, establish a farm. |

People can pursue several compatible goals and suspend one for survival. Failure should change a plan before arbitrarily changing the underlying dream. Goals can succeed, become irrelevant, conflict, or be abandoned.

Start ordinary survival and livelihood behavior with planning templates that have explicit preconditions, costs, actions, and outcomes. Examples include obtain a meal, secure shelter, learn a craft, earn compensation, start an enterprise, find protection, gain office, settle a grievance, and migrate. Extend this with bounded proposal construction and reasoning from structured precedents once agreements and disputes exist. Template selection alone does not implement the confirmed invention capability. Reasoning uses each person's known cases, represented principles, and uncertain estimates rather than omniscient world state.

## Decision loop

1. Update needs and known changes.
2. Check whether the current action remains physically possible and whether an urgent need requires interruption.
3. Update goal and plan progress at a suitable review interval or meaningful event.
4. Generate apparently feasible next actions from needs, active plans, known offers, obligations, and opportunities, using the character's information.
5. Score candidates using expected outcomes known to this character.
6. Keep the current action unless an urgent condition or sufficiently better option justifies switching.
7. Attempt the chosen action through the same reservations and physical resolution used for player actions.
8. Learn from its outcome and reconsider blocked plans.

Candidate generation must include self-directed opportunities such as foraging or proposing a project; citizens cannot depend exclusively on tasks created by the player or an employer.

## Utility and feasibility

Use a common score scale for comparable immediate actions:

```text
utility = expected need relief
        + goal progress and personal benefit
        + benefit to valued people or institutions
        + expected compensation, learning, and reciprocity
        + accepted duties and social consequences
        - time, effort, danger, and opportunity cost
```

Personality and beliefs weight the terms. Expected benefit includes uncertainty, credibility, and delays. Settlement directives are messages or institutional instructions with a known sender, not a universal hidden bonus on every citizen's farming score.

Separate physical infeasibility from social prohibition. A character cannot carry more than their capacity. They can consider breaking a rule when they believe they can attempt it. Guards, detection risk, conscience, relationships, and punishment affect the latter choice.

Planning and route estimates use known or believed conditions. Execution checks actual physical conditions. A person can therefore approach an empty store or a newly blocked route before discovering the problem; hidden state must not silently remove the candidate or reveal a better alternative. Learning why an attempt failed follows observation and reporting rules.

First compare survival options by whether they can plausibly succeed in time. Do not allow a large ambition bonus to make every starving person continue a years-long project. Desperation changes the range of acceptable risk without guaranteeing an identical response from everyone.

Use stable tie-breaking and a switching margin. Retain a commitment unless its assumptions fail, needs become urgent, or another option exceeds it by the configured margin. Log the winning terms and blocked alternatives for inspection.

## Household and community behavior

Households pool or separate assets according to their actual arrangements, care for dependents, coordinate work, and create consumption demand. Kinship does not automatically imply perfect loyalty or shared ownership.

Community behavior emerges from personal contribution, membership, reciprocal help, coordination, and enforcement. Collective projects need someone to propose them, people to support them, and a procedure for decisions. A desire for a school does not instantly create teachers or construction labor.

## Explainability and acceptance

The debug inspector explains a person's action, immediate intention, leading utility reasons, knowledge, and next blocker. Normal player views reveal only what the ruler knows or has been told, including uncertainty; they do not expose another person's private scores or intentions. Distinguish a poor choice based on false information from a bug using information the actor could not possess.

Acceptance scenarios cover survival with accessible supplies, deprivation despite inaccessible abundance, different choices from different beliefs or goals, learning through work, plan revision after failure, care of dependents, and mortality with conserved possessions. Also verify that denied requests can lead to new plans rather than repeated identical requests every minute.
