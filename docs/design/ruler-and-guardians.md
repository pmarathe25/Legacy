# The ruler, guardians, and reincarnation

## Confirmed rules

- One enduring lineage reincarnates as successive distinct people.
- Each life has its own body, childhood, personality, skills, relationships, and experiences.
- The successor is chosen from actual children born after the previous ruler dies. Without new births, there is no new ruler.
- The player controls the new incarnation, including as a newborn with very limited capabilities.
- The guardians have an innate connection to the ruler and can locate the successor instinctively. They still need to travel and overcome physical obstacles to reach them. The player can spectate that journey.
- During upbringing, the player can interact, make choices, and influence the guardians. The guardians decide when the ruler is ready to leave mentorship and enter the adult command relationship.
- Guardians have a loyal disposition toward each incarnation and unwavering dedication to finding, mentoring, and assisting rulers across lifetimes. They can disagree, warn, and refuse orders.
- The ruler can die and later reincarnate. Political rejection or deposition does not end player control.
- The guardians are immortal, extremely resilient, and do not suffer hunger. They can become tired, injured, or incapacitated. Destroyed bodies regenerate in place over time; imprisonment can contain a recovered guardian.
- The ruler can draw on past-life memories and abilities. Combining lives can create especially powerful effects.
- Past-life information requires active invocation. It does not automatically become current-life knowledge; deliberate notes and teaching can preserve information outside the invocation.
- Past-life use can cause physical strain, identity loss, emotional instability, reduced lifespan, and temporary loss of control. Severity increases with duration and the number of lives invoked.
- While the player retains control, the game warns before a new invocation cost and offers dismissal or acceptance.
- Accepting loss of control commits the player to the warned duration and whatever consequences follow during it. There is no emergency dismissal, cost cap, or renewed consent that prevents an outcome during that interval.
- Regular use should be practical. Direct permanent invocation penalties are very rare and reserved for very protracted use; ordinary consequences of uncontrolled actions are not capped.
- The guardians live continuously, retain their own experience, and develop more gradually.

## Identity and persistence

Keep a lineage record separate from each incarnation's `Character` record. It points to previous lives, their preserved memories or capabilities, the current incarnation if one exists, and the state of the transition. There is at most one living incarnation belonging to this lineage.

A former ruler remains a historical character. Their promises, family, political relationships, property, and mistakes are not all converted into the new child's personal state. Other people and institutions decide which claims or duties they recognize across lives.

Each guardian is one persistent character across recovery. Destroying a guardian's body does not create a new identity, clear their memories, or cancel their obligations. Equipment and carried goods remain subject to ordinary loss and destruction.

## Reincarnation lifecycle

Track succession, reaching the successor, care, and development separately:

```text
Succession: living incarnation → awaiting an eligible birth → living incarnation
Guardian access: awaiting successor → instinctively located → approaching → reached
Care and mentorship: actual caregivers and guardian availability
Development: the character's age, capabilities, and life stage
```

The access states describe progress, not a mandatory linear sequence. Instinctive location does not guarantee physical access: guardians can be distant, imprisoned, or regenerating. A child continues aging during their absence. Reaching an adult does not send them back through childhood or imply that mentorship occurred. Care can come from the birth household, and later separation can require another journey.

Political recognition is separate from guardian recognition. An identified adult can still be an exile, prisoner, private citizen, or disputed claimant.

On death:

1. Resolve ordinary death, interrupted actions, possessions, dependents, offices, and local consequences.
2. Preserve the life in the lineage's history without giving the next life automatic ownership of its estate.
3. Mark the lineage as awaiting a successor from subsequent births; continue the guardians' mission.
4. Continue simulating institutions, relationships, economy, and conflict.
5. On the selected eligible birth, bind the new character to the lineage and move player character control to them.

Births come from the population's ordinary reproduction system. Do not manufacture a child, pregnancy, or family to guarantee succession, or select someone born before the previous ruler's death. If no eligible birth occurs, the lineage remains without a living ruler, however long that lasts. Selection among eligible births and the presentation of this waiting period remain to be specified; neither requires a guaranteed birth deadline.

The innate connection supplies the guardians with the ability to locate the ruler; discovery does not depend on gathering mundane clues or testing candidate children. Reaching the ruler still requires routes, travel, and responses to danger or confinement. Represent the connection as an explicit source of guardian knowledge, not general access to world state. Other people do not receive that knowledge automatically, and political recognition still depends on their own beliefs and evidence.

## Childhood and player agency

The player does not switch into controlling a regent or guardian as their primary character. The realm keeps acting while the player experiences a child's limitations.

