# Individual economic simulation

This composed simulation joins personal work, actual dated goods, food acquisition, meals, trade choices, revocable promises, individual stock claims, opinions, travel, optional family lifecycle, dependent care, cultivation, personal encounters, disease, expeditions, and ruler continuity at staged boundaries. People may assert conflicting claims, respect or disregard another's claim, and choose to claim their own delivered outputs. Caregivers and social participants can be motivated by personal willingness, fear or obligation; food must actually move. Newborns receive custody atomically; journeys retain their carried lots. Disease can spread through social encounters, caregiving, successful personal transfers, and actual births when a strain enables maternal transmission; individuals may choose to consume medicine they hold. Additional routes of exposure and treatment remain to be integrated. Succession uses actual births, guardians travel and mentor, and invocation warnings precede their physical costs. It remains an incomplete slice, not yet the complete campaign host. All arrangements arise from individual choices; it contains no institutional or payroll entities.

Work sites can optionally wear with completed production and require real repair materials before they admit more work. An individual may choose to repair an idle site, including under their own fear or felt obligation; competing activity and material consumption are resolved at the physical boundary.

Optional renewable animal products grow as bounded individual biological state. Collection uses an animal's response and the collector's choices, produces real dated goods, and competes with other actions involving that animal. An existing physical hold can support explicitly forced collection without implying consent.

Giving policies and responses are shared immutable collections across staged generations. Rust callers constructing `Giving` can convert vectors into the shared fields; callers changing them use `policies_mut()` or `responses_mut()`, which detach on write. Their serialized representation remains ordinary arrays.

Regional exposure uses finite shelter and authored seasons; seasonal planting, growth and harvest rules affect actual fields. Travelers face source-region exposure. Shelter allocation and crop growth are deterministic across worker counts and saves.

Social isolation and sociability influence individual voluntary encounters; personal fear or obligation may independently motivate a response. Newborns can inherit dispositions without inheriting other people’s relationships.

For affinity-guided meetings, each person can give affinity a positive, negative, or zero weight and override it for particular people. The ignored `affinity_social_benchmark` measures this selection; set `LEGACY_NEGATIVE_SOCIAL` to exercise people seeking those they dislike and `LEGACY_WORKERS` to vary concurrency.

Individuals can gather finite or renewable local resources, subject to personal choice or social pressure and actual stock capacity. Only successfully delivered output depletes a site or supports a personal claim. Health can recover under authored rules, with rest providing an additional benefit. Purpose deprivation tracks completed work and blocked goals.

Optional individual wildlife grazes the same finite resource nodes after staged human gathering and regrowth. Each animal ages, feeds, can starve, and may reproduce with a local mate; young inherit diploid traits from actual parents. The animal generation and resource deductions commit atomically. Controlled people can observe local animals without seeing their hidden genetics or pregnancies.

People can ask for food, give supplies, or provide medical care through real local contact and consumed medicine. Requests expire and do not require a response; actual food delivery satisfies them. Individual willingness and feelings influence autonomous help; treatment can expose a caregiver to infection and does not compel later cooperation. All distinct successful physical contacts can transmit disease, including several contacts involving the same person at one boundary.

People may attempt to take goods from another living person's custody. The target's own standing preferences determine whether they notice and resist; not resisting does not imply approval. A resisted attempt moves no goods, and the actor may separately choose to escalate into a real tactical encounter. Actual transfers compete for the same finite lots and capacity as other actions. Participants can configure independent signed interpretations of outcomes, and an unaware target gains no direct experience. Taking does not create a universal theft label or automatic obligation.

The ignored `taking_world_benchmark` measures the complete boundary for `LEGACY_ATTEMPTS` attempts and twice as many people. Set `LEGACY_SHARED` for one target, `LEGACY_RECOVERY` to make targets incapacitated and enable personal recovery, and `LEGACY_WORKERS` to vary concurrency; add `--features profile` and `LEGACY_PROFILE` for phase timings. The benchmark is an isolated synthetic boundary rather than a complete campaign.

The ignored `independent_combat_benchmark` stages many separate two-person
encounters in the same world and reports import and three-boundary times
separately. Set `LEGACY_ENCOUNTERS` and `LEGACY_WORKERS` to vary the workload.
It is a synthetic tactical workload, not a complete war or mixed campaign.

