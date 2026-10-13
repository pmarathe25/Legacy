# game-skills

An actor dying at an interval boundary still earns its opening activity's elapsed practice. `prepare_actor_learning` (or grouped `prepare_actors_learning`) stages those credits with the physical and archive receivers. `EvaluatedLearning::prepare_acknowledgement` retains the evaluated remainder exclusively; finish it after accepted publication, or drop it when publication is rejected. Applied actors leave the later Skills phase, and approved teacher credits update the witness while pupils keep their original mastery caps.

`game-skills` owns personal learning: selection of trained skills, elapsed practice, teacher mastery caps, level deadlines, and bounded semantic event credit. It reads and writes each person's canonical `Skills` component in the host's shared store. Its runtime retains only validated rules.

`evaluate_learning` consumes factual activity, body, equipment, care, and social interaction inputs. Its `EvaluatedLearning` retains rules and the teacher Craft practice used for live lesson caps. `prepare_learning` validates those witnesses and prepares the final Skills-phase effects; unrelated semantic credit to the teacher remains compatible, while a changed teaching cap fails before publication. It selects Farming for actual gathering/cultivation, Craft for construction/workshop/craft play, Medicine for treatment/boiling, and the appropriate pupil and teacher practice for lessons. Invalid or blocked intervals earn nothing. Social intervals stop at the interaction's completion time; route instruction trains the teacher's Leadership without inventing a pupil skill.

`prepare_event` receives typed birth and adopted-agreement facts. Newborns receive empty records; accepted agreement participants and amendment proposers earn Negotiation practice. Semantic action credit is limited to one award per actor and skill per simulation minute. Work and damage adjustments and skill views have owner entry points, while exact work remainders preserve totals across time slices.

`prepare_practice` retains affected opening rows, including rows whose practice cap produces no write. `stage_into` appends owner writes to a generic ECS batch and immediately binds a `StagedPractice` guard using the shared `ReadSet` protocol. Final validation checks runtime rules, entity presence, opening rows, batch affinity, and exact admitted projections. Independent owners may append unrelated rows. `commit` uses that same staging and validation path before atomic publication.

`validate_world` combines owner progress and semantic timestamp validation with factual person membership and physical-runtime presence. Saves keep the existing `skills/personal` schema. Owner tests exercise the production learning, event, deadline, staging, view, damage, and save-validation interfaces without the assembly.

Production dependencies: `game-core`, `game-ecs`, and `serde`.
