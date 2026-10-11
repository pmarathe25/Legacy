# game-ecs

Stores plain, typed feature data on entities with stable game IDs. Features register component schemas and validation, then use cached joins with required or optional components. Adding or removing components requires refreshing a query; stale queries return an error. Query order is unspecified, so systems order authoritative results by stable IDs.

`Store::iter<T>` enumerates matching archetypes and sorts borrowed rows by stable ID. It visits matching rows rather than probing every entity; it allocates a temporary list of those rows. Cached joins avoid this per-call sorting when callers can merge results in their own stable order.

Saves contain sorted IDs and component columns under explicit stable keys. Restore validates a separate store before it can replace live state. Clone and equality use typed values; scheduling and gameplay rules remain outside this crate.

A typed `Batch` applies sparse insertions and removals atomically. All registrations, validators, and identities pass before any row changes. Features check their own opening values; unrelated components remain in place.

Query caches retain an opaque generation token. Structural changes replace that token; value updates preserve it. Tokens are derived, excluded from saves, and have no numeric counter to exhaust. Cached queries retain old tokens until refreshed, so allocation reuse cannot make a stale query appear current.

The private backend wraps components for the chosen engine. Consumers use ordinary Rust data, borrowed references, and game IDs; engine handles, derives, query types, and worker pools stay behind the adapter. Query bindings are runtime caches and must be rebuilt for a cloned or restored store.

Run `cargo bench -p game-ecs --bench sparse_iteration` to compare sparse iteration with all-ID probing as unrelated populations increase. That baseline uses public lookups, so its timings include lookup overhead and do not measure the exact previous backend. It measures iterator scaling, not complete-game performance.
