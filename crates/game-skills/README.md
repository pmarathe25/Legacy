# game-skills

`game-skills` tracks each person’s practice and level for each skill. Practice accumulates in milliseconds, produces fractional progress between levels, and stops at the configured maximum. A teacher can limit how much new practice a learner receives to the teacher’s own mastery, while the learner keeps any progress already earned.

Skill levels increase work output and damage through configurable rates. Work arithmetic carries fractional remainders, so splitting an interval into smaller steps preserves the same total. The crate supplies these mechanics; it does not assign skills to particular activities.
