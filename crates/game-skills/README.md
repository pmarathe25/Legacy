# game-skills

`game-skills` tracks each person's practice and level. Feature owners describe earned learning as `PracticeDelta` effects; applying a delta updates practice, fractional level progress, and any teacher mastery cap while preserving existing learning. Progress stops at the configured maximum.

Social action credit is bounded to one award per actor, skill, and simulation minute, so duplicate reports do not multiply learning. Physical actions such as strikes keep individual credit. Skill levels tune work and damage rates, and carried remainders make work totals consistent across different time slices.

Each person's `Skills` component contains practice records and semantic credit timestamps in the host's shared store. Typed `PersonId` access updates only that component. The rules-only `Runtime` applies learning and validates progress against configured caps and the supplied clock; the schema validates intrinsic row structure without capturing rules or time. Newborns start with empty records.
