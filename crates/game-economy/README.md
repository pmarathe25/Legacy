# Individual economic simulation

This composed simulation joins personal work, actual dated goods, food acquisition, meals, trade choices, revocable promises, individual stock claims, opinions, travel, optional family lifecycle, dependent care, cultivation, personal encounters, disease, expeditions, and ruler continuity at staged boundaries. People may assert conflicting claims, respect or disregard another's claim, and choose to claim their own delivered outputs. Caregivers and social participants can be motivated by personal willingness, fear or obligation; food must actually move. Newborns receive custody atomically; journeys retain their carried lots. Disease can spread through social encounters, caregiving, successful personal transfers, and actual births when a strain enables maternal transmission; individuals may choose to consume medicine they hold. Additional routes of exposure and treatment remain to be integrated. Succession uses actual births, guardians travel and mentor, and invocation warnings precede their physical costs. It remains an incomplete slice, not yet the complete campaign host. All arrangements arise from individual choices; it contains no institutional or payroll entities.

`boundary_time` derives an exact elapsed time and configured quantum from the persisted tick. `advance_boundaries` runs a bounded number of whole boundaries, streams each committed event set, and reports completion, a caller-requested yield, or a failure with any pending invocation warning. Earlier boundaries remain committed when a later one stops. This API does not yet advance partway through a boundary; arbitrary-duration progress and saved in-flight intervals remain unimplemented.

The separate `core_interval` API composes already-admitted physical work, travel, hunger, nutrition, and pending food effects for fractional progress inside one existing quantum. It freezes work rates, saves and resumes the in-flight state, and settles periodic effects once at the quantum boundary. It can publish completed nonperishable work early into the job's physical output custody and publish completed living travelers at their destinations. After pending events are published, callers can explicitly start a batch of physically reserved jobs at the current offset; their workers must be able and local, and their work progresses only afterward. It does not run the rest of the economy or select actions, and is not arbitrary-time advancement of an `Economy`.

Callers can also stop a batch of active workers at a partial offset. Their work stops immediately; reserved inputs remain in job custody until their physical return at the boundary, which can be blocked. Stale or invalid batches change nothing, and saved intervals preserve the stopped state.

The core can also commit transfers between people in the same region, or between a person and that region's stock, at a partial offset. The caller owns the actors' choices and other social consequences. These transfers do not consume goods or touch work custody, and do not run meals, meetings, or other world decisions.

Chosen people can start a journey at a partial offset. Existing work and caller-supplied occupations prevent departure, and a closed route produces the traveler's own observation without moving them. A journey begins accumulating travel time only after departure, carries the person's actual possessions, and can cross a boundary or a save before arrival. The caller still chooses destinations and supplies occupations belonging to other owners.

In the limited `CoreHost`, optional `Travel.destinations` records each person's enabled destination preference. At scheduled decisions, eligible uncontrolled people pursue a directly connected destination after their work choices; they do not leave while resting, working, or otherwise occupied. A closed route is attempted and personally observed. This is a direct-route preference, not a general itinerary planner, and the separate whole-boundary `Economy` runner does not yet schedule these preferences.

Chosen living people can eat food they actually carry at a partial offset. Only accepted consumption reduces hunger immediately; health and nutrition effects are retained for the next boundary. Failed consumption and invalid batches do not grant effects, and a resumed interval preserves both the custody and pending effects.

The host can also stage autonomous meal choices at an offset: hungry people with `seek_meals` select an unexpired portion from their own custody. Controlled people are excluded, and the host supplies people occupied by other activities. This does not yet schedule the rest of their decisions or food acquisition.

`start_autonomous_work` lets enabled personal work policies choose and reserve inputs at an offset, while respecting personal willingness, health, hunger, and existing active work. The host must exclude actors blocked by owners absent from this core, including site, tool, claim, and other activity constraints. This slice does not process work requests or the rest of the autonomous decision system.

When a meal consumes identified property, the host uses `prepare_meals` to stage its item-owner changes before committing the physical and physiological effects. The `eat` convenience method rejects identified consumption. Likewise, ordinary early-work and boundary convenience calls reject identified input consumption; the host must use their staged forms to update item owners.

`CoreInterval::next_deadline` observes the earliest work completion, traveler arrival, optional hunger threshold, or quantum boundary, including ties. `progress_to_deadline` stops at that time. The core can publish nonperishable work and arrivals at the offset; other early deadlines remain pending until settlement.

The host can defer selected completed jobs, including perishable production, until the boundary. Deferred jobs keep their inputs and workers reserved, and no longer block other early completions. The host chooses which outputs require deferral.

For identified boundary output, `prepare_settlement` exposes staged physical identities before any owner commits. A host can persist its tool owner alongside `CoreSnapshot`, stage tool conditions from the draft, validate, and then publish the core and prepared owner state. Plain `settle` does not know an external owner's rules; callers with identified goods must use the staged API. This slice does not run tool use, wear, or the rest of the economy during the interval.

The same host can use `prepare_early_work` to stage nonperishable identified output at a partial offset. It must stage and validate the item owner before committing the early draft; the convenience `publish_early_work` creates fungible output and is only appropriate when no separate item identity is required.

