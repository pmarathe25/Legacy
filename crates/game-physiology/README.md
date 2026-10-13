# game-physiology

`game-physiology` owns numerical body rules and physical development. Hunger, healing, harm, and supplied strain or injury contributions combine before one health/fatigue clamp. Fractional remainders preserve exact integration across time slices; dead bodies remain dead.

`Body` and personal trait `Development` are canonical components in the shared store. Registration, getters, insertion, and intrinsic validators belong to this crate. `World` retains only `Rules`; `Rules::expressed` applies a supplied phenotype and `newborn` constructs the initial body. Contextual save validation consumes explicit exemption, carried-load, movement, activity, and phenotype facts.

`ActivityKind` describes activity identity. The owner selects exertion, rest, exhaustion, recovery, and mortality effects. `next_boundary` clips hunger's slope change before finding the earliest stable-rate threshold. Work deadlines consume completion facts from Workshop. `EvaluatedInterval` retains numerical results and the component reads that supplied them; `validate_interval_outputs` rejects stale joins and missing participants.

Prepared receiving effects stage body changes into a shared batch. Meals use `prepare_meal_relief`; completed numerical intervals use `prepare_interval_reception`. Instant injuries use `prepare_injury`, whose receipt identifies the exact target and damage and reports only a new lethal transition. Receipts validate their final batch before publication, so Cargo debits, Combat turns, or Workshop progress can publish with the body effect. A coordinator performs death aftermath once after a successful lethal publication. `receive_injury` uses the same prepared path for standalone changes.

Fatigue charges provide a typed receipt. A survival decision tolerates only that receipt's exact body edit in the same batch; other writes or removals reject before publication.

The `traits` module owns authored predisposition definitions, growth, training, phenotype expression, and refresh cadence. Optional `genetics` integration reads opaque allele pairs. `prepare_refresh_from_facts` selects participants and interprets maturity and skill facts, preserves injury fractions, and advances the deadline only after admission. `register_newborn` owns the baseline development and `validate_references` checks participant and allele coverage.

Owner tests exercise fractional integration, stale facts, strict schema rejection, and actual Body–Combat–Skills composition without importing assembly. This crate exposes plain components and typed effects, with no private entity store or engine-specific API.