The ignored `help_promise_benchmark` stages actual food gifts followed by
independent personal offers. Set `LEGACY_PAIRS` and `LEGACY_WORKERS` to vary
the population and concurrency. It reports a state fingerprint and times two
boundaries; setup and fingerprinting are outside the timed interval.

The ignored `concentrated_autonomous_delivery_benchmark` measures a full boundary with `LEGACY_PEOPLE` travelers giving to one recipient. Set `LEGACY_CAPACITY` below the population to exercise capacity contention and `LEGACY_WORKERS` to vary concurrency; add `--features profile` and `LEGACY_PROFILE` for phase timings.

The ignored `concentrated_expedition_acquisition_benchmark` measures travelers independently seeking one shared stock. Set `LEGACY_PEOPLE`, `LEGACY_STOCK`, and `LEGACY_WORKERS` to vary population, available goods, and concurrency; `LEGACY_NAMED_STORE` selects a shared named store. Profile timings use the same flags.

The ignored `concentrated_meeting_objectives_benchmark` measures many travelers independently seeking one person. Set `LEGACY_PEOPLE` and `LEGACY_WORKERS` to vary population and concurrency.

Travelers who choose to acquire delivery supplies can use regional stock or a named store in their current region. Named stores have separate physical custody; regional stock claims apply to regional stock. The traveler still needs capacity, retains their chosen reserve, and can change their acquisition preference.

People held in the same cell can provide direct medical care using medicine they carry, provided they are available and the caregiver chooses to act. A barrier blocks treatment across it even when an opening permits supplies to pass. Escape, self-treatment, requests, and giving compete with medical care for the person's action.

People can independently attempt to handle a living local animal. The animal's own willingness and remembered affinity determine its response, and its configured signed reactions can change later behavior. Feeding takes precedence for an animal already selected for feeding, and handling excludes hunting that animal in the same boundary. Handling preserves the animal's identity and does not establish ownership or obedience.

Live animal restraint is a separate physical interaction. An animal may yield, resist, be overpowered, or later escape; any capable local person can release it. A hold is not ownership and ends if the handler leaves, becomes incapacitated, or is separated by a barrier. The animal remains in its actual region and continues ordinary biology.

Individuals can independently choose to release a local held animal, including one held by another person, according to their own affinity or social pressure. Releases contend per animal and take priority over its escape attempt. A handler may independently spend effort resisting one release, and a stronger releaser can prevail.

Each participant in an opposed release may interpret the other person using their own general or relationship-specific opposed-action reactions. These signed feelings can influence later choices; the outcome creates no automatic approval, hostility, or obligation.

The ignored `concentrated_animal_defense_benchmark` exercises the complete boundary with contested or independent holds. Set `LEGACY_PEOPLE` and `LEGACY_WORKERS` to compare population and worker counts, `LEGACY_SHARED` for one contested animal, `LEGACY_OPPOSED_REACTIONS` to include both participants' relationship updates, and `LEGACY_GRAZING` for competition over a shared food patch. With the shared and reaction flags, `LEGACY_PREEXISTING_EDGES` adds many existing relationships and checks sparse updates. Run it with `cargo test -p game-economy concentrated_animal_defense_benchmark -- --ignored --nocapture`; add `--features profile` and set `LEGACY_PROFILE` for phase timings. Timings depend on the machine and enabled systems.

Available cellmates can seek ordinary encounters, form relationships, transmit disease through actual contact, and respond to personal pressure. People can also choose to converse through a supply opening, including with strangers, acquaintances, or someone they fear. Talking through the opening is a social encounter but does not count as direct contact for the ordinary contact-based disease route; physical treatment still cannot cross the wall.

The minimum age for work does not by itself prevent a child from asking for food, giving possessions, having an encounter, or receiving medical treatment. Actual activities still compete for that person's time, and help remains another person's choice.

A person's food-response preference can be motivated by affinity above or below a personally chosen threshold. A signed affinity weight also controls how that person ranks multiple requesters, so they can favor people they like, favor people they dislike, or disregard affinity. Fear or felt obligation can independently motivate a response.

An authored medical preference can set `patient_health_at_most` to act on observable health. If omitted, the caregiver acts when health is below the world's maximum. The caregiver may use an ineffective medicine and still consume it; a cure only follows a successful treatment with a matching remedy. The hidden diagnosis does not decide whether they attempt care.

