# game-knowledge

Owns per-person facts with source, observation time, and confidence. Views return recorded beliefs, including stale information. Inspection is an action; passive queries never teach facts.

`ObserveStore` records stock, location, capacity, and mass. `ObserveSurroundings` learns local roads and exposed resource patches, and discovers accessible local containers not previously inspected. Refreshing remembered inventory stock requires `ObserveStore`. `ObservePatch` cannot inspect a distant resource.

Searching a previously observed store at its remembered location can disprove its presence. `ObserveStore` then records `{"absent_from": area}` under the stock key, rather than inventing its new location or contents. Food projections exclude the absent container as both a supplier and a gathering destination. Finding it again restores ordinary stock and location evidence. Uninformed remote inspection still fails, and another observer's beliefs remain unchanged.

`Tell` requires both people at the same place and passes only the selected fact. `Report` is the lower-level delayed-delivery operation used by communication systems and fixtures. Neither operation also transfers unrelated knowledge. Reports retain the original observation time and commit order, with reduced confidence. A delayed report cannot replace newer evidence recorded at the same clock time.

`route` constructs routes from recorded roads, including known endpoints not yet visited. Routing looks up only reached areas in these recorded snapshots; it does not reconstruct or copy the whole known graph for each query. Actual transport validates each road separately. Newly closed roads remain believed open until observation or communication supplies the change.

Run `cargo test -p game-knowledge`. Tests cover local discovery, stale stock, report delivery, spoken locality, and unknown intermediate roads without the application.

Road, patch, stock, and location payloads are typed numeric snapshots in memory. JSON stays at serialization boundaries; unknown schemas retain their JSON data. Facts share immutable payloads across observers while keeping individual source, time, confidence, and revision. A source change replaces the cached payload; existing beliefs and delayed reports retain their previous snapshot.

Personal fact tables support namespace ranges so planning avoids unrelated memories. Streaming load restores shared keys and payloads, including past-life memories. The permission owner indexes accessible inventories and ecology indexes patches by area, so local discovery avoids world-wide scans.

Repeated observation updates provenance in place when the immutable payload still matches. Once all permitted containers are known, each personal fact table retains a weak permission-version token and skips container discovery until access changes. Unknown distant containers remain pending, so their arrival can still be discovered. Replacing it with a fresh personal fact table clears this derived cache; loading reconstructs it on the next inspection. Roads and patches keep their authored observation cadence, and remembered stock stays stale until deliberate inspection.

Visible roads and exposed patches share immutable scenes through `game-facts`. Each person keeps an independent evidence stamp; stock and container inspection remain personal and can become stale. Moving freezes earlier scene evidence into cold history. Explicit inspections and reports override local entries until the next observation. Registered per-patch hooks retain the ordinary request path and its rollback behavior.

Scene caches track weak versions of their world sources and are rebuilt after loading. They never expose distant patches or other people's inventories. `Initialize` supplies privileged starting facts without replacing existing personal knowledge.
