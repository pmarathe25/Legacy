# game-equipment

Carried insulating clothing can be equipped through `Equip`. `warmth:N` tags provide protection scaled by condition. Weekly exposure compares actual local weather, clothing, and expressed hardiness; harmful cold produces real bodily damage. Automatic decisions apply only in enabled campaigns and exclude the current player.

Condition declines with use. `Repair` consumes one real item tagged `repair-cloth` from the garment's accessible local inventory. Equipping another garment retains the wearer's current condition, so swapping does not restore durability. Clothing remains counted in cargo. Condition currently belongs to a wearer's equipped slot, rather than to unique transferable garment instances.