Self-treatment similarly uses `health_at_most`, or below maximum health when omitted. Controlled people may attempt their chosen medicine even without an infection. Treatment commands report whether the medicine was consumed, rather than disclosing a hidden diagnosis or whether it was effective.

An optional `medical.acquaintances` preference lets someone choose among local people they have actually met. Their signed urgency and affinity weights rank eligible patients; fear or felt obligation toward a patient may motivate an unwilling caregiver. The selected action competes for both people's time, consumes real medicine on success, and can be ineffective. A person may change their willingness without creating a formal role or binding commitment.

Diseases can separately define transmission through actual combat strikes. Out-of-range attacks and strains without that route do not cause combat exposure.

An optional personal acquaintance-request preference lets a hungry individual ask someone they have actually met and can currently reach. Familiarity and the requester's signed affinity can guide their choice; unanswered requests can lead them to try another known person. The recipient independently decides whether to help, including under fear or felt obligation.

The requester can set minimum and maximum affinity, a signed preference for higher or lower affinity, and whether to avoid their previous target. These are personal choices; an unanswered request does not force the requester to abandon someone.

People with an individual learning inclination can develop a bounded work aspiration after personally witnessing a requested job succeed. Other people's work memories and disabled aspirations are not overwritten; a new aspiration still needs ordinary resources and voluntary action.
Production skill rules can also credit actual work before a job finishes. Pauses and failed attempts to continue do not teach; training and physical progress commit together, and a person retains what they learned if they later abandon the job.
People can explicitly offer a lesson or ask someone to teach during an actual reachable meeting. The requested teacher independently chooses according to their teaching preference or their own fear or felt obligation; a lesson requires a stronger teacher and does not bind either person to future cooperation. Fear or obligation can also motivate autonomous teaching independently of whether the teacher willingly agreed to meet. Active past-life skill can support an offered lesson without copying that skill to the current person.
Each participant can interpret a completed lesson with their own optional signed changes to trust, affinity, fear, and felt obligation. These reactions can differ by partner; failed or declined lessons do not create them.
The ignored `concentrated_skills_benchmark` measures three composed boundaries. Set `LEGACY_PRACTICE` to exercise unfinished-work learning, and `LEGACY_PEOPLE` and `LEGACY_WORKERS` to vary population and concurrency.
An individual can also decide to offer a revocable promise to a helper whose requested work they personally witnessed. The helper is selected from that person's actual memories, and neither the offer nor acceptance moves goods or compels later delivery.
Alternatively, an individual may offer after personally observing a refusal, and separately choose to ask again after making an offer. A failed job is distinct from a refusal; an unseen outcome creates no such memory.
An individual goal may count a helper's personally witnessed completion as progress when `recognize_help` is enabled. The work-request owner must retain at least one memory; an unseen or failed job does not count, and recognizing work does not transfer its output or create an obligation.

Optional personal practice minds remember bounded experiences of their own food shortage, received gifts, and unanswered requests. After recurring shortage, a person can combine quantities and reserves from an actually received gift into a proposal to that helper during a real encounter. The recipient independently evaluates it on a later boundary, with private approval separate from outward adoption under fear or felt obligation. The author can ask again, and an adopter can choose whether to give real carried food while retaining the proposed reserve. No proposal transfers goods or binds anyone; failed or unanswered requests remain distinct from refusals. Signed personal weights influence candidate terms, and an individual can optionally revise quantity after unanswered requests.

People may also develop a bounded personal value for conditional food aid from their own related experiences. Individually configured signed reactions to receiving, giving, and unanswered requests change their later proposal evaluations; the same experience can move different people in opposite directions. Actions under pressure do not imply approval, and a learned value does not force a person to act.
An optional precedent preference lets a person compare a new aid proposal with their own bounded memories, giving individual weights to the person involved, good, and quantity difference. They interpret the selected experience using their own signed setting. This supports unfamiliar combinations of known facts within the food-aid domain without sharing private memories or making the resulting decision binding.

An individual's signed proposal-affinity weight can make their own affinity toward the author influence private approval in either direction. Pressure can still lead them to adopt a proposal they privately disapprove of.

