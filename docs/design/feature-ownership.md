# Feature ownership review

Feature ownership separates physical state, personal evidence, policy, and composition at the boundaries below. Rust modules within a crate are implementation organization; separate crates need distinct responsibilities or dependency boundaries.

| Concept | Owner and boundary |
| --- | --- |
| Communication | `game-communication` owns sound associations, grounding, decoding, and instruction channels; `game-signals` owns perceived nonverbal cues. It does not own the truth of a report, ideology, care decisions, or creative artifacts. |
| Information | `game-knowledge` owns remembered facts and forecasts; `game-testimony` owns report delivery; `game-inquiry` owns empirical reflection and explanation. Communication determines whether spoken information can be understood; a successful utterance does not make a claim true. |
| Social interpretation | `game-relationships` owns private opinions, remembered statements, and directed regard. `game-social-life` owns consensual encounters and their action clocks; `game-companionship` owns initiator selection and bounded search. |
| Advocacy and membership | `game-discourse` owns advocacy and interpretation of pressure purposes. `game-affiliation` owns admission, departure, and succession. Discourse is now composed explicitly, removing affiliation's implicit registration and dependency on advocacy. |
| Names | `game-names` owns identities and personally heard names. `game-naming` owns choice costs and contextual labels. It consumes communication forms rather than maintaining another phoneme catalog. This state/policy boundary also keeps literal fixture names usable. |
| Creative works | `game-pastimes` owns activity duration, materials, cancellation, satisfaction, and practice. `game-works` owns the resulting compositions and their interpretation. `game-items` tracks physical identities through cargo transfers and destruction. A composition is not a second cargo unit. |
| Provisioning and care | `game-food-memory` projects personal evidence; `game-provisioning` compares sources and exposes the acquisition facade; `game-food-actions` executes physical plan stages. `game-nurture` composes voluntary delivery for someone else. Keep callers on the provisioning facade instead of duplicating travel or harvesting in care. |
| Physiology | `game-body` is pure arithmetic, `game-metabolism` derives expressed rates, `game-soma` owns bodily state, and `game-physiology` composes actions. These retain distinct dependency and testing boundaries; combining them would couple numerical forecasts to hosted effects. |

The concrete coupling removed here was advocacy registration inside affiliation. Communication development was also consolidated into vitality's shared growth curves and expressed-ability function, removing a separate maturation calculation. Works uses one physical-access test for both visibility and reading. No crate consolidation was justified by overlapping mutable state. Stale body/physiology ownership descriptions were corrected. The other splits above separate records, policy, or physical execution; their facades and documentation now identify the owner rather than treating each crate as a complete copy of the concept.

New code uses receiver-owned requests and mutates only its own resources. Composition and versioned save adaptation stay in assembly; new feature state is empty and inactive in older saves. Current saves retain strict manifest and state validation. Dependency direction and existing source budgets remain enforced.

`game-choice` collects bounded typed opportunities before `game-characters` compares them. Providers do not consume the action slot during collection. Satisfaction, attention, remembered failure, and physical costs affect motivation. Creative works follow completed actions without an annual creation gate. See [learning](learning.md) for the generic observation and explanation boundary, and [the autonomy implementation record](../../plans/implemented/character-autonomy.md) for remaining validation work.

`game-information` owns information interests, questions, and voluntary response
proposals. `game-testimony` owns evidence delivery, `game-inquiry` owns cognitive
explanation, and `game-literacy` owns physical written records. `game-search`
owns remembered-route searches for people and goods; livelihoods invokes it when
local production planning has no solution. None of these owners inspect another
character's private knowledge to select whom the searching character should ask.
