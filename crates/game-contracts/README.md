# game-contracts

Runtime data and serialization contracts: resources, typed requests/events, wire
messages and resource codecs. The simulation host keeps owner checks, dispatch,
transactions, schedules and commits. `game-sim` reexports the existing public API.
There is no gameplay policy or execution path here.

Dependencies: game-core, serde, serde_json, rustc-hash.
