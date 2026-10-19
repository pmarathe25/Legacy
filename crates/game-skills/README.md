# game-skills

`game-skills` tracks per-person learned skill levels with diminishing returns and deduplicated work attribution IDs.

**Why it matters:** Characters improve at gathering, farming, crafting, construction, and music through actual completed work, while work-ID deduplication prevents double-counting practice from the same task.

## Responsibilities

- **Skills resource (`Skills`):** Stores each person's named skill levels and compact set of attributed work IDs (`skills.levels`).
- **Diminishing-return practice (`Practice`):** Increases a person's skill level when a new `work_id` is credited, scaling gains by expressed learning capability from [`game-vitality`](../game-vitality/README.md) and diminishing as the skill level rises.

## Boundaries

- **Personal learned levels only:** Tracks practice earned during a person's own lifetime; borrowed past-life memories in [`game-invocation`](../game-invocation/README.md) remain separate and never overwrite personal skill records.

## How It Works

- **Compact attribution bitmap:** Stores seen `work_id`s using sparse offsets or dense bitmap pages so thousands of completed work intervals compress cleanly in memory while serializing to the canonical ordered format.
- **Idempotent work credit:** Submitting `Practice` twice with the same `(person, skill, work_id)` is a no-op on the second call.

## Game Integration

- **Plugin / Entry Point:** Registers `plugin(&mut Builder)` (`skills`, resource `skills.levels`).
- **Requests & Events:** Handles `Practice`; emits `Practiced`.
- **Callers:** Credited by [`game-ecology`](../game-ecology/README.md), [`game-agriculture`](../game-agriculture/README.md), [`game-workshop`](../game-workshop/README.md), [`game-projects`](../game-projects/README.md), [`game-pastimes`](../game-pastimes/README.md), and [`game-guardians`](../game-guardians/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-storage`](../game-storage/README.md), [`game-sim`](../game-sim/README.md), [`game-traits`](../game-traits/README.md), [`game-vitality`](../game-vitality/README.md).
- **Testing:** `cargo test -p game-skills` verifies diminishing practice curves, duplicate `work_id` rejection, bitmap page compaction, and save/load round-trips.
