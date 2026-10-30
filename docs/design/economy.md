# Goods, labor, projects, and trade

**Status:** Physical transport, contingent property arrangements, voluntary and coerced labor, and enforceable or broken agreements are required. Detailed rules below are proposed unless marked confirmed.

## Physical goods and social rights

Every physical good has one physical location at a time: an area store, a building's storage, a character's carried inventory, a vehicle, or a shipment. Travel moves its carrier and cargo together. A good cannot be in a warehouse and on a cart simultaneously.

Track these independently:

| Concept | Meaning |
| --- | --- |
| Physical location | Where the object or fungible lot actually is. |
| Custody | Who currently controls or is responsible for it. |
| Ownership claim | Who asserts ownership, with evidence and social recognition. |
| Access permission | Who is allowed to consume, carry, borrow, or use it, under which conditions. |
| Reservation | Which pending physical action has temporarily committed a quantity or object. |
| Obligation | What an agreement, custom, or institution says someone owes. |

Custody is not ownership. A carrier can hold a farmer's harvest. A thief can possess disputed goods. A communal organization can claim a store while residents have consumption rights. Conflicting claims never duplicate the physical quantity.

Start with integer fixed-point quantities: `1,000` internal units equal one displayed unit. Use resource definitions for mass, volume, perishability, and uses. Introduce individually identified items for tools, weapons, equipment, and unique valuables when condition and identity matter.

Split and merge fungible lots only when location, custody, claims, quality, spoilage cohort, and reservations remain representable. Preserve provenance needed for disputes. Production should aggregate compatible output rather than create a new permanent record every minute.

## Storage, carrying, and delivery

**Confirmed:** Goods must be carried between areas. Local hauling is not an automatic settlement-wide transfer.

Storage has finite capacity, permitted uses, access conditions, and exposure or spoilage properties. Personal carrying capacity depends on physiology, equipment, injury, and load. Containers hold goods without changing ownership.

A delivery task specifies source, destination, cargo, recipient or custodian, terms, and urgency. Its execution is:

```text
obtain permission or choose unauthorized taking
→ reserve available cargo and carrying capacity
→ travel to source → load → travel with cargo → unload
→ confirm delivery and settle any resulting obligation
```

Reservations cannot take goods out of an inventory. Loading does. A recipient must have capacity and accept delivery under the relevant access rules. If unloading fails, the cargo stays with its carrier and the task enters a blocked state.

A delivery plan may request a reservation before the carrier arrives only through an actual custodian or access arrangement. It cannot lock remote goods merely because a planner selected them. Reservations coordinate actions; they are not physical protection against theft, force, spoilage, or destruction. Revalidate the remaining stock and access when loading, release invalid reservations, and notify affected plans through the appropriate knowledge channels.

A stolen, destroyed, spoiled, or abandoned shipment changes physical stock and creates the appropriate knowledge and agreement consequences. Canceling a delivery does not teleport cargo back. The carrier must return, redirect, store, surrender, or abandon it through another action.

Travel, carrying, eating, and work compete for the same character's time. Meals can be consumed from accessible carried food while stationary. Whether specific travel modes permit eating or resting underway is a later transport rule, not a free extra action.

## Food and survival

Food is physically produced, stored, transported, prepared where required, consumed, and spoiled. A realm-wide stock total is informational; it does not make food available to a hungry character.

The character's candidate plans can include using their own provisions, using a recognized entitlement, requesting a gift, exchanging goods or labor, collecting an agreed payment, stealing, coercing someone, or migrating. Consider time to obtain food as well as quantity and entitlement. An unreachable meal does not solve hunger.

Consumption requires physical access. Permission determines whether the action is accepted, disputed, or concealed; it does not replace physical checks. Denied access can trigger negotiation, a different plan, or conflict.

Guardians do not consume food to satisfy hunger. Their freedom from hunger does not remove transport, storage, or time costs when they supply someone else.

## Production and land

Production uses a site, labor, inputs, tools, knowledge, and time. The physical land has capacity and environmental properties independent of who claims it. Recognized use rights and the ability to resist trespass determine social access.

A recipe specifies inputs, required tools and facilities, worker capacity, skill effects, progress, output, waste, and completion conditions. Consume inputs at declared phases; do not refund consumed material merely because work is interrupted. Preserve fractional output or progress remainders.

Full farming includes preparation, planting, tending, growth, and harvest. Seed is a distinct resource definition in the full design; harvested crops can yield food and retained seed through explicit recipes. Seasons, crop failure, soil quality, and water require a chosen starting environment and calendar.

The earliest engine test may use continuous food production as a test fixture. It must not become an undocumented substitute for the final farming model.

Renewable forests need regrowth and accessible yield; mines have finite deposits. Resource depletion and transport cost can motivate new sites, settlement expansion, trade, and conflict.

## Work tasks and motivations

A task describes useful work; it is not an order that binds an arbitrary citizen. It has a source, location, required skills and equipment, capacity, inputs, expected outputs, beneficiary, access conditions, hazards, and offered terms.

A worker evaluates benefits to themselves and others, relationships, duties, pay, learning, personal goals, risk, travel, fatigue, and alternatives. Work may be self-employment, household contribution, communal service, paid employment, apprenticeship, reciprocal help, an institutional duty, or coerced labor.

The same farm can use several arrangements. Do not give a privately owned enterprise free workers merely because it posts tasks. Workers need an arrangement they choose or a concrete mechanism compelling them.

