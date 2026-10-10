# game-api

`game-api` defines the renderer-independent contract for interacting with the game. A `Command` inside a `Submission` expresses a request; a `CommandReceipt` records its acceptance and sequence. Shared types make these requests serializable and give callers a consistent vocabulary for game actions.

`PlayerView`, events, and advance results expose committed observations, ongoing activity, and simulation progress. The crate also re-exports public protocol types used by those inputs and observations. It describes the interface; authoritative state changes and game rules are handled elsewhere.
