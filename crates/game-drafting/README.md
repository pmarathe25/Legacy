# game-drafting

Generic parallel drafts with ordered application, conservative type dependencies
and runtime counters. This crate knows no simulation schema or gameplay rule.
Read footprints hash resource types into 256 bits; collisions only reject extra
drafts. Versions advance on every attempted write and are not undone on rollback,
so changed or restored inputs conservatively retry. Nested worker reads share an
atomic footprint. A caller supplies draft and application functions, validates
inputs, and owns effect blocking, rollback and commit.
