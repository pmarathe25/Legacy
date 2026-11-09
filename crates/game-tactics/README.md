# Tactical geometry

`game-tactics` owns checked tactical movement and line-of-sight geometry on deterministic, serializable fixed-point local maps. Maps are persistent world state; changing or reloading one does not regenerate its obstacles. Coordinates are integer subunits and no renderer is required.

Map primitives and generation are owned by [game-tactical-map](../game-tactical-map/README.md). The runtime index stores battle identities and actor geometry for direct or binary-search lookup. Movement updates cached points only after the batch validates; casualties leave tombstones until compaction. New encounters rebuild the index while old maps remain stored.

The current [economy CLI](../game-economy-cli/README.md) exposes tactical encounters and movement. The `movement` example measures owner command cost and the first command after casualty compaction; it is not a full-boundary performance measurement.
