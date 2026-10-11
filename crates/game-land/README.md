# game-land

Stores soil fertility, moisture, and capacity on land entities. Undeveloped plots and cultivated fields share this component, so cultivation preserves the same soil.

The crate owns soil bounds and typed storage access. Agriculture and ecology choose how work, weather, and recovery change it. Soil has no knowledge of crops, climate runtimes, people, or property rights.