Tasks use `Open`, `Blocked`, `Completed`, and `Cancelled` states. Claims reserve capacity, release on departure or invalidation, and cannot exceed physical limits. Work producers maintain stable tasks rather than duplicating them every update. A blocked task records what is missing and can create a procurement, repair, or delivery subtask.

## Labor arrangements

People can make personal offers describing duties, location, expected output, conditions, compensation, supplies, and consequences. Repeated patterns may be described as employment; there is no employment or payroll entity. In-kind food, output shares, and reciprocal work are possible before money exists.

Acceptance creates obligations and access permissions. Each worker still decides whether to attend, continue, demand payment, leave, or refuse unsafe conditions. An employer may default or dismiss workers. Physical coercion, where attempted, belongs to the conflict system rather than a forced assignment flag.

People may observe work and form expectations of compensation based on time or output. Payment requires a later personal decision and a physical transfer; it may be withheld or disputed. Record actual work once so different observers can reason about the same activity without creating extra work.

Apprenticeship, household contribution, and communal obligation describe interactions among people, including work, teaching, access, and support. They do not require universal contribution accounting or a wage contract.

## Agreements

Support typed terms before adding new economic forms:

- transfer a specified asset or quantity;
- grant access, use, occupancy, or custody rights;
- provide labor, transport, protection, or instruction;
- deliver a quantity by a date or recurring schedule;
- share a fraction of defined production or revenue;
- lend goods or currency with repayment terms;
- supply collateral or escrow and define its release;
- promise reciprocal or contingent support.

Each agreement records parties, authorized signatories, terms, knowledge or witnesses, dates, guarantees, recognized adjudicator if any, performance records, and state. States include proposed, active, fulfilled, rejected, withdrawn, breached, renegotiated, and terminated. Breach does not automatically erase surviving obligations.

Validate responses against authority and each term's meaning, recording whether participation was willing or compelled. A coerced signature or apparent agreement based on deception does not establish private approval or universal legitimacy. Acceptance atomically creates the agreement and any immediately effective rights or claim transfers. Only transfers physically possible at that moment occur immediately. A promised delivery from a distant store becomes an obligation and task, never an instant stock relocation.

People can remember promised shares of attributed production and decide whether to deliver, demand, withhold, dispute, or forgive them. Any division must move actual goods.

Division occurs through the actual actions of people controlling the goods. A custodian can conceal or withhold output, subject to detection and consequences.

Death, incapacity, destroyed property, and political change do not silently settle a promise. People interpret its terms and decide whether to pursue, honor, substitute, renegotiate, or abandon claims.

## Exchange, markets, money, and credit

Begin with direct offers and standing requests. A market is a place and set of practices that lets buyers and sellers discover offers, meet, inspect goods, and arrange exchange. Prices reflect individual estimates, stock, need, alternatives, transport, and bargaining power.

Currency requires an accepted medium, issuers or sources, and trust. Keep its physical or account form explicit. A bank balance is a claim against an institution, not a second copy of deposited physical goods.

Credit records a future obligation and default risk. Institutions can provide custody, clearing, loans, and enforcement only once the required records and behaviors exist. Do not add universal money, frictionless global prices, or a central exchange as hidden prerequisites for the first farm.

## Projects and enterprises

A project represents an intended result, sponsor, beneficiaries, proposed ownership and access, prerequisites, resources, financing, labor arrangements, milestones, blockers, and resulting assets. Characters and organizations can create projects autonomously.

Use states `Draft`, `SeekingSupport`, `Ready`, `Active`, `Blocked`, `Completed`, `Abandoned`, and `Failed`. A rejected funding proposal does not necessarily kill the project: the sponsor may revise it, find another partner, delay, or abandon it. Require a material change or a cooldown before repeating an identical rejected offer.

An enterprise is a description of ongoing interactions among people, assets, knowledge, production, and personal commitments. It is not an actor or a second authoritative inventory. Loss of access, death, and unavailable labor can change or end the pattern without deleting physical property.

## First complete farm example

This scenario demonstrates the systems; private ownership is one possible result.

1. A skilled citizen wants greater independence or security and identifies unmet demand for food.
2. They identify usable land, existing claims, likely supporters, and required inputs.
3. They propose an arrangement to the relevant claimant or community. Public authority participates only if it actually controls needed rights or resources.
4. Supporters negotiate use rights, food and seed, tools, timber, labor, risk, and expected returns.
5. Accepted terms create obligations, permissions, procurement tasks, and deliveries.
6. Workers voluntarily accept roles or face an explicit coercive attempt; inputs physically reach the site.
7. Construction and cultivation progress through real work. The project can stall or fail.
8. People may continue proposing production, maintenance, sales, and delivery work.
9. Harvest enters actual custody. People may deliver, withhold, dispute, or renegotiate expected shares.
10. Outcomes change wealth, trust, livelihoods, institutions, and future project choices.

## Acceptance scenarios

- Ownership changes without teleporting goods; physical movement can occur without ownership changing.
- Shared, private, and coercive access can coexist in one settlement.
- A carrier interrupted mid-trip retains cargo; canceled deliveries conserve goods.
- Labor shortages block production despite available land and capital.
- Input consumption, production, spoilage, meals, theft, and destruction explain every stock change.
- A broken output-share agreement records debt and consequences without magically extracting payment.
- Death and conflicting inheritance claims preserve property and obligations for resolution.
