# Crates

Each mechanic has one owner. Dependencies point toward shared types, the host, and lower features. Features communicate through receiver-owned requests and typed notifications, and their tests do not depend on `game-assembly`.

`game-core` contains shared values. `game-storage` provides persistent dense leaves and shared sparse records. `game-packed` provides dense rows and nested undo journals; `game-facts` separates immutable scenes from personal evidence. `game-sim` is the single generic host. `game-api`, `game-assembly`, `game-openings`, `game-session`, and `game-cli` contain public values, composition, fixture initialization, threading policy, and text presentation respectively. Every other crate owns a named mechanic or pure utility.

Source size and dependency direction are checked by [architecture tests](game-assembly/tests/architecture.rs). A feature can be checked and tested directly:

```sh
cargo check -p game-agriculture --lib
cargo test -p game-agriculture
```

| Crate | Responsibility | Direct game dependencies |
| --- | --- | --- |
| [game-agreements](game-agreements/README.md) | Negotiated multi-party promises, acceptance/refusal, outstanding duties, physical fulfillment, and default | `game-storage`, `game-core`, `game-sim`, `game-property`, `game-cargo` | Dense stable-ID components, nested row undo, numeric ID indexes, bulk CPU work. |
| `game-facts` | Shared immutable scenes, personal evidence, cold history, borrowed fact views. |
| `game-storage`, `game-core`, `game-sim`, `game-property`, `game-cargo` |
| [game-agriculture](game-agriculture/README.md) | Sowing consumes seed and labor; growing follows configurable time and climate; harvesting consumes labor and produces food | `game-storage`, `game-content`, `game-objects`, `game-core`, `game-sim`, `game-transport`, `game-cargo`, `game-property`, `game-physiology`, `game-environment`, `game-skills` |
| [game-api](game-api/README.md) | Typed player commands and immutable observations for CLI or a future renderer | `game-lineage`, `game-core`, `game-physiology`, `game-transport`, `game-knowledge`, `game-invocation`, `game-conflict` |
| [game-armies](game-armies/README.md) | Individual recruitment, unit membership, real food consumption, and mission deployment | `game-storage`, `game-objects`, `game-property`, `game-transport`, `game-core`, `game-sim`, `game-missions`, `game-cargo`, `game-physiology` |
| [game-assembly](game-assembly/README.md) | Ordered plugin installation, public command binding, immutable view composition, and Simulation lifecycle | `game-content`, `game-objects`, `game-traits`, `game-core`, `game-sim`, `game-api`, `game-openings`, `game-land`, `game-transport`, `game-cargo`, `game-property`, `game-environment`, `game-physiology`, `game-knowledge`, `game-skills`, `game-ecology`, `game-agriculture`, `game-workshop`, `game-health`, `game-relationships`, `game-agreements`, `game-institutions`, `game-justice`, `game-lifecycle`, `game-lineage`, `game-guardians`, `game-invocation`, `game-characters`, `game-logistics`, `game-projects`, `game-realm`, `game-conflict`, `game-confinement`, `game-missions`, `game-armies` |
| [game-cargo](game-cargo/README.md) | Physical inventories, checked quantity accounting, atomic local transfers, carried custody, and output-capacity reservations | `game-storage`, `game-objects`, `game-core`, `game-sim`, `game-transport` |
| [game-characters](game-characters/README.md) | Autonomous survival choices, committed food plans, known travel costs, learned values, urgent needs, and failed-option cooldowns | `game-storage`, `game-packed`, `game-traits`, `game-provisioning`, `game-content`, `game-transport`, `game-core`, `game-sim`, `game-physiology`, `game-knowledge`, `game-relationships`, `game-invocation`, `game-lineage`, `game-social-life` |
| [game-cli](game-cli/README.md) | Small text adapter for the same public commands as a future renderer | `game-content`, `game-core`, `game-api`, `game-assembly`, `game-sim` |
| [game-confinement](game-confinement/README.md) | Custodial confinement, explicit release, and bounded escape checks | `game-storage`, `game-core`, `game-sim`, `game-physiology`, `game-transport` |
| [game-conflict](game-conflict/README.md) | Configurable tactical fixture with generated continuous geometry, simultaneous movement/attacks, fixed world time per shared round, player input stops, and lasting bod… | `game-storage`, `game-core`, `game-sim`, `game-physiology`, `game-transport` |
| [game-communities](game-communities/README.md) | Individual cooperation, chosen decision rights, personal allegiance, and practical supply access | `game-core`, `game-sim`, `game-storage`, `game-social-life`, `game-relationships`, `game-institutions`, `game-property`, `game-cargo`, `game-knowledge`, `game-lineage` |
| [game-content](game-content/README.md) | Parse versioned TOML definitions and fingerprint their normalized data | `game-core` |
| [game-core](game-core/README.md) | Typed identities, milliseconds, checked time arithmetic, errors, and deterministic counter-based randomness |  |
| [game-ecology](game-ecology/README.md) | Timed gathering reserves finite stock, consumes one canonical action, produces physical goods, and credits learning | `game-storage`, `game-core`, `game-sim`, `game-cargo`, `game-transport`, `game-physiology`, `game-skills` |
| [game-environment](game-environment/README.md) | Configurable calendar and periodic climate lookup using integer time | `game-content`, `game-core`, `game-sim` |
| [game-facts](game-facts/README.md) | Shared immutable scenes, independent evidence, cold history, and borrowed fact views | `game-core`, `game-storage` |
| [game-genetics](game-genetics/README.md) | Seeded inheritance, bounded mutation, and trait expression as pure functions | `game-core` |
| [game-guardians](game-guardians/README.md) | Guardian identity, an innate successor connection, route-by-route search, mentorship assessment, and refusal without abandonment | `game-storage`, `game-core`, `game-sim`, `game-lineage`, `game-transport`, `game-land`, `game-spatial`, `game-physiology`, `game-skills` |
| [game-health](game-health/README.md) | Actual treatment consumes medicine and caregiver time | `game-content`, `game-objects`, `game-core`, `game-sim`, `game-physiology`, `game-cargo`, `game-property`, `game-transport` |
| [game-institutions](game-institutions/README.md) | Founding and explicit procedures, bounded access-rule proposals, voting with executable permission consequences, offices, and removal | `game-storage`, `game-core`, `game-sim`, `game-property` |
| [game-invocation](game-invocation/README.md) | Selected past-life memories remain temporary | `game-content`, `game-conflict`, `game-core`, `game-sim`, `game-lineage`, `game-physiology` |
| [game-justice](game-justice/README.md) | Explicit complaints, evidence, bounded reasoning over a judge's known precedents, decisions, and physical enforcement | `game-storage`, `game-core`, `game-sim`, `game-property`, `game-cargo` |
| [game-knowledge](game-knowledge/README.md) | Local observations, stock and route memories, spoken and delayed reports, and route planning from known roads | `game-storage`, `game-packed`, `game-facts`, `game-land`, `game-spatial`, `game-ecology`, `game-property`, `game-core`, `game-sim`, `game-cargo`, `game-transport` |
| [game-land](game-land/README.md) | Owns named areas and positive-duration bidirectional links | `game-storage`, `game-core`, `game-sim` |
| [game-lifecycle](game-lifecycle/README.md) | Actual scheduled pregnancies and births with stable identities, inherited traits, caregivers, and physical placement | `game-storage`, `game-core`, `game-sim`, `game-physiology`, `game-transport`, `game-cargo`, `game-genetics` |
| [game-lineage](game-lineage/README.md) | Persistent life records, mortal ruler death, indefinite succession gaps, and selection from actual post-death births | `game-storage`, `game-core`, `game-sim`, `game-physiology`, `game-lifecycle`, `game-skills`, `game-knowledge` |
| [game-logistics](game-logistics/README.md) | Physical load–travel–unload delivery with capacity, local access, multi-road routing, and blocked-delivery retries | `game-storage`, `game-core`, `game-sim`, `game-land`, `game-spatial`, `game-transport`, `game-cargo`, `game-property`, `game-physiology` |
| [game-missions](game-missions/README.md) | Consenting parties, preparation, shared strategic travel, arrival, return, and mortality consequences | `game-storage`, `game-core`, `game-sim`, `game-physiology`, `game-transport`, `game-land`, `game-spatial`, `game-cargo` |
| [game-objects](game-objects/README.md) | Runtime object catalogs with stable data keys and IDs, names, mass, tags, and edible properties | `game-core`, `game-sim`, `game-content` |
| [game-openings](game-openings/README.md) | Selectable, explicitly labeled noncanonical fixtures | `game-content`, `game-objects`, `game-traits`, `game-scenarios`, `game-core`, `game-sim`, `game-land`, `game-transport`, `game-cargo`, `game-property`, `game-environment`, `game-physiology`, `game-knowledge`, `game-skills`, `game-ecology`, `game-agriculture`, `game-workshop`, `game-health`, `game-relationships`, `game-agreements`, `game-institutions`, `game-justice`, `game-lifecycle`, `game-lineage`, `game-guardians`, `game-invocation`, `game-characters`, `game-logistics`, `game-projects`, `game-realm`, `game-conflict`, `game-confinement`, `game-missions`, `game-armies` |
| [game-packed](game-packed/README.md) | Dense stable-ID components, row undo journals, numeric indexes, and bulk CPU work | `game-core` |
| [game-physiology](game-physiology/README.md) | Owns hunger, fatigue, rest, injury, mortal death, and in-place immortal recovery | `game-storage`, `game-packed`, `game-content`, `game-objects`, `game-core`, `game-sim`, `game-transport`, `game-cargo`, `game-property` |
| [game-projects](game-projects/README.md) | Proposals, physical in-kind contributions, material consumption, construction time, failure, and an operating site | `game-storage`, `game-core`, `game-sim`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-skills` |
| [game-property](game-property/README.md) | Conflicting ownership claims, recognition, revocable per-person permission, and physical taking | `game-storage`, `game-packed`, `game-core`, `game-sim`, `game-cargo`, `game-transport` |
| [game-provisioning](game-provisioning/README.md) | Executable meal and foraging plans using learned sources, real travel, containers, gathering, and consumption | `game-core`, `game-sim`, `game-objects`, `game-knowledge`, `game-cargo`, `game-transport`, `game-physiology`, `game-ecology` |
| [game-realm](game-realm/README.md) | Mandates attach to recognized offices and impose jurisdiction and spending limits | `game-storage`, `game-core`, `game-sim`, `game-institutions`, `game-property`, `game-cargo` |
| [game-relationships](game-relationships/README.md) | Directed trust, affection, fear and grievance; separate social beliefs and slowly learned concept values | `game-storage`, `game-core`, `game-sim` |
| [game-scenarios](game-scenarios/README.md) | Declarative scenario schemas, stable symbolic references, population groups, validation, and runtime discovery | `game-core`, `game-content`, `game-physiology` |
| [game-session](game-session/README.md) | A dedicated host thread processes input between bounded advances and coalesces ordinary progress into one slot | `game-core`, `game-api`, `game-assembly`, `game-sim` |
| [game-sim](game-sim/README.md) | One generic transactional host: typed resources, due requests, stable event order, command deduplication, bounded advancement, and validated saves | `game-storage`, `game-packed`, `game-core` |
| [game-skills](game-skills/README.md) | Diminishing skill practice and unique work attribution | `game-storage`, `game-core`, `game-sim` |
| [game-social-life](game-social-life/README.md) | Local companionship, mutual conversations, directed impressions, learned fellowship, and remembered reports | `game-core`, `game-sim`, `game-storage`, `game-content`, `game-physiology`, `game-transport`, `game-knowledge`, `game-relationships`, `game-traits` |
| [game-spatial](game-spatial/README.md) | Pure deterministic shortest-path search | `game-core`, `game-land` |
| [game-storage](game-storage/README.md) | Persistent dense leaves, shared sparse records, ordered scans, and bounded copying |  |
| [game-traits](game-traits/README.md) | Authored trait names, bounds, defaults, and utility weights, plus individual profiles | `game-storage`, `game-core`, `game-sim`, `game-content` |
| [game-transport](game-transport/README.md) | Timed road traversal, partial travel, cancellation, and physically carried movement | `game-storage`, `game-packed`, `game-core`, `game-sim`, `game-land` |
| [game-workshop](game-workshop/README.md) | Configurable input recipes, time-consuming production, attributed learning, and explicit waste from interruptions | `game-storage`, `game-content`, `game-objects`, `game-core`, `game-sim`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-skills` |

