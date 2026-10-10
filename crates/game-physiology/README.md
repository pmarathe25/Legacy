# game-physiology

`game-physiology` provides shared body arithmetic for the game. It advances hunger, health, and fatigue over elapsed time, applying hunger harm, rest-based healing, exertion fatigue, and supplied health or fatigue effects. Fractional changes carry across updates, and results stay within configured health and fatigue limits.

The crate also finds the first time a stable interval crosses a body threshold, allowing the simulation to stop at death or another boundary. Callers provide activity and effects; clinical conditions and care are handled elsewhere.

The `traits` module owns physical predisposition, growth, training, and expression deadlines. Trait definitions and their inheritance bindings are supplied as content; optional `genetics` integration reads raw alleles without moving body rules into genetics.

The crate registers plain components in the host's shared store; it owns no second store and exposes no engine-specific types. `Body` contains vitals and fractional remainders. Missing bodies stay absent; configured limits remain explicit integration inputs.
