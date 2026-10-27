# game-drafting

`arbitration` provides a read-only provisional admission step for indivisible
multi-resource intents. Each intent names its actor and any additional required
participants; intents sharing a participant or resource form a conflict group.
Independent groups can be resolved in parallel. Priority classes precede
a deterministic actor rotation based on a caller-supplied logical round. A
claim is admitted only if all participants and quantities are available; a failed
claim occupies neither. Results are ordered by stable intent ID. This is not a
physical reservation, commit, permission check or guarantee
of starvation freedom. Owners must validate and apply effects against current
state; denied provisional claims do not imply personal failure.

Generic parallel drafts with ordered application, conservative type dependencies
and runtime counters. This crate knows no simulation schema or gameplay rule.
Read footprints hash resource types into 256 bits; collisions only reject extra
drafts. Versions advance on every attempted write and are not undone on rollback,
so changed or restored inputs conservatively retry. Nested worker reads share an
atomic footprint. A caller supplies draft and application functions, validates
inputs, and owns effect blocking, rollback and commit.