An adopter may communicate a readiness claim during a real encounter, including a deliberately false claim when they lack the goods or reserve. The listener receives only the statement and independently believes, doubts, or ignores it; they may condition a later request on believing it. The actual transfer still checks physical custody and can fail despite an accepted claim. A controlled person can choose the statement directly.
Individuals may also choose to offer carried goods to a particular local animal based on its observable health. Their willingness can change, and fear or felt obligation toward another person can motivate them independently. Each offer consumes real goods; only food matching the animal's private diet reduces its hunger. Simultaneous offers are staged independently before the animal and shared grazing patches advance.
People can also misstate remembered regional food when sharing ordinary information. A recipient can choose how to interpret a later direct observation that differs from the report, including changes in affinity or trust, or no change at all. Taking accounts can also misstate the target, good, or quantity; this changes the statement without changing the memory or physical events. The difference alone does not prove dishonesty because stock can change over time.
People may give their own trust in a requester a signed weight when deciding whether to begin or continue requested work. This connects actual experiences of fulfilled or broken promises to later cooperation without making a promise binding.
An individual can also value an outstanding offer of a particular good when considering work. Offers remain personal and revocable, and actual delivery still requires a separate physical action; withdrawing an offer can change the person's decision to continue.
Individuals may interpret a witnessed work completion or refusal with independent signed affinity changes, including different reactions toward particular people. Unwitnessed outcomes do not create these impressions.
Actual newborns may acquire a parent's general learning inclination when configured, but do not inherit their specific aspirations or memories.

With `giving.inherit_births` and family birth enrollment enabled, a newborn may acquire a parent's general helping and acquaintance-request inclinations. Experiences, remembered helpers, previous targets, and target-specific requests are not copied.

Travelers can eat carried food. People on the same directed route at the same progress are treated as together by this coarse travel model: they can meet autonomously or by choice, ask for food, give supplies, provide care, and independently propose, consider, or physically fulfill promises; contact can transmit disease. Other routes and people at the source remain separate. Each action advances the shared world and the journey.

Individual hauling preferences can move real lots between regions. A carrier can change their mind between pickup, departure, and delivery; fear or obligation may motivate an otherwise unwilling carrier. Routes, claims, carrying capacity, destination capacity, and expiry all remain physical constraints.

Authored reusable tool requirements check personal custody before a work attempt competes for materials and before each work increment. Missing or expired tools pause work without consuming the tool.

Physical work sites can limit concurrent jobs. Slots remain occupied by paused jobs, and contested site claims are provisional until inputs actually move. Individuals can construct sites from physical kits and independently choose to destroy specific idle local sites, including when motivated by fear or felt obligation. Competing attempts remove a site only once. Blueprints may retain a specified portion of the actual dated kit in site custody; after destruction, people can independently recover the remaining local debris, subject to real carrying capacity and contested physical transfers.

Named stores within a region have separate physical custody and capacities. A person can pick up goods and later deposit them in another store or regional stock, or stop while still carrying them. Each action takes a boundary; this model does not yet represent walking distances within a region.

Hungry people who choose to seek meals may obtain an actual portion from a locally observed named store when it is their best available source. They carry the food before consuming it; regional stock claims apply to regional custody rather than to a separate store.
The ignored `concentrated_named_food_benchmark` measures many hungry people competing for local stores. Set `LEGACY_PEOPLE`, `LEGACY_STORES`, and `LEGACY_WORKERS` to vary population, store count, and concurrency; population and half-population must divide evenly among stores. Add `--features profile` and `LEGACY_PROFILE` for phase timings.

People with an enabled personal goal-fetching preference can obtain missing inputs for their own active work goals from locally observed stores, physically carrying and depositing the goods before work can begin. They may stop and retain what they carry.
When input planning is enabled, they can also seek supplies for intermediate recipes in a bounded dependency search.

A claimant present when their site's debris is recovered can interpret that experience through their own signed affinity setting, independently of their feelings about using or destroying a site.

People with enabled goal-building preferences can select an available blueprint and kit for a site missing from their own current work plan. With input planning enabled, they can also plan bounded production of a missing kit and build intermediate sites needed for that production, acquiring real tools as needed. Cycles and search depth are bounded; later reviews rotate through recipe alternatives. They respect or disregard personal claims according to their own preference.
