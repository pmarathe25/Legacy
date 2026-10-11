# game-land

Stores soil fertility, moisture, and capacity on land entities. Undeveloped plots and cultivated fields share this component, so cultivation preserves the same soil.

The crate owns soil bounds and typed storage access. Agriculture and ecology choose how work, weather, and recovery change it. Soil has no knowledge of crops, climate runtimes, people, or property rights.

`validate_uses` checks that each supplied land-use parcel has exactly one soil row, with no missing, orphaned, or shared ownership. Consumers supply IDs, so this invariant also works for new land-use types.
