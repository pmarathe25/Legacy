# Autonomous pressured contact

Available local people consider disjoint contacts in shuffled windows of up to 16 people within stable person-ID neighborhoods. Windows rotate between ticks, so the same person can encounter different nearby people. Neighborhoods do not change when travel or save/load splits physical blocks. A person's private fear or obligation is consulted only after a partner is selected, so it does not grant the initiator knowledge of another person's feelings. Physical availability, each person's normal preferences, and the responder's individual pressure preference determine the result.

Run `cargo run --release -p game-parallel-fused-physical --example fused_combat_rounds -- 1 100000 hot social-pressure 5`, varying the worker count, to time a concentrated boundary with many such responses.