`CoreHost` saves the limited core together with optional autonomous-work policies and tool conditions. It coordinates carried-food choices, work admission, early production, and boundary settlement with those owners; identified consumption with no matching tool owner is rejected. The enclosing host must still supply exclusions for other activities, sites, tools, and claims, and must handle other world owners. Use `resume` for an in-flight snapshot and `begin` with new work rates after a completed boundary.

Optional timed fatigue records each person's elapsed work, travel, rest, and idle time, including across saves and early completions. At each quantum boundary it applies a duration-weighted update and retains fractional fatigue; automatic rest uses individual thresholds, while a controlled person chooses when to rest. Rest and missing required tools independently pause a job without discarding its desired rate. Time supplied as occupied by an external owner does not count as rest or idle. Fatigue consequences and automatic rest take effect at boundaries, not at the exact instant a threshold is crossed.

The host can also retain personal goals, directed opinions, and optional personal agreements. At scheduled decisions a person with an active goal and asking preference can address a local request. A local worker evaluates their own fear, felt obligation, affinity, trust, preferences, and addressed offers; willingness is not required, and a started worker remains free to abandon the job. People can choose to propose or accept promises, and the maker's own disposition determines whether making a promise changes their felt obligation. Promises do not reserve goods or guarantee delivery. A debtor may choose delivery according to their own policy and directed feelings; chosen deliveries move real goods between local people and advance agreements only for accepted physical transfers. Refusal and witnessed work outcomes update the existing bounded personal memories. Completed personal work advances the person's goal, while help advances it only when the goal's owner recognizes help and is present. Requests do not bind anyone, and insufficient inputs leave a willing response pending. Food and cultivation memories and other social systems are not included in this host.

The host also accepts work chosen by an enclosing social owner without requiring the worker's willingness flag. Physical eligibility and input custody still apply, and the worker can abandon the job. Physical transfers likewise do not establish consent or permission.

Optional `CoreSites` adds site capacity and maintenance to the limited host. Site slots are provisional until physical inputs are reserved; completed bound jobs wear their sites, broken sites cannot admit new jobs, and personal repair choices consume carried material. Occupied sites cannot be repaired. Slot bindings and wear survive reload, and repair events clear at the boundary. The host still relies on its caller for claim restrictions and does not run site construction or destruction.

People can also choose to repair damaged identified tools. Repair material is consumed from personal custody, and accepted repairs reset the same item's damage; failed attempts leave its condition unchanged. Optional data-defined tool requirements check each worker's carried goods at admission. A configured identified tool must be unbroken, and it receives one wear increment only when the physical job starts. If required goods leave the worker's custody, work pauses until the goods return; the saved rate survives reload and boundaries. An item worn to its limit at the start can finish that job but cannot admit a new one until repaired. Ongoing work checks carried quantities rather than binding a particular item identity.

A host can enable a periodic decision schedule and call `advance_scheduled`. The runner publishes tied arrivals and nonperishable work, defers perishable completions to the boundary, and then stages carried meals, requests and responses, and autonomous work. Decision phase and timing survive save/load. `advance_scheduled` stops at the boundary for an explicit `settle`; `advance_continuous` settles and opens subsequent intervals, preserving decision timing and retained work rates. At a boundary, externally blocked or unable workers pause; their retained rate can resume at a later boundary. If an action fails, the result reports the failure and earlier events remain committed. The host supplies consistent external activity exclusions across split advances. Hunger decisions are periodic, not a complete world action scheduler.

The ignored `core_host_autonomous_work_benchmark` measures policy selection and reservation through this saved host. One 49,999-start run took 61.7, 42.3, 29.1, and 27.3 ms at one, two, four, and six workers, with matching saved-state fingerprints. It does not measure the complete campaign.

Set `LEGACY_SCHEDULED=1` on that benchmark to include 500 ms of scheduled advancement and three decision points. A 50,000-person run took 113, 73, 50, and 40 ms at those worker counts, with matching fingerprints. This is a limited-core workload and remains sublinear.

Set `LEGACY_CONTINUOUS=1` instead to include 1,500 ms, eight decision points, a settled boundary, and early work completion. A 50,000-person run took 329, 204, 139, and 120 ms at those worker counts, with matching fingerprints. Setup is excluded; this does not run the complete economy.

After adding social phases, skipping phases for disabled owners reduced a repeat from 397, 237, 161, and 145 ms to 340, 216, 149, and 129 ms, with matching fingerprints in both runs.

Set `LEGACY_CHOSEN=1` to measure explicit chosen work for people whose willingness flag is false. A 49,999-start run took 57.4, 35.5, 24.4, and 22.4 ms at the same worker counts, with matching fingerprints; social decision-making is not included.

The ignored `core_host_work_requests_benchmark` includes 49,999 fear-motivated personal request responses and physical reservations. One run took 77.9, 48.0, 33.1, and 27.8 ms at one, two, four, and six workers, with matching saved-state fingerprints. Setup and later work outcomes are excluded.

