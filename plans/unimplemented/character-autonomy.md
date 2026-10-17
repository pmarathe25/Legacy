# Character autonomy audit and implementation plan

**Status:** Deferred until the multi-generation experiments are assessed. Reviewed through 2026-10-17, including the extinction repairs. References use files and symbols rather than unstable line numbers. Current work remains subject to validation.

Characters should choose from perceived opportunities using their characteristics, needs, beliefs, memories, commitments, and expected consequences. Survival is one motive. Mistakes, recklessness, obsession, altruism, defiance, and unsuccessful societies are legitimate outcomes.

## Existing foundations

Feature owners provide duration, capacity, resource costs, cancellation, rollback, and typed consequences. Personal knowledge limits provisioning routes and remembered sources. Conversation requires the other person's willingness.

Current work also introduces several appropriate changes:

- Contested use remains physically possible; permission affects reluctance and witnessed interference (`crates/game-property/src/use_goods.rs`). Grievances depend on individual recognition and empathy (`crates/game-grievances/src/lib.rs`).
- Adopted institutional rules can remain unenacted when a custodian refuses (`crates/game-institutions/src/lib.rs`; `crates/game-communities/src/organizing.rs`). Fear, loyalty, personal arrangements, and ballots have distinct representations.
- Leisure satisfaction reduces motivation without forbidding repetition (`crates/game-pastimes/src/motivation.rs`). Companionship competes with leisure; rotating encounters and novelty permit new contacts (`crates/game-social-life/src/conversation.rs`).
- Inheritance expresses traits before decisions consume them (`crates/game-heredity/src/lib.rs`). Intelligence affects planning depth; learning affects practice; strength affects conflict; size and strength affect carrying capacity. Ability definitions extend to 16,000 with a baseline of 1,000.
- Vitality separates inherited potential from aging, stress, and condition. Nutrition tracks intake and dependence; plant lots retain provenance. Storage and shelter consume materials and time.
- Action/meal completion now wakes existing decisions; food intentions retain their chosen good. Missing suppliers are disproved at their remembered locality without revealing a remote destination (`game-characters/scheduling`, `game-provisioning`, `game-knowledge::ObserveStore`). These repair scheduling and evidence rather than prescribe motives.

These capabilities do not yet establish a common model of autonomous choice.

## Prioritized changes

### 1. Make features offer actions to one character decision owner

The largest restriction is execution order. `crates/game-characters/src/lib.rs` starts reserve gathering before idle activities. `IdleReview` listeners then directly execute actions; the host visits named listeners in lexical order (`crates/game-sim/src/transaction.rs`, `registry.rs`). Early care, cultivation, travel, or work can consume the action slot before leisure is considered. General `Candidate` scoring currently compares food and rest, not all these motives.

Introduce bounded proposal → choice → execution. Features offer perceived options, estimated consequences, and typed executable requests. Characters compares motives and commitments; the chosen owner validates reality and executes. Preserve feature ownership and acyclic dependencies through a small decision interface.

Reconsider when evidence changes. Replace urgent hunger's survival-only filter (`crates/game-utility/src/lib.rs`, `choose`) with individual assessment; incapacity remains physical. Careless overproduction and foresighted reserves should both be possible. Committed food plans currently bypass that comparison until success/failure; wakeups should eventually offer reconsideration rather than mandate continuation.

Earlier scarcity runs exposed preferred batches being treated as physical minimums. Provisioning now permits smaller harvests; preserve quantity alternatives when comparing motives.

Dependent care now serves local children before travel and compares claim costs, preferring personal supplies. It still uses common fixed weights rather than comparing care, source, and other motives (`crates/game-households/src/lib.rs`, `tend`).

### 2. Separate scheduling frequency from willingness

Several timers still suppress choices: social retry after successful conversations (`crates/game-social-life/src/conversation.rs`), two-day journeys (`crates/game-wayfaring/src/lib.rs`), weekly affiliation (`crates/game-affiliation/src/lib.rs`), and annual creative works (`crates/game-works/src/lib.rs`). Leisure satisfaction is the better pattern.

Keep bounded scheduling and cached failed searches, invalidated by relevant changes. Satisfaction, frustration, preparation, attention, and recovery affect motivation or actual capability. Review intervals must not forbid available actions. Separate choosing to create a work from automatically recording one after eligible leisure.

### 3. Complete the distinction between institutions and physical power

Membership, recognized procedures, and acceptance determine valid institutional records. They do not physically prevent taking, refusal, departure, or force.

Horticulture still excludes supplies without permission (`crates/game-horticulture/src/lib.rs`). Offer known contested opportunities with estimated consequences, without revealing unseen stores. Preserve mutual consent for exchange and conversation; coercion needs separate actions.

Justice uses nearest precedents and caller-supplied enforcement willingness (`crates/game-justice/src/lib.rs`); grievances chiefly provoke submission demands. Offer forgiveness, warnings, negotiation, reporting, avoidance, retaliation, and enforcement. Targets need resistance, flight, and allies. Confinement needs physical barriers and derived escape capability (`crates/game-confinement/src/lib.rs`).

Pressure now permits anticipation from co-located visible build, backdown, and escape along personally known roads; stale roads cannot manufacture escape (`crates/game-pressure/src/{judgment,retreat}.rs`). Its ordered response still tests submission before flight, preventing comparison even when both motives are strong. This belongs in the deferred choice redesign, not an information-boundary rollback.

### 4. Separate biology, family preferences, and cultural transmission