Proposed interactions grow with development:

- Early dependency: attention, comfort, attachment, and simple preferences.
- Early childhood: play, curiosity, companionship, and requests for activities.
- Later childhood: interests, lessons, practice, friendships, disagreement, and requests to travel or meet people.
- Adolescence: deeper study, supervised responsibilities, political opinions, and negotiated independence.

These are candidate interaction categories, not settled ages or a scripted childhood sequence. They should affect relationships, learned skills, confidence, values, and opportunities without letting an infant issue state orders.

Guardians consider the child's wishes alongside safety, development, their own judgments, and their duty. A rejected request should be understandable and may prompt another conversation. Childhood choices should influence the current person rather than merely allocate bonuses to an otherwise identical future ruler.

The transition out of mentorship is at the guardians' discretion. Their assessments should follow experience with the ruler and their judgments of readiness, rather than a universal birthday automatically unlocking obedience. Keep those assessments separate from physical development and each institution's legal adulthood or political recognition. A guardian judging someone ready does not create physical capabilities or require agreement with every subsequent order.

Time acceleration and interruption rules need a separate pacing decision. The confirmed ability to spectate and interact rules out treating every childhood as an unconditional timeskip.

## Guardian action and recovery

Guardians use the same movement, equipment, work, social, and injury systems as other characters, with different physiology and capabilities. They cannot be in two areas at once or personally defend a settlement while searching elsewhere.

Proposed body states are active, impaired, incapacitated, and regenerating. Time always provides a path back to bodily function. Aid can improve conditions or accelerate recovery, but food, money, or a rare material must not become an absolute prerequisite for recovery. Physical recovery does not guarantee freedom from confinement.

Recovery is not restoration of all lost possessions. An enemy can interrupt duties by incapacitating a guardian, isolate them, or exploit their absence, even though permanent death is impossible.

Record the destruction location and regenerate the body there. Do not respawn at a shrine, beside the ruler, or outside a prison. A recovered guardian remains contained while the physical means of imprisonment remain effective; escape or rescue uses actual actions. Recovery durations and detailed confinement mechanics need tuning and implementation, but neither can make the guardian mortal or make recovery an automatic escape.

Guardians evaluate orders as loyal autonomous people. They can disagree, warn, or outright refuse, including in the adult relationship. Explain the refusal through their judgment and knowledge. Their dedication to finding, mentoring, and assisting the lineage is unwavering; ordinary relationship updates cannot turn that mission into an abandoned goal. Loyalty to a ruler does not require agreement with an order or allegiance to the ruler's current government. Guardians continue acting toward their mission during absence, incapacity, and gaps between incarnations.

## Past-life memories and abilities

Store past-life knowledge, practiced capabilities, relationships, and significant experiences separately from the current person's own knowledge and skill. A memory can supply useful evidence while still being incomplete, outdated, or politically disputed.

Access to the past life's information ends when its invocation ends. Querying that memory does not automatically insert it into the current person's ordinary knowledge. The ruler can deliberately write information down or teach it to someone while invoking, using actual time, suitable materials, and communication. Those records or people can supply information later through ordinary reading and learning. An invocation view is not itself a recording or teaching action.

The design covers three different uses:

1. Recalling information: laws, languages, techniques, places, relationships, and historical events.
2. Drawing on a prior person's practiced abilities for a current task.
3. Combining multiple lives for an exceptional effect.

Access conditions, invocation duration, compatibility, benefit, and cost must be explicit. Government, crafting, teaching, and diplomacy need the same treatment as combat. A cost chosen only for battle could make noncombat invocation effectively free and dominant.

All five costs listed in the lore are now selected. Their exact thresholds, rates, recovery, and interaction with ability strength remain to be designed. Access through childhood and how borrowed mastery affects practice also remain open.

## Invocation burden and recovery

**Proposed mechanism:** Track active lives, uninterrupted duration, and a persistent burden state. Burden rises with time and with the number of simultaneous lives; combining lives increases its rate more than using one alone. The effective rate and projected thresholds are inspectable.

Ending an invocation stops additional invocation burden, while accumulated strain recovers over time. Briefly dismissing and recalling a life must not reset exposure and bypass the tradeoff. Different symptoms can have different recovery times.

Use a practical ordinary-use range, followed by escalating costs:

