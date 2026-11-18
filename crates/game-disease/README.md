# Disease

Data-defined strains spread through actual contacts supplied by the host. Incubation, infectious duration, damage, transmission probability and immunity are authored rules. Each person may carry multiple strains; immunity and source arbitration apply separately to each strain. Contacts use the committed generation, so newly infected people cannot transmit until a later boundary. Multiple active strains contribute saturating damage, and a deterministic seeded choice selects one source for each newly acquired strain.

Each strain may independently configure ordinary-contact, combat-strike and birth transmission. Birth exposure uses the mother's contagious cases at the start of the boundary, even if she receives treatment later. Newborn infections do not damage or transmit in that boundary. The validated generation path retains already-staged infections for that boundary and suppresses duplicate exposure. The host stages all exposure, damage and events with the rest of the world. Physical contacts can stage infections before the disease generation; those cases cannot cause damage or spread until a later generation, and recovery receipts remain distinct from reinfection.

Treatment definitions associate a medicine with a strain. A successful physical course clears only matching infection episodes present when the choice was made; it does not erase unrelated strains or newly acquired episodes. Personal treatment preferences and actual medicine consumption are owned by `game-economy-treatment`.

Interval scenarios can set `dynamics.disease.time_unit_millis`. Incubation,
infectious duration, immunity, and the spacing of damage pulses then use that
duration. A new episode starts at the exact time of the contact or birth.
The first damage pulse occurs one unit into the infectious stage, with the
last pulse at the end of that stage. Newly exposed people cannot spread the
illness at the same instant. Saved episodes and history receipts retain their
elapsed timestamps. Contact opportunities follow the systems that generate
actual encounters.

The ignored `coinfection_exposure_benchmark` measures the disease phase for two strains. `LEGACY_PEOPLE` sets population, `LEGACY_WORKERS` selects concurrency, and setting `LEGACY_CONCENTRATED` makes all contagious sources expose one target. The benchmark excludes the rest of the composed simulation.