Setting `LEGACY_OFFERS=1` instead uses 49,999 addressed unfunded offers to motivate the responses. One run took 78.6, 47.3, 35.2, and 31.5 ms at those worker counts, with matching fingerprints; setup, fulfillment, and later work outcomes are excluded.

The ignored `core_host_agreement_delivery_benchmark` measures 25,000 independent chosen deliveries. One run took 25.7, 17.4, 13.1, and 13.8 ms at one, two, four, and six workers. After removing a duplicate agreement-state clone and adding dense-ID lookup, a repeat took 24.6, 17.8, 17.1, and 12.8 ms, with matching fingerprints. These samples do not establish a clear scaling improvement. Setup and personal delivery decisions are excluded.

With `LEGACY_AUTOMATIC=1`, that benchmark includes 24,999 debtors choosing delivery and physical settlement. One run took 35.8, 24.2, 18.8, and 18.1 ms at the same worker counts, with matching fingerprints. One additional debtor is occupied by pre-existing work.

The ignored `core_host_offer_choices_benchmark` measures 24,999 personal proposals followed by recipient acceptance. One run took 13.7, 9.3, 8.4, and 8.2 ms at one, two, four, and six workers, with matching fingerprints. Setup and later delivery are excluded.

The ignored `core_host_site_admission_benchmark` includes personal work selection, site-slot arbitration, and physical reservation. For 49,999 starts across 50,000 sites, one run took 90.9, 54.5, 38.3, and 34.3 ms at one, two, four, and six workers. With `LEGACY_SITE_SCARCE=1`, one shared site had room for 24,999 of those contenders; a run took 65.4, 33.6, 23.6, and 21.2 ms. With `LEGACY_SITE_SETTLE=1`, the timed section instead completed 50,000 jobs, settled the boundary, and applied site wear; a run took 132.9, 84.0, 59.4, and 57.8 ms. Each workload produced matching fingerprints across worker counts; setup is excluded.

Adding `LEGACY_TOOLS=1` checks and wears an identified tool for each accepted start. A many-site run took 124.0, 73.1, 50.0, and 44.8 ms at those worker counts; combined with `LEGACY_SITE_SCARCE=1`, a run took 80.7, 51.9, 33.9, and 32.1 ms. Accepted counts, tool wear, and fingerprints matched across workers.

Adding `LEGACY_TOOL_PAUSE=1` with `LEGACY_TOOLS=1` instead times transferring 49,999 tools to one person and pausing the affected jobs. One run took 112.9, 72.3, 51.7, and 44.6 ms at the same worker counts, with matching fingerprints. Setup and the original work starts are excluded.

With `LEGACY_FATIGUE=1`, the site benchmark times progress, completion, site wear, and duration-weighted fatigue for 50,000 workers. One run took 169.8, 93.3, 66.2, and 62.6 ms at one, two, four, and six workers. Splitting progress into 20 steps with `LEGACY_FATIGUE_STEPS=20` took 263.2, 161.1, 115.1, and 100.9 ms. All runs produced the same fingerprint. These isolated timings are sublinear and exclude admission and setup.

The ignored `core_host_departure_benchmark` times 49,999 chosen departures, their travel, fatigue, and boundary settlement. One run took 19.3, 12.6, 9.1, and 8.0 ms at one, two, four, and six workers, with matching saved-state fingerprints. Setup is excluded, and the timings are sublinear.

Setting `LEGACY_AUTO_TRAVEL=1` includes selection from 49,999 personal destination preferences. One run took 23.7, 14.6, 9.5, and 10.5 ms at those worker counts, with matching fingerprints. Setup is excluded.

The ignored `core_host_site_repair_benchmark` measures 49,999 people repairing separate sites using carried materials. One run took 47.3, 31.2, 24.5, and 26.2 ms at the same worker counts, with matching fingerprints and setup excluded.

The ignored `core_host_tool_repair_benchmark` measures 49,999 identified-tool repairs with carried materials. One run took 81.3, 49.4, 39.1, and 34.5 ms at one, two, four, and six workers, with matching fingerprints and setup excluded.

The ignored `core_host_goal_asking_benchmark`, with `LEGACY_PEOPLE=200000`, creates 99,999 requests at one endpoint. One run took 45.7, 30.8, 21.3, and 21.4 ms at one, two, four, and six workers, with matching fingerprints. Setup and responses are excluded; this workload levels off around four workers on the measured machine.

The ignored `composed_core_progress_benchmark` with `LEGACY_CORE_ARRIVALS=1` measures publication of 50,000 arrivals among 100,000 people. One run took 5.87, 2.86, 1.58, and 1.25 ms with one, two, four, and six workers, with matching fingerprints. This measures only arrival publication in the limited core.

The ignored `interval_work_admission_benchmark` starts 49,999 jobs from one shared stock within a partial interval. A specialized physical path reduced one instrumented run from 111, 70, 52, and 46 ms to 73, 45, 35, and 31 ms with one, two, four, and six workers, with matching fingerprints. Set `LEGACY_ADMISSION_STOCK=25000` to admit only 25,000 jobs; one run took 83, 52, 37, and 34 ms with matching fingerprints across worker counts. These isolated admission workloads are sublinear and do not measure whole-economy time advancement.