Parenthood now separates reproductive intentions from household records. Both prospective parents weigh attachment, fear, grievance, learned kinship interest, current needs, stress, and remaining care for biological and foster children. Uniform empathy/patience eligibility, lifetime child allowances, and mandatory annual/three-year waits have been removed. Lifecycle models maturity and a coarse fertile window relative to inherited natural lifespan; partnerships remain possible beyond it. Local, personally motivated fostering and independent orphan enrollment repair missing care paths. Remaining work includes multiple relationship forms, distinct reproductive roles, recovery and fertility biology, separation, shared caregiving, and child assent. Fertility must not directly reward a favored personality.

Continue genes → characteristics → behavior, with meaningful cognitive capabilities and tradeoffs. Newborn beliefs now begin neutral; learn culture through interactions. Avoid reading private counterpart beliefs as expressed attitudes (`crates/game-social-life/src/conversation.rs`, `warmth`). Existing developmental shortcuts also need replacement: general agency starts abruptly at age six (`game-households::tend`), and `game-physiology/src/eating.rs::Feed` chooses from the patient's private dietary experience. Model gradual agency and communicated preferences or caregiver estimates; these predate the extinction fixes.

### 5. Give new environmental systems the same choice boundary

**Storage and shelter:** `crates/game-food-storage/src/decisions.rs` imposes a foresight threshold, personally owned stores, and exactly two retained portions per food. Shelter follows fuel-fetch intentions before reassessing cold (`crates/game-shelter/src/decisions.rs`, `consider`). These prototype policies should become choices among sharing, departure, construction, fuel, spoilage, reserves, and other interests.

**Seeds:** `crates/game-cultivars/src/stocks.rs` uses personal observations and bounded search, but universally optimizes yield/time. Permit reliability, nutrition, flavor, tradition, and experimentation. Horticulture now allows marginal climate trials within the strain tolerance range, without guaranteeing success. Its permanent one-plot limit and representative-strain mixed seedkeeping also bound experiments; these are prototype restrictions, not evolved practices.

**Food:** Source-specific learned dietary motives now prevent deficiency or craving from compelling unrelated foods (`crates/game-nutrition/src/motives.rs`). Coarse experienced estimates avoid exact catalog knowledge. These motives still compete mainly within food/rest; include abstinence and other interests in the common choice model. Physical `ConsumeEdible` permits specific goods and quantities.

**Physical consistency:** Exposure now uses the surrounding climate during travel, with a passing road/save replay regression. Maturation checks temperature only at completion (`crates/game-maturation/src/lib.rs`, `Ripen`): a coarse prototype approximation. Distinguish these from valid duration, finite stocks, carrying limits, and biological constraints.

### 6. Let exchange conventions arise from individual decisions

Unequal proposed quantities and personal acceptance memories now replace equal swaps as the only exchange form (`game-exchange`, `game-demand`). The resale estimate has no designated currency, mint, universal price, or global knowledge. It can motivate an intermediate commodity after actual completed agreements. This is a foundation, not evidence of a monetary society: measure repeated re-exchange across independent partners in histories.

Still replace the daily encounter gate, first-eight disclosure, immediate negotiation, fixed relationship refusal thresholds, and universal diminishing-stock utility with individual time, trust, commitments, and learned preferences. Evidence of failed offers should also update perceived acceptance. Shared denominations, debt, promises accepted by third parties, and coin manufacture should require character actions and beliefs; no institution should force a currency's value or acceptance.

### 7. Let attachment influence choices without granting magical protection

Sparse physical object identities and personal sentimental memories now affect exchange, receiving, crafting, tending, and witnessed loss (`game-items`, `game-sentiment`). Plain substitutes can be traded without losing a keepsake. Retain private memories after transfer or destruction, and never notify absent owners of unseen loss. Specific wild representatives have stable identities separate from their genomes; individual animals still need a physical owner. Recovery, gifting, remembrance, and inherited testimony should become proposed choices rather than forced scripts.

## Acceptance evidence

- Reordering proposal registration cannot change decisions; identical evidence reproduces decisions across workers and save/load.
- Matched characters choose differently when personality, perceived risk, interests, or experience differs. Repeating leisure remains possible; urgent information invalidates stale retries.
- A prohibited but physically possible act succeeds; absent witnesses do not acquire knowledge. Witnesses can disagree, and an adopted order can fail through refusal or resistance.
- Family formation and reproductive decisions vary without forced replacement births or a universal personality filter.
- Repeated meal wakeups preserve chosen food and bounded scheduling; negative supplier evidence reveals no remote stock or destination. Feeding uses caregiver evidence or expressed requests, and children gain agency gradually.
- Pressure compares submission, flight, and resistance using the same personally available evidence; unknown or closed roads cannot release a real encounter.
- Storage and heating options compete with food, travel, and personal goals; sharing or contested use remains possible. Changed weather can end a fuel-fetch intention.
- An adapted seed can be considered wherever its expressed biology permits growth. Different learned priorities can favor different strains without inspecting unseen genomes.
- Food effects occur after consumption; first exposure does not grant exact chemical knowledge. Craving can influence choices without forcing compliance, and actors can knowingly choose harmful substances.
- Thermal exposure advances during travel, and curing experiments distinguish actual accumulated conditions from the current completion-time approximation.
- Twenty-generation journals retain parentage, cohort trait distributions, reproductive success, deaths, environment, and performance measurements. Distinguish selection from drift, inherited variation from learned adaptation, and extinction from an implementation defect. Survival counts alone establish none of these.
