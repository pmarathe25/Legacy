# game-knowledge

Owns per-person facts with source, observation time, and confidence. Views return recorded beliefs, including stale information. Inspection is an action; passive queries never teach facts.

`ObserveStore` records stock, location, capacity, and mass. `ObserveSurroundings` learns local roads and exposed resource patches, and discovers accessible local containers not previously inspected. Refreshing remembered inventory stock requires `ObserveStore`. `ObservePatch` cannot inspect a distant resource.

`Tell` requires both people at the same place and passes only the selected fact. `Report` is the lower-level delayed-delivery operation used by communication systems and fixtures. Neither operation also transfers unrelated knowledge. Reports retain the original observation time and commit order, with reduced confidence. A delayed report cannot replace newer evidence recorded at the same clock time.

`route` and `next_step` construct routes from recorded roads, including known endpoints not yet visited. Actual transport validates each road separately. Newly closed roads remain believed open until observation or communication supplies the change.

Run `cargo test -p game-knowledge`. Tests cover local discovery, stale stock, report delivery, spoken locality, and unknown intermediate roads without the application.