Packing homogeneous single-input starts avoids per-job transaction vectors. In a later instrumented comparison, the same abundant-stock workload fell from 69.6, 43.6, 33.4, and 30.5 ms to 59.5, 37.0, 27.5, and 25.6 ms with one, two, four, and six workers, with identical fingerprints. With stock for 25,000 jobs, a separate run took 69, 42, 30, and 27 ms. These are synthetic partial-core measurements, not full-campaign timings.

At 199,999 packed starts, one run took 260, 163, 123, and 107 ms at one, two, four, and six workers, respectively, with matching fingerprints. Increasing the workload did not produce linear scaling.

Building the packed holders and transfers in chunks reduced a subsequent instrumented comparison at that size from 263, 162, 116, and 114 ms to 250, 148, 108, and 96 ms at the same worker counts, with matching fingerprints. These timings remain sublinear and measure only partial-core work admission.

Set `LEGACY_ABANDON=1` on the admission benchmark to stop every accepted worker at the same offset. Stopping 49,999 workers took 6.25, 2.98, 1.59, and 1.23 ms at one, two, four, and six workers in one run, with matching fingerprints. The timing excludes admission and later physical returns.

Set `LEGACY_DEFER=1` instead to let the admitted jobs finish and defer their publication. Deferring 49,999 jobs took 3.67, 1.75, 0.89, and 0.73 ms at those worker counts in one run, with matching fingerprints. Admission, progress, and boundary production are outside that timing.

Set `LEGACY_IDENTIFIED_SETTLE=1` to stage and commit identified output with tool conditions at the boundary. A 50,000-job run took 170, 99, 69, and 74 ms at one, two, four, and six workers, with matching physical and tool-owner fingerprints. It excludes earlier admission and does not run whole-economy decisions.

Set `LEGACY_IDENTIFIED_EARLY=1` to stage and publish identified nonperishable output before the boundary. A 49,999-job run including staged tool conditions took 161, 95, 67, and 59 ms at those worker counts, with matching combined fingerprints. Earlier admission and later delivery are excluded.

Set `LEGACY_AUTOWORK=1` to include personal policy selection, stock projection, and work admission. One 49,999-start run took 64.5, 37.8, 27.0, and 27.1 ms at one, two, four, and six workers. With `LEGACY_ADMISSION_STOCK=25000`, 25,000 starts took 33.2, 21.6, 15.9, and 14.1 ms. Fingerprints matched within each workload; these partial-core timings are sublinear.

The ignored `interval_shared_transfer_benchmark` measures transfers from one stock into existing personal custody. A 50,000-transfer run took 27.4, 16.3, 10.6, and 10.4 ms at one, two, four, and six workers, with matching fingerprints. It excludes social choices and is not a campaign measurement.

The ignored `interval_identified_meals_benchmark` measures identified consumption and tool-condition cleanup. One 50,000-person run took 64.9, 43.6, 34.5, and 30.7 ms at one, two, four, and six workers, with matching combined fingerprints. Setup is excluded and scaling is sublinear.

Set `LEGACY_MEALS=1` to time the same people consuming their received food after the transfer. One 50,000-meal run took 63.8, 39.9, 31.2, and 28.4 ms at one, two, four, and six workers, with matching fingerprints. This excludes the earlier transfer and remains sublinear.

Set `LEGACY_AUTO_MEALS=1` to include autonomous personal meal selection and consumption. A 50,000-person run took 61.6, 39.2, 27.1, and 28.6 ms at one, two, four, and six workers, with matching fingerprints; earlier transfers are excluded.

At 200,000 meals, a separate run took 283, 176, 139, and 151 ms at those worker counts, with matching fingerprints; six workers were slower than four on that run.

An independent-source custody path improved a later instrumented 200,000-meal comparison from 262, 169, 126, and 113 ms to 253, 151, 111, and 95 ms at one, two, four, and six workers, with matching fingerprints. Build with `--features profile` and set `LEGACY_PROFILE=1` to print meal phase timings.

Avoiding redundant receipt sorting and lookups when meal records already align reduced a later 200,000-meal comparison from 252 to 235 ms with one worker and from 99 to 97 ms with six, with matching fingerprints.

An incapacitated traveler does not move during the core interval, even if an immortal recovers at its closing boundary. The existing coarse economy path can move such a traveler after recovery in the same boundary, so this case is not equivalent between the two paths.

Work sites can optionally wear with completed production and require real repair materials before they admit more work. An individual may choose to repair an idle site, including under their own fear or felt obligation; competing activity and material consumption are resolved at the physical boundary.

With local walking enabled, people at the same endpoint can make personal work and hauling requests, offer and fulfill promises, and exchange carried goods. A request or promise does not compel the other person. Work at a regional site and departures from the region still require reaching the stock endpoint. Moving or performing another chosen action can make someone unavailable for an interaction.

The existing `autonomous_hauling_requests_benchmark` accepts `LEGACY_HAUL_STORE=1` to place all requesters and their shared carrier at one store. On the measured machine, 100,000 people over two boundaries took 72.8 ms with one worker and 33.7 ms with six, with matching state fingerprints and 99,999 requests retained. This isolated workload scales sublinearly.