| Range | Intended experience |
| --- | --- |
| Ordinary use | Frequent access is viable, with clearly communicated upkeep and recoverable effects. |
| Sustained use | Increasing physical strain and emotional instability create reasons to stop and recover. |
| Severe overextension | Identity interference and temporary loss of control can occur after explicit warnings and acceptance. |
| Very protracted overextension | Rare permanent identity effects or lifespan loss become possible. Warn separately while the player retains control; an accepted control-loss interval allows consequences to proceed. |

This is a proposed arrangement of the confirmed costs, not a settled duration table. Past lives can be used in daily governance and labor without routinely forcing permanent damage. Permanent consequences must not arise as an unannounced random penalty for ordinary use.

Gate permanent consequences on both severe burden and very protracted exposure. Invoking many lives briefly can sharply increase temporary costs, but must not turn a short burst into a permanent penalty. The warning states when both conditions would be met; sustained use with insufficient recovery counts as exposure rather than resetting on every dismissal.

These exposure rules govern direct permanent invocation penalties. Actions taken during loss of control can also cause injury, death, broken relationships, and other lasting world consequences on ordinary timescales. Apply the normal rules to those outcomes without a protective cap or artificially adding penalties merely because control was surrendered.

Record actual consequences on the current incarnation. Do not silently erase a permanent effect when the invocation ends. Whether any identity consequence carries across reincarnation needs an explicit decision.

## Warning and consent contract

For player-directed activation or adding a life, preview the benefit, current burden, known ongoing cost, and time to the next new cost under current conditions. If activation itself incurs a cost, require the choice before activation takes effect.

While the player retains control, before a new cost type or severity begins:

1. Stop time advancement at a safe boundary before applying it.
2. Explain the consequence, magnitude or bounded risk, expected duration, recovery, and whether it is permanent.
3. Offer to dismiss the active lives or continue under the stated cost.
4. Record the accepted cost and its limits if the player continues.
5. Warn again before exceeding those limits or introducing another cost.

Do not demand a new confirmation every minute for an unchanged, already accepted rate of fatigue. Consent covers an explained type, rate or severity band, and duration or condition. Accepting strain does not authorize later lifespan loss.

Changing the number of active lives or conditions refreshes the projection while the player retains control. Time acceleration, queued actions, save/load, and combat must preserve pending warnings. Neither an NPC decision nor an automated time advance can initiate acceptance of control loss on the player's behalf.

Dismissing lives while in control prevents a newly warned cost before it begins; it does not reverse damage already incurred. Before control loss starts, explain its duration, the loss of action and dismissal authority, and the possibility of additional costs, irreversible consequences, or death that cannot all be predicted individually. Acceptance authorizes that risk for the interval. During it, autonomous behavior and ordinary consequences proceed without further consent, exceptional dismissal, or a cap at the costs previously previewed. Notifications may explain events, but must not create an escape from the commitment.

The agreed duration bounds this control-loss episode, not its consequences. At its end, restore player authority subject to the character's actual condition; death, injuries, or resulting confinement persist. Before further player-controlled invocation advances, apply the ordinary warning rules to its current conditions. Save/load, combat rounds, and queued commands must preserve the remaining interval and prevent player action or dismissal during it. Session pause, observation, and saving remain available without restoring character control. The autonomous controller's behavior is still to be designed.

## Acceptance scenarios

- A successor comes from an actual post-death birth, with no overlap between living incarnations. A world with no new births produces no replacement ruler.
- The player controls the child, while the guardians search and decide how to raise them.
- A childhood request can influence a guardian without becoming an adult order.
- A guardian can judge readiness and refuse an adult order while remaining loyal and dedicated to the lineage's mission.
- A guardian survives starvation conditions without eating, can be incapacitated, and regenerates at the destruction location. Effective imprisonment still contains the recovered guardian.
- Political institutions and citizens keep acting during death, search, childhood, and guardian recovery.
- Finding the true successor does not automatically restore political power or transfer former estates.
- A child incarnation's death starts another transition without duplicating the lineage.
- A guardian locates the successor without mundane clues but must still travel or escape confinement to reach them. Delayed arrival preserves the successor's actual development and history.
- Ordinary repeated invocation with adequate recovery avoids direct permanent invocation penalties.
- Greater duration or more active lives increases burden, with no reset exploit from rapid dismissal.
- While the player retains control, advancing a month stops before an unaccepted cost just as minute-by-minute play does. Accepting fatigue alone does not authorize a new permanent consequence.
- Accepted loss of control allows subsequent costs and harmful outcomes without another consent stop or dismissal. Pausing, saving, loading, and changing advance chunks preserve the commitment and its consequences.
- Dismissal removes access to invocation-only information without erasing a deliberately written record or another person's learned knowledge.
