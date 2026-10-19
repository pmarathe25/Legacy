# Ideas

This document tracks improvement ideas for the Legacy simulation.
Items which have not been implemented are marked `[NOT DONE]`.
Completed items should be marked `[DONE]` after they have been verified.

- [DONE] Language should arise naturally from phonemes as people try different sounds to communicate. Agreed upon sounds (which can happen using gestures or show and tell) can eventually proliferate as a full language. Names should be derived from the language itself or its phonemes. Not every language will use every phoneme that can be physically generated.
- [DONE] When choosing names, characters should consider whether other important characters in their lives have the same or similar names and if that will cause confusion. The cost of confusion can be balanced against their reverance/respect for other characters - so you still might have a child named after a beloved family member, but we shouldn't have every child always named the same thing or have the same two or three names constantly repeat in every single family line. We shouldn't explicitly prevent such a thing, but we should change the cost model for choosing names.
- [DONE] After language is generated, we can also have characters write poetry, music, books, etc. For the sake of the player, we will probably also want to keep some kind of decoder/translator which we can probably do by tracking how the language evolves and how the characters themselves explain and share it with each other.
- [DONE] There must be some way of communicating _other_ than language - for example, a baby should provide some type of signal for its needs, and it should be possible for a caregiver to be intelligent enough to guess or anticipate those needs. This is not a mandate to change the simulation in one way or another, but to expand the types of characters that _can_ exist. Some characters might be better at understanding verbal or non-verbal communication than others. Some may never be able to communicate at all.
- [DONE] Assess any duplication or unclear division of responsibilities among the crates. Every crate should have a single, clear purpose. We don't want crates that do everything but we also don't want to split the logic for a single concept across multiple different crates. We should split crates or consolidate them as necessary. Ensure the architecture remains modular such that we can change one feature without needing to touch lots of other crates.
- [DONE] Model basic physics such that characters can invent machines and other complex systems using existing objects. The rules can remain simple while complexity comes from composition.

## Implementation notes

The communication and naming work was initially verified with 383 workspace tests, strict Clippy, formatting, and a fresh 20-year history. Language currently uses procedural phonetic vocabularies, local demonstrations, simple explanations, and personal decoding. Human grammar and fluent prose remain future extensions. Poems, songs, and books are procedural compositions.

See [communication](crates/game-communication/README.md), [naming](crates/game-naming/README.md), [creative works](crates/game-works/README.md), and the [feature ownership review](docs/design/feature-ownership.md) for behavior and boundaries. Transient run evidence stays under `target/history/ideas-2026-10-17/`.

Basic mechanics now composes finite material parts into force/travel graphs. Friction,
load feedback, overload, bodily effort, experiments, and physically demanding recipes
share that model. Intelligence and attention bound voluntary prototype search;
remembered problems motivate experiments, and observed capacities can be taught.
The current physics is quasi-static and acyclic; inertia and fluids remain extensions.
The expanded workspace passed 484 Rust tests, including mid-experiment save/load,
material conservation, failed manual work, newborn capability, and transmission of
observations. See [physics](crates/game-physics/README.md),
[mechanics](crates/game-mechanics/README.md), and
[invention](crates/game-invention/README.md). Further attention and invention policy
work is tracked in [the autonomy follow-up](plans/implemented/character-autonomy-followup.md).
