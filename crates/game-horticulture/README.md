# game-horticulture

**Purpose:** People can begin and tend small plantings during ordinary idle reviews. [Agriculture](../game-agriculture/README.md) continues to own seed consumption, labor, crop phases, climate outcomes, and harvested goods.

`Plant { person, crop, inventory }` creates a fallow plot, starts sowing atomically, and returns its `PlotId`. Failed seed access, missing seed, or an occupied action leaves no plot behind. The public `Plant` command exposes this operation to the player. Each person can establish one planting through this owner; its stewardship is saved in `Gardens`.

After `Enable`, `Consider` responds to `IdleReview` at most once per hour per person. Patient people may plant from seed in their own permitted carried supplies when local warmth and rain suit the crop. Existing stewards harvest ripe crops and resow fallow plots while local accessible stocks hold less than one harvest of their crop. When needed, the optional stockpiles owner can move carried surplus into a finite ground cache before harvesting. The current player is excluded from these automatic decisions. Physiology can interrupt farm work under its existing hunger policy; canceled sowing loses its seed, and canceled harvesting leaves the crop ripe.

Generated worlds begin without plots. People first collect or prepare planting stock through ordinary livelihoods. This owner does not allocate fields to everyone, create seed, move distant supplies, limit geographic land area, transfer stewardship, or select a replacement crop for an existing plot.

**Verification:** `cargo test -p game-horticulture` checks paid sowing and harvesting, failed-transaction rollback, disposition, player control, cancellation, and save/load across worker counts.
