# game-mechanics

Owns physical apparatus and manual operation. Components remain finite cargo in a
ground inventory; removal or overload disables their contribution. Broken material
is retained as debris. Operation costs bodily energy and fatigue, including partial
work, and supports save/load during an experiment.

Production owners use receiver-owned PrepareLabor and FinishLabor requests. Only
personally learned working capacities guide aid selection. Physical outcomes are
resolved by game-physics. Discovery, observation interpretation, and voluntary
prototype goals belong to game-invention.

Loaded control handles overflowing capacity probes without assuming the operator
is too weak. Final work rechecks current strength and agility; a worker cannot
retain an earlier pace after impairment. The model conservatively caps the whole
work interval at the lesser starting/current capability, rather than integrating
continuous changes within that interval.

Physics-focused simulations cover 20,000 generated transmission networks,
5,000 load-control trials, and 256 composed apparatus trials with interruptions,
breakage, finite materials, and 64 mid-trial cross-worker replays. Set
`LEGACY_PHYSICS_REPORT_DIR` to an absolute path under `target/` when running
`cargo test -p game-physics -p game-mechanics` to save compact JSON summaries.
These are physical stress simulations, not population-history evidence.