Optional combat wounds persist independently of health. They can pause or reduce continuing work and optionally prevent tactical movement, retreat, and strategic departure. A real, accepted treatment using configured medicine can clear an earlier wound; a new wound in the same boundary remains. The player's local view shows who is visibly wounded without exposing private causes.

An idle autonomous person can choose to treat their own wound through an individual medical policy naming themselves as patient. They consume their own supplies, and after their wound clears they do not repeat the wound treatment merely because their health remains low.

Optional renewable animal products grow as bounded individual biological state. Collection uses an animal's response and the collector's choices, produces real dated goods, and competes with other actions involving that animal. An existing physical hold can support explicitly forced collection without implying consent.

Optional written food claims use identified physical pages. Writing consumes configured paper and ink in the same transaction that creates a page, preserving the author's chosen claim, original witness and observation time. A controlled person can write an actively invoked memory, including a deliberately altered quantity, hand over a specific page, or read a held page in a known script. Authored `authors` entries let independent people choose to write a remembered region once, with a personal `food_delta` that can misstate the quantity; failed attempts retry after a delay. Enabled `study` entries let a person learn to read a script over repeated encounters with a literate person when either holds a page in that script. Observation does not require the other person's consent or convey the page's claim. A `practice` entry lets a person who can read a script learn to write it through successful sessions consuming paper and ink; people can practice autonomously or the player can initiate a session. A person with `auto_read = true` can independently choose to read a held page; their personal report-reception setting separately controls whether they retain its claim. Handover alone teaches nobody. Reading attempts are remembered so an uninterested or distrustful person is not forced to read the same page every boundary. Pages follow physical custody and are removed from the writing owner if the physical item is consumed.

Data-defined `writing.text_templates` and individual `writing.text_authors` also let autonomous people or the controlled person produce a physical page containing their chosen text. Each configured text must contain 1 to 4,096 UTF-8 bytes. Reading such a page records the encounter with the text but creates no automatic belief, obligation, agreement or action. Optional individual `writing.text_retention` preferences retain a bounded number of texts actually written or read, even if their pages are later destroyed; people without an enabled preference do not automatically acquire such memories. Re-reading refreshes a recollection without changing its original encounter time. See `content/scenarios/economy-written-text.toml`.

The writing owner's literacy, page, and reading-history collections share immutable storage across staged generations. Rust callers editing an authored literacy list can use `literacy_mut()`; serialized data remains ordinary arrays.

The ignored `independent_autonomous_readers_benchmark` measures one full economy boundary with `LEGACY_PEOPLE` independent readers; `LEGACY_WORKERS` selects concurrency. It excludes world construction and reports the median of three warmed runs.

`independent_text_retention_benchmark` measures the same scale of independent readers with half choosing to retain their text. On an AMD Ryzen 5 7600, 100,000 readers took a median 60.6 ms with one worker and 22.5 ms with six, producing identical fingerprints. The existing food-reader workload measured 57.8 ms and 22.4 ms in the same run. These are workload-specific results and show sublinear scaling.

The ignored `independent_autonomous_authors_benchmark` measures one boundary in which `LEGACY_PEOPLE` people independently consume materials and create identified pages. `LEGACY_WORKERS` selects concurrency. Repeat the benchmark to assess timing variation; a `profile` build with `LEGACY_PROFILE` set also prints the instrumented phase timings.

`independent_text_authors_benchmark` exercises the same physical process with data-defined text, reporting a warmed median and state fingerprint. On an AMD Ryzen 5 7600, 100,000 authors took 384.9 ms with one worker and 148.4 ms with six, with identical fingerprints. A single same-run measurement of the existing food-author workload was 398.1 ms and 161.8 ms respectively; scaling is sublinear and varies by workload and machine.

The ignored `independent_writing_practice_benchmark` measures a full boundary of `LEGACY_PEOPLE` independently consuming materials and learning to write. `LEGACY_WORKERS` selects concurrency.

The ignored `independent_study_benchmark` measures the learning phase for `LEGACY_PEOPLE` independent encounters, excluding fixture construction and the rest of the economy boundary. `LEGACY_WORKERS` selects concurrency.

The ignored `concentrated_food_search_benchmark` measures a complete boundary of people departing toward remembered food. Set `LEGACY_RETENTION_STRIDE` to enable personal retention for every Nth person, and `LEGACY_PEOPLE` and `LEGACY_WORKERS` to control population and concurrency.

Existing personal giving preferences can move written pages as physical goods; the recipient's separate reading and report-reception choices still apply. The ignored `independent_written_circulation_benchmark` measures three complete boundaries in which `LEGACY_PAIRS` authors write, hand pages to recipients, and recipients read them. `LEGACY_WORKERS` selects concurrency.

`independent_written_journeys_benchmark` extends that workload through seven complete boundaries, including recipients traveling based on the claim, observing actual stock, and updating their own relationships. It uses the same `LEGACY_PAIRS` and `LEGACY_WORKERS` settings.