[game-worldgen](game-worldgen/README.md) generates seeded physical initial conditions without prebuilt societies. `game-openings` runs their live history before player control; generation is a pure utility rather than a second simulation host.

[game-pressure](game-pressure/README.md) owns coercive demands, real conflict-backed fear, resistance, and submission events. It changes no institution or permission itself.

| Daily-life owner | Responsibility | Physical action owners |
| --- | --- | --- |
| [game-livelihoods](game-livelihoods/README.md) | Individual stock goals, bounded ingredient planning, retries, and personal work interests | Ecology and workshop |
| [game-horticulture](game-horticulture/README.md) | Voluntary planting and tending decisions | Agriculture |
| [game-care](game-care/README.md) | Local personal concern, caregiver assignments, and gratitude | Health |
| [game-pastimes](game-pastimes/README.md) | Authored individual activities, reusable possessions, skill practice, and learned interests | Cargo and the common action clock |

All four subscribe to `game-characters::IdleReview` and depend on lower features. Characters has no dependency back to them. Generated openings enable the decision owners; authored fixtures keep their controlled behavior.

[game-stockpiles](game-stockpiles/README.md) owns finite personal ground-cache indexes and composes cargo, property, and knowledge requests. [game-wayfaring](game-wayfaring/README.md) owns voluntary journey intentions and actual visit histories. Provisioning owns survival searches beyond familiar food sources. [The history tool](../tools/history/README.md) records unattended assembled runs without changing player observations or gameplay rules.

