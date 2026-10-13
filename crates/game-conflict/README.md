# game-conflict

`game-conflict` owns encounter admission, initiative, rounds, tactical perception, movement, reach, strikes, obstacle damage, capture, and default combat autonomy. `World` retains validated rule metadata. Profiles, maps, encounters, participation, and captivity live in registered components in the shared store.

A Map belongs to its stable map ID. Area `MapRef` and local endpoint `RoadMapRefs` provide keyed scene lookup; reversed road coordinates resolve to the same normalized point. Per-person `Participation` provides active and latest encounter lookup without scanning every encounter. Fighter coordinates, memories, and search state belong to their encounter component.

`prepare_opening` freezes relevant rows before `prepare_start` computes first initiative and stages an encounter alongside its prepared scene. The application combines those effects with activity interruptions and reserves IDs through the generic host transaction. Schema rejection publishes no rows, allocations, events, or revisions.

`prepare_opening_after_motion` reads a Transport-owned projection while retaining the canonical opening rows. `PreparedStart::stage_after_motion` requires the matching native motion footprint and exact projected locations. Newborn profiles use `prepare_newborn_profile`, allowing Life, Person, Body and Skills owners to attach their rows to the same reserved identity.

`prepare_turn` returns a guarded encounter/map projection and typed effects. Planned damage, captivity, practice and release writes compose through private receiving `InjuryReceipt`, `ConfinementGuard`, `StagedPractice` and Force receipt proofs. `stage_with_release` admits those exact overlaps and guarded Transport release locations. Physiology owns injury and lethality, Skills owns learning, and Missions owns communicated objectives. The assembly translates public action variants and orders the prepared effects.

Conflict also owns observation, late-entry eligibility, round completion, contact, initiative driving, fallback Wait, mission/force preemption, and the default target/search policy. Drive ports supply facts or invoke named owner operations. Save validation checks component relationships against factual geography, people, body state, and registered deadlines.

Confinement consumes real carried binding stock and creates an equal binding for the captive in the same batch as custody. Forced binding requires no recipient acceptance and may exceed voluntary carrying limits. The `force` module owns surrender, restraint, and escape components and policy.

Run `cargo bench -p game-conflict --bench encounters` for keyed scene/contact reads, bounded initiative updates, and a 128-fighter perception/Wait round. The benchmark includes guarded preparation and publication. Dense encounter memories and per-fighter perception remain bounded by the encounter limit; larger groups cost more than empty-memory initiative alone.

Production dependencies: `game-cargo`, `game-core`, `game-ecs`, `game-physiology`, `game-skills`, `game-spatial`, `game-transport`, and `serde`. Owner tests additionally use the generic host without importing the assembly.