Giving policies and responses are shared immutable collections across staged generations. Rust callers constructing `Giving` can convert vectors into the shared fields; callers changing them use `policies_mut()` or `responses_mut()`, which detach on write. Their serialized representation remains ordinary arrays.

Regional exposure uses finite shelter and authored seasons; seasonal planting, growth and harvest rules affect actual fields. Travelers face source-region exposure. Shelter allocation and crop growth are deterministic across worker counts and saves.

Social isolation and sociability influence individual voluntary encounters; personal fear or obligation may independently motivate a response. Newborns can inherit dispositions without inheriting other people’s relationships.

For affinity-guided meetings, each person can give affinity a positive, negative, or zero weight and override it for particular people. The ignored `affinity_social_benchmark` measures this selection; set `LEGACY_NEGATIVE_SOCIAL` to exercise people seeking those they dislike and `LEGACY_WORKERS` to vary concurrency.

Individuals can gather finite or renewable local resources, subject to personal choice or social pressure and actual stock capacity. Only successfully delivered output depletes a site or supports a personal claim. Health can recover under authored rules, with rest providing an additional benefit. Purpose deprivation tracks completed work and blocked goals.

Optional individual wildlife grazes the same finite resource nodes after staged human gathering and regrowth. Each animal ages, feeds, can starve, and may reproduce with a local mate; young inherit diploid traits from actual parents. The animal generation and resource deductions commit atomically. Controlled people can observe local animals without seeing their hidden genetics or pregnancies.

People can ask for food, give supplies, or provide medical care through real local contact and consumed medicine. Requests expire and do not require a response; actual food delivery satisfies them. Individual willingness and feelings influence autonomous help; treatment can expose a caregiver to infection and does not compel later cooperation. All distinct successful physical contacts can transmit disease, including several contacts involving the same person at one boundary.

People may attempt to take goods from another living person's custody. The target's own standing preferences determine whether they notice and resist; not resisting does not imply approval. A resisted attempt moves no goods, and the actor may separately choose to escalate into a real tactical encounter. Actual transfers compete for the same finite lots and capacity as other actions. Participants can configure independent signed interpretations of outcomes, and an unaware target gains no direct experience. Taking does not create a universal theft label or automatic obligation.

The ignored `taking_world_benchmark` measures the complete boundary for `LEGACY_ATTEMPTS` attempts and twice as many people. Set `LEGACY_SHARED` for one target, `LEGACY_RECOVERY` to make targets incapacitated and enable personal recovery, and `LEGACY_WORKERS` to vary concurrency; add `--features profile` and `LEGACY_PROFILE` for phase timings. The benchmark is an isolated synthetic boundary rather than a complete campaign.

With local walking enabled, physical taking requires the actor and target at the same endpoint, and a witness must be there to observe it. `LEGACY_LOCAL_TAKING=1` exercises those checks in the existing benchmark. On a Ryzen 5 7600 with 50,000 attempts among 100,000 people, one and six workers measured 63.6 and 28.1 ms for independent targets and 32.5 and 13.7 ms for one contested target; each pair had matching fingerprints.

The ignored `independent_combat_benchmark` stages many separate two-person
encounters in the same world and reports import and three-boundary times
separately. Set `LEGACY_ENCOUNTERS` and `LEGACY_WORKERS` to vary the workload.
It is a synthetic tactical workload, not a complete war or mixed campaign.

The ignored `independent_reinforcement_benchmark` measures a full boundary of personal attempts to join existing fights. Set `LEGACY_HOT=1` to put all attempts into one fight and exercise its finite tactical capacity; `LEGACY_ENCOUNTERS` controls the number of helpers, and `LEGACY_WORKERS` controls concurrency.

When local walking is enabled, an attack or reinforcement requires the participants to be at the same endpoint. Add `LEGACY_LOCAL_CONFRONTATIONS=1` to the existing confrontation benchmark or `LEGACY_LOCAL_REINFORCEMENT=1` to the reinforcement benchmark to exercise this check. On a Ryzen 5 7600, 10,000 people initiating independent confrontations measured 6.7 and 3.8 ms with one and six workers; 5,000 independent reinforcements among 15,000 people measured 15.1 and 7.8 ms. Each pair had matching fingerprints.

The ignored `combat_wounds_benchmark` measures a full boundary of real strikes producing wounds. Set `LEGACY_HOT=1` to direct all strikes at one person; `LEGACY_ENCOUNTERS` and `LEGACY_WORKERS` select the workload and concurrency.

The ignored `immobilized_departure_benchmark` measures food-search decisions and departures with half the population immobilized. Set `LEGACY_PEOPLE` and `LEGACY_WORKERS` to vary the workload and concurrency.

The ignored `autonomous_wound_care_benchmark` measures a full boundary of independently chosen self-treatments, with physical medicine consumption. Set `LEGACY_PEOPLE` and `LEGACY_WORKERS` to vary the workload and concurrency.

The ignored `tactical_site_damage_benchmark` measures a full boundary of attacks on maintainable work sites. `LEGACY_ENCOUNTERS` controls strike count, `LEGACY_HOT=1` directs all strikes at one site, `LEGACY_AUTONOMOUS=1` makes individual fighters choose the site attack, and `LEGACY_COLLAPSE=1` enables collapse. With both hot contention and collapse enabled, `LEGACY_BARRIER=1` adds a site-owned tactical barrier. `LEGACY_WORKERS` controls concurrency.