## Longer histories

[game-heredity](game-heredity/README.md) expresses inherited characteristics through traits. [game-households](game-households/README.md) owns voluntary families and dependent care. [game-exchange](game-exchange/README.md) composes consensual local swaps and aid requests. [game-affiliation](game-affiliation/README.md) handles admission, exit, and succession through institutions and communities.

[game-works](game-works/README.md) retains individual creative works and personal evidence. [game-preservation](game-preservation/README.md) tracks food batch ages through cargo movements. [game-equipment](game-equipment/README.md) owns clothing condition, repair, and exposure decisions. [game-campaign](game-campaign/README.md) composes their generated-world enable requests. [game-diagnostics](game-diagnostics/README.md) provides optional process-local timing measurements.

[game-grievances](game-grievances/README.md) owns witnessed complaints and individual responses to contested use. Property records claims; physical action owners retain locality, time, quantity, and capacity checks. [The deferred autonomy plan](../plans/unimplemented/character-autonomy.md) identifies remaining limits of feature priority and prescribed social templates.

[game-food-memory](game-food-memory/README.md) indexes remembered edible stores and carried containers from immutable personal evidence. It is a query utility below provisioning, with no game resource or independent behavior. Payload deltas keep unchanged historical inventories out of repeated searches.

## Biology, ecology, and personal value

