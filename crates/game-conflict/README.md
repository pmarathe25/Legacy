# game-conflict

`game-conflict` defines encounter and map state for tactical conflicts. It ranks eligible fighters by initiative and fatigue, checks a fighter's turn movement against map bounds and obstacles, and provides reach, strike, and capture rules. Maps also place fighters, track obstacle integrity and exits, and support deterministic routes, movement steps, and bounded search through observed geometry.

`game-sim` applies these rules to combat actions, actor health, and encounter timing. This crate supplies the tactical rules and geometry used by that simulation.