Regional work sites occupy the stock endpoint when local walking is enabled. Tactical site attacks check that location, including orders restored from a save. Add `LEGACY_LOCAL_SITE_ACCESS=1` to the existing site-damage benchmark, and `LEGACY_SITE_REMOTE=1` to place all fighters at a different store. On a Ryzen 5 7600, 5,000 reachable attacks measured 5.8 and 2.8 ms with one and six workers; remote attacks measured 6.7 and 3.3 ms and caused no site damage. Each pair had matching fingerprints.

The ignored `route_closure_batch_benchmark` attempts a mixed batch of open and closed routes and advances a boundary. `LEGACY_PEOPLE` controls the number of travelers, `LEGACY_DISTRIBUTED=1` gives them independent routes, and `LEGACY_WORKERS` controls concurrency.

With `LEGACY_LOCAL_WALKERS=1`, the existing `traveling_social_benchmark` places people at the same recorded point on a local walking leg and checks their autonomous encounters. A 100,000-person, two-boundary run took 83 ms with one worker and 41 ms with six; the state fingerprints matched. Add `LEGACY_DISTRIBUTED=1` to split people between two progress points: one run took 88 and 48 ms, again with matching fingerprints. With the profiling feature and `LEGACY_PROFILE=1`, the benchmark also reports phase timings. These measurements are specific to these workloads.

The ignored `personal_route_planning_benchmark` measures a full boundary of food seekers planning from individual route reports. `LEGACY_PEOPLE` sets the population, `LEGACY_MULTIHOP=1` exercises multi-leg paths, and `LEGACY_WORKERS` sets concurrency.

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

Regional animals currently occupy the regional stock endpoint when local walking is enabled. A person at another endpoint cannot directly interact with them, and a restraint ends when its handler walks away.

Set `LEGACY_ANIMAL_LOCAL=1` on `concentrated_animal_defense_benchmark` to include local-walking reconciliation. For 100,000 people, one and six workers took 29.5 and 14.7 ms with independent holds, and 15.7 and 8.0 ms with one shared animal. Each pair produced matching fingerprints; these isolated workloads scale sublinearly.

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

Authored directed route closures block new departures during their scheduled interval, while journeys already underway continue. Each actual attempt produces a bounded, dated personal observation; other people do not receive an automatic update. Blocked members of a departure batch do not prevent unrelated travelers from leaving.

When `personal_route_planning` is enabled in content, autonomous food seekers select paths only from their own bounded route reports and estimated durations. An unknown path is unavailable, and an old closed report can be rechecked after the food seeker's review delay. A physical attempt updates that person's report without broadcasting it or replacing their travel-time estimate. Hauling, guardian and expedition planning still use the shared route graph; every departure is subject to physical closures.

An individual can share a remembered route report through a real meeting, including an intentionally altered claim about access or duration. People with individual `route_sharing` preferences can do so autonomously during encounters, willingly or under their own fear of or felt obligation toward the listener. The listener's own report-reception preference determines whether it is accepted; the speaker sees contact rather than that private decision. Reports retain the stated observation time and their immediate source. A newer report for the same route takes precedence.

The ignored `autonomous_route_sharing_benchmark` measures a full boundary of encounters and personal route reports. `LEGACY_PEOPLE` sets population and `LEGACY_WORKERS` sets concurrency.

Individual hauling preferences can move real lots between regions. A carrier can change their mind between pickup, departure, and delivery; fear or obligation may motivate an otherwise unwilling carrier. Routes, claims, carrying capacity, destination capacity, and expiry all remain physical constraints.

Authored reusable tool requirements check personal custody before a work attempt competes for materials and before each work increment. Missing or expired tools pause work without consuming the tool.

Physical work sites can limit concurrent jobs. Slots remain occupied by paused jobs, and contested site claims are provisional until inputs actually move. Individuals can construct sites from physical kits and independently choose to destroy specific idle local sites, including when motivated by fear or felt obligation. Combat can also collapse an explicitly configured site at its wear limit, interrupting bound jobs. Competing attempts remove a site only once. Blueprints may retain a specified portion of the actual dated kit in site custody; after destruction, people can independently recover the remaining local debris, subject to real carrying capacity and contested physical transfers.

An authored shared-work rule can allow a bounded crew within an existing job's reserved site footprint. Each autonomous helper independently decides whether to assist a named lead and recipe, including under their own fear or felt obligation; the controlled person can also choose to assist for one boundary. A helper's skills, injuries, tools, hunger, availability and site claims affect their contribution. Deterministic receipts distinguish offered effort from progress actually applied; the lead's job retains the one set of physical inputs and outputs. Helpers gain their own practice where that recipe supports practice learning, but helping grants no ownership or automatic payment. Optional succession preferences let individuals separately choose whether to take over a departing or incapacitated lead's unfinished job, with a bounded vacancy retaining its site and inputs. A successor does not inherit the former lead's personal commitments.

