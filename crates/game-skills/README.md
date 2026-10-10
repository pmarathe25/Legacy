# game-skills

`game-skills` tracks each person's practice and level. Feature owners describe earned learning as `PracticeDelta` effects; applying a delta updates practice, fractional level progress, and any teacher mastery cap while preserving existing learning. Progress stops at the configured maximum.

Social action credit is bounded to one award per actor, skill, and simulation minute, so duplicate reports do not multiply learning. Physical actions such as strikes keep individual credit. Skill levels tune work and damage rates, and carried remainders make work totals consistent across different time slices.