[game-body](game-body/README.md) provides pure bodily evolution; [game-soma](game-soma/README.md) owns physical bodies; [game-vitality](game-vitality/README.md) expresses inherited potential, aging, and experienced condition. [game-nutrition](game-nutrition/README.md) owns diets and dependence. Physiology composes eating and activity without editing these other owners' resources.

[game-crops](game-crops/README.md) owns crop definitions and seasonal exposure; [game-cultivars](game-cultivars/README.md) owns diploid plant strains and stock provenance; [game-botany](game-botany/README.md) owns representative wild stands and seed banks. Agriculture owns plot labor and harvests. [game-food-storage](game-food-storage/README.md) supplies preservation conditions and built stores; [game-maturation](game-maturation/README.md) owns prepared batches that transform over elapsed time.

[game-shelter](game-shelter/README.md) owns constructed indoor areas and finite fuel heat; [game-exposure](game-exposure/README.md) owns body temperature and thermal harm. Equipment owns worn garments and condition.

[game-items](game-items/README.md) tracks sparse identities of actual cargo units. [game-sentiment](game-sentiment/README.md) owns personal memories and attachment, composing physical experience events. [game-demand](game-demand/README.md) owns personal evidence of others accepting goods. Exchange compares practical, sentimental, and expected resale value without a designated currency.

[game-names](game-names/README.md) owns given and chosen identities, heard names, and personal forms of address. [game-naming](game-naming/README.md) composes parent choices, nicknames, earned descriptions, and voluntary self-renaming. Vitality supplies gradual physical and psychological development; wellbeing interprets childhood experiences and conversations without rewriting inherited potential.

`game-parenthood` owns voluntary reproductive intentions over household attachments, experienced condition, current caregiving demands, and lifecycle biology. It reads feature-owned state and asks lifecycle to conceive; it owns no target population.

`game-utility` supplies pure candidate comparison and switching rules. `game-food-memory` projects personal inventory evidence and contextual food appraisal; provisioning owns physical acquisition plans and their execution.