The ignored `shared_work_boundary_benchmark` measures one boundary of existing jobs and helper contributions. `LEGACY_PEOPLE` sets helper count, `LEGACY_HOT=1` sends all helpers to one job, `LEGACY_CREW_LIMIT` can constrain that crew, `LEGACY_SHARED_SITE=1` places independent jobs and competing new work at one shared site, and `LEGACY_WORKERS` sets concurrency. `LEGACY_SUCCESSION=1` instead measures independent choices to take over vacant work; `LEGACY_CONTINUE=1` measures the subsequent boundary.

On an AMD Ryzen 5 7600, the optimized test profile with 100,000 candidates measured median takeover boundaries of 112.7 ms with one worker and 43.5 ms with six for independent jobs, and 25.7 ms and 9.6 ms for one contested vacancy. The subsequent boundary for 100,000 independent successors measured 116.3 ms and 45.4 ms. Each pair produced identical state fingerprints. For context, the existing independent shared-work benchmark measured 117.7 ms and 47.2 ms on the same run; these measurements show useful but sublinear scaling, not a guarantee for other machines or workloads.

Named stores within a region have separate physical custody and capacities. A person can pick up goods and later deposit them in another store or regional stock, or stop while still carrying them. Each action takes a boundary. Optional authored walking routes add travel time between stock and named stores, with pickup or deposit on a later boundary after arrival. The initial spatial slice constrains logistics rather than all social proximity, and planners cannot inspect or take from remote stores. A goal fetcher may choose a bounded search of an authored sequence of known stores, then return to stock with personally held inputs and continue their goal. A traveler may similarly choose to visit a known store for missing delivery supplies.

Hungry people who choose to seek meals may obtain an actual portion from a locally observed named store when it is their best available source. They carry the food before consuming it; regional stock claims apply to regional custody rather than to a separate store.
When local walking is enabled, direct gifts and expedition deliveries require both people to be at the same local endpoint. Someone in transit cannot hand goods to another person at a distant endpoint.
The same endpoint rule applies to treatment and food requests, including remembered-helper and acquaintance choices. A person can still treat themselves or interact with someone at the same store.
The ignored `concentrated_named_food_benchmark` measures many hungry people competing for local stores. Set `LEGACY_PEOPLE`, `LEGACY_STORES`, and `LEGACY_WORKERS` to vary population, store count, and concurrency; population and half-population must divide evenly among stores. Add `--features profile` and `LEGACY_PROFILE` for phase timings.

People with an enabled personal goal-fetching preference can obtain missing inputs for their own active work goals from locally observed stores, physically carrying and depositing the goods before work can begin. They may stop and retain what they carry.
When input planning is enabled, they can also seek supplies for intermediate recipes in a bounded dependency search.

A claimant present when their site's debris is recovered can interpret that experience through their own signed affinity setting, independently of their feelings about using or destroying a site.

People with enabled goal-building preferences can select an available blueprint and kit for a site missing from their own current work plan. With input planning enabled, they can also plan bounded production of a missing kit and build intermediate sites needed for that production, acquiring real tools as needed. Cycles and search depth are bounded; later reviews rotate through recipe alternatives. They respect or disregard personal claims according to their own preference.

Identified reusable tools may optionally have item-owned wear rules. Scenario residents can carry `items = [{ id = 1001, good = "tool" }]`; `[[tool_wear.rules]]` names the tool and repair goods, durability, per-boundary wear and repair quantity, while `[[tool_wear.conditions]]` associates damage with that item identity. Configured tool goods require a tracked, unbroken identified item for each worker or helper; generic lots do not satisfy that requirement. An idle person's willing `[[tool_wear.repairs]]` choice or the controlled person's `repair-tool` action consumes the specified material from their own custody on acceptance. Repair attempts exert the person even when materials are unavailable. Work and repair use opening-boundary custody; an item transferred in that boundary keeps its condition. A broken tool remains a physical item. An autonomous worker whose active job is blocked by a missing usable tool can choose to pause and repair a damaged carried tool, retaining the job and its reserved inputs; the job can resume on a later boundary. Completed production creates individually identified tools for configured nonperishable tool goods within the job's physical transaction; their condition starts undamaged and follows their actual custody. The saved identity cursor advances beyond imported and newly created items, including written pages. See `content/scenarios/economy-tool-wear.toml`.

Irrigation is optional and belongs to fields and individuals. The scenario names a physical regional water good, field capacity, minimum moisture for growth, evaporation, initial field moisture and a person's watering choice for each field. A choice may also respond to that person's fear or felt obligation toward a specified individual. Watering consumes actual regional stock through the common physical arbiter; denied requests add no moisture, but attempted watering still takes time and causes fatigue. Growth uses opening-boundary moisture, evaporation and accepted watering update the next boundary, and unlisted fields retain ordinary rain-fed growth. Each field and person currently have at most one configured autonomous watering choice. A controlled person may also attempt watering; simultaneous attempts at one field are resolved deterministically and each attempt retains its own outcome. Irrigation does not imply ownership, employment or payment. See `content/scenarios/economy-irrigation.toml`.
