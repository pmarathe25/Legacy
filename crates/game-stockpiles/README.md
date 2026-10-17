# game-stockpiles

**Purpose:** A person can set surplus possessions in a finite, owned ground cache to make room in a carried container. Goods remain physical, local, and accounted for.

`StoreSurplus { person, inventory, needed_mass }` works only on the capable person's own permitted carried inventory. It moves enough mass to fit the requested load, preferring nonfood supplies with high total mass. It retains eight meals of each edible type. A cache holds four times the source container's capacity, and the owner creates at most one per area. Failed access, insufficient movable goods, or a full cache rolls back the request. More space is never invented by discarding goods or allocating another cache.

Cargo owns creation and transfers; property owns the personal claim; knowledge records both observed inventories. `Stockpiles` owns only the cache index and stored-mass total. `Stored` reports committed movement. Save validation checks physical locality and the recorded custodian's claim.

Autonomous horticulture requests room before a harvest; provisioning can request room before gathering food. Direct player actions retain explicit control over possessions. Ground caches require no building or magical container item. They are area-bound piles with finite capacity, rather than portable inventory upgrades. Shared stores, cache theft decisions, spoilage, and construction-based storage improvements remain separate mechanics.

Run `cargo test -p game-stockpiles` for quantity conservation, ownership, capacity, rollback, observation, and save/load checks.
