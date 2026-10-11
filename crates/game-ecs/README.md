# game-ecs

Stores plain, typed feature data on entities with stable game IDs. Features register component schemas and validation, then use cached joins with required or optional components. Adding or removing components requires refreshing a query; stale queries return an error. Query order is unspecified, so systems order authoritative results by stable IDs.

Saves contain sorted IDs and component columns under explicit stable keys. Restore validates a separate store before it can replace live state. Clone and equality use typed values; scheduling and gameplay rules remain outside this crate.

A typed `Batch` applies sparse insertions and removals atomically. All registrations, validators, identities, and structural counter checks pass before any row changes. Features check their own opening values; unrelated components remain in place.

The private backend wraps components for the chosen engine. Consumers use ordinary Rust data, borrowed references, and game IDs; engine handles, derives, query types, and worker pools stay behind the adapter. Query bindings are runtime caches and must be rebuilt for a cloned or restored store.
