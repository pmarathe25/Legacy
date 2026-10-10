# game-ecs

Stores plain, typed feature data on entities with stable game IDs. Features register component schemas and validation, then use cached two- or three-component joins. Adding or removing components requires refreshing a query; stale queries return an error. Engine types remain private.

Saves contain sorted IDs and component columns under explicit stable keys. Restore validates a separate store before it can replace live state. Clone and equality use typed values; scheduling and gameplay rules remain outside this crate.
