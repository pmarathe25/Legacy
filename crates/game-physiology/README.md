# game-physiology

`game-physiology` provides shared body arithmetic for the game. It advances hunger, health, and fatigue over elapsed time, applying hunger harm, rest-based healing, exertion fatigue, and supplied health or fatigue effects. Fractional changes carry across updates, and results stay within configured health and fatigue limits.

The crate also finds the first time a stable interval crosses a body threshold, allowing the simulation to stop at death or another boundary. Callers provide activity and effects; clinical conditions and care are handled elsewhere.

The `traits` module owns physical predisposition, growth, training, and expression deadlines. Trait definitions and their inheritance bindings are supplied as content; optional `genetics` integration reads raw alleles without moving body rules into genetics.

Trait `Runtime` retains definitions and the next expression deadline. Personal `Development` is a plain component on the person's shared stable entity, registered by the module under `physiology/development`. Growth and phenotype methods take the shared store explicitly; store-aware validation checks development against configured training bounds after schema restore. Optional inheritance uses the opaque `AlleleSource` interface, and acquired development never changes genomes.

The trait owner selects physical training from supplied skill-level facts and scales exact work-rate fractions. Its prepared refresh evaluates old and new phenotypes, preserves the injury fraction and fixed-point remainder, and keeps dead bodies dead. Commit checks the used allele pairs, development and body rows, actor membership, definitions, and deadline before admitting one batch and advancing the expression cadence.

The crate registers plain components in the host's shared store; it owns no second store and exposes no engine-specific types. `Body` contains vitals and fractional remainders. Missing bodies stay absent; configured limits remain explicit integration inputs.
