# Feature ownership review

Reviewed all 90 crate purpose statements, dependency manifests, and resource declarations, with focused source review of the overlapping boundaries below, while implementing the language, naming, communication, and creative-work ideas. Rust modules within a crate are implementation organization; separate crates need distinct responsibilities or dependency boundaries.

| Concept | Owner and boundary |
| --- | --- |
| Communication | `game-communication` owns sound associations, grounding, decoding, and perceived nonverbal cues together. It does not own the truth of a report, ideology, care decisions, or creative artifacts. |
| Information | `game-knowledge` owns observations, remembered facts, and delayed reports. Communication determines whether spoken information can be understood; a successful utterance does not make a claim true. |
| Social interpretation | `game-relationships` owns private opinions, remembered statements, and directed regard. `game-social-life` owns consensual encounters and their action clocks. |
| Advocacy and membership | `game-discourse` owns advocacy and interpretation of pressure purposes. `game-affiliation` owns admission, departure, and succession. Discourse is now composed explicitly, removing affiliation's implicit registration and dependency on advocacy. |
| Names | `game-names` owns identities and personally heard names. `game-naming` owns choice costs and contextual labels. It consumes communication forms rather than maintaining another phoneme catalog. This state/policy boundary also keeps literal fixture names usable. |
| Creative works | `game-pastimes` owns activity duration, materials, cancellation, satisfaction, and practice. `game-works` owns the resulting compositions and their interpretation. `game-items` tracks physical identities through cargo transfers and destruction. A composition is not a second cargo unit. |
| Provisioning and care | `game-food-memory` projects personal evidence; `game-provisioning` compares sources and exposes the acquisition facade; `game-food-actions` executes physical plan stages. `game-nurture` composes voluntary delivery for someone else. Keep callers on the provisioning facade instead of duplicating travel or harvesting in care. |
| Physiology | `game-body` is pure arithmetic, `game-metabolism` derives expressed rates, `game-soma` owns bodily state, and `game-physiology` composes actions. These retain distinct dependency and testing boundaries; combining them would couple numerical forecasts to hosted effects. |

The concrete coupling removed here was advocacy registration inside affiliation. Communication development was also consolidated into vitality's shared growth curves and expressed-ability function, removing a separate maturation calculation. Works uses one physical-access test for both visibility and reading. The inventory found 66 distinct declared resource keys and no duplicate owner keys. No crate consolidation was justified by overlapping mutable state. Stale body/physiology ownership descriptions were corrected. The other splits above separate records, policy, or physical execution; their facades and documentation now identify the owner rather than treating each crate as a complete copy of the concept.

New code uses receiver-owned requests and mutates only its own resources. Composition and versioned save adaptation stay in assembly; new feature state is empty and inactive in older saves. Current saves retain strict manifest and state validation. Dependency direction and existing source budgets remain enforced.

Remaining design work is the common proposal/choice interface in [the autonomy plan](../../plans/unimplemented/character-autonomy.md). Listener order, creation intervals, and some social policies still constrain choice. This review does not claim those have been replaced by language or by the ownership cleanup.
