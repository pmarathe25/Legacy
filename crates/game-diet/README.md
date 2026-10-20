# game-diet

Pure nutrient reserves, intake, learned effects, dependence and food evaluation.
`game-nutrition` owns simulation resources, meals, physical effects and notifications.
The existing nutrition data types remain available through its reexports.

An evaluation borrows one immutable personal diet. It calculates current nutrient
balances once for all food alternatives, while retaining learned nutrients, cravings,
exact saturation and authored nutrient keys. It cannot outlive or mutate its inputs.

Dependencies: game-core, game-objects, serde.
