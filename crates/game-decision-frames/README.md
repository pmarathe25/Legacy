# game-decision-frames

Opt-in fixed-step proposal preparation. The existing choice owner registers its
requests and stores its optional frame in `game-offers::Offers`. There is no new
plugin manifest entry or state in ordinary saves.

Configure a positive interval to schedule priority-49 frames. Higher owners gather
participants and refresh their observations, then submit one Batch. Independent
providers prepare actor cohorts on the same world; scratch drafts cannot change
world state. Effectful providers retain live ordered execution. Prepared motives
and alternatives may be reused until the next frame, even after world changes;
this is a model experiment, not an exact optimization. Physical events, urgency,
arbitration, receiver validation and execution remain ordered.

`ConfigureDemand(true)` selects the optional demand-driven clock from
`game-decision-clock`. Character wakes and forecast mind deadlines request the
next useful boundary; empty periods generate no frame jobs. This changes global
identity allocation, so it is measured as another model variant. Periodic timing
remains the default.

Frame proposals and counters are saved, allowing exact continuation within this
model. Disabled state is omitted. No cache or provider classification is global:
each registry owns its callbacks. A provider observed to attempt an external
effect stops speculative preparation and retains its original callback.



One pool submission covers the actor-by-provider work matrix. Adaptive chunks
target at least two tasks per worker; each chunk shares one private scratch
transaction. A one-worker batch uses one chunk per actor. Callback lookup retains
the registered owner. Actor action
identities are captured once per cohort rather than once per provider. Raw results
join before owner-controlled storage; application still occurs at ordinary reviews.
A provider attempting effects is disabled for the entire cohort and recorded in
saved frame state, avoiding worker-count or reload-dependent classification.
Frames share immutable prepared state through Arc so review checkpoints do not
copy the cohort. Runtime hit/miss counters are omitted from saves.

Completed provider groups survive a later provider's blocked effect or failure;
the failed provider's partial proposals are discarded. Final choice, food planning
and effectful providers still execute live. An actor entering a periodic review
after the boundary cohort was gathered can also use live provider fallback. The
pilot therefore does not freeze every decision input or parallelize action
resolution. Coalescing action-completion reviews can leave an actor idle until the
next boundary; the interval changes activity rates as well as information latency.

`Tune` independently enables idle recovery, pure rechecking of selected snapshot
proposals, and auxiliary food inputs. Default tuning is omitted from saves.
Rechecking runs the original provider against current personal inputs before an
attempt; a vanished proposal is skipped as an arbitration conflict. Continuing
actions keep their existing continuation path. A still-offered action that fails
keeps ordinary failure learning. Effectful or failed rechecks fall back to the
original execution path. Runtime conflict counts are diagnostic only.

Auxiliary providers subscribe to a separate event and never enter normal offer
arbitration. They share the cohort's single pool submission, with a separate task
per auxiliary query so expensive food searches do not block provider chunks.
Saved typed input payloads remain readable after reload.
`decode_pending` also inspects a selected request by its checked wire key, using
the typed fast path or a saved-payload fallback without invoking its handler.

The opt-in `meal_arbitration` policy requires whole-actor preparation and a
positive grouped-commit size. It changes the ordering of character reviews;
the character owner validates and executes every accepted action. It does not
promise starvation freedom for changing cohorts. The additional `meal_wave`
option executes multiple admitted self-meals in a phased transaction: stock and
its listeners remain ordered, surviving bodies may update in parallel, and all
meal body and mood effects settle before `Fed` observers. It is a separate
behavioral model with hook, interruption and failure fallback; other actions
continue through the ordered path. Current cohorts are capped at 128; body
updates run serially at that size after a short synthetic worker screen.

The independent `nonurgent_millis` experiment retains a bounded shortlist of
nonurgent offers while recomputing food, hunger, fatigue and scoring on frame
reviews. Urgent evidence clears it; an expired or exhausted shortlist refreshes.
Owner requests still validate at execution. Its wake can be delayed until the
next frame, and a hunger crossing has no independent interrupt detector. Cached
motives and provider observations can change behavior; it defaults to zero.

Each cohort captures the actor's action identity as a resolution precondition.
If an earlier resolution newly involves that actor in an activity, its obsolete
decision defers to the next boundary. This prevents a stale prepared turn from
cancelling a just-started joint activity. An action that merely finishes does not
trigger this guard. Preconditions survive save/load with the cohort.

`Tune::batch_decisions` delegates the cohort to `PlanCohort` subscribers instead
of preparing individual providers. The character-owned `game-decision-intents`
adapter performs one complete preparation per actor and stores typed decisions
through `StoreDecisions`. Prepared effects and pure-provider provenance survive
reload. Named-provider rechecks preserve the conflict-memory mitigation after
preparation moves out of the provider cache. This freezes more inputs than the
original hybrid model and is separately opt-in.

`Tune::urgent_immediate` is a separate opt-in interrupt experiment. Injury,
arrival and urgent care alerts schedule a live review at the current simulation
time instead of waiting for the next frame. Other wakes also interrupt if the
actor's hunger has already reached the configured urgent threshold. This does not
create an independent hunger-threshold detector. The interrupt discards that
actor's prepared frame inputs, coalesces same-time wakes and keeps ordinary
frame scheduling for other decisions. Timing and choices differ from both the
original frame and event-driven models; measure outcomes as well as performance.

`Tune::commit_batch` optionally resolves whole-actor frames in groups of 1–128
actors. Character-owned scheduling runs after cohort preparation; ordinary
individual wake jobs remain the default. Grouped commits require whole-actor mode.
