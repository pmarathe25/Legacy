# game-ecs

Stores plain, typed feature data on entities with stable game IDs. Features register component schemas and validation, then use cached joins with required or optional components. Adding or removing components requires refreshing a query; stale queries return an error. Query order is unspecified, so systems order authoritative results by stable IDs.

`Store::iter<T>` enumerates matching archetypes and sorts borrowed rows by stable ID. It visits matching rows rather than probing every entity; it allocates a temporary list of those rows. Cached joins avoid this per-call sorting when callers can merge results in their own stable order.

Saves contain sorted IDs and component columns under explicit stable keys. Restore validates a separate store before it can replace live state. Clone and equality use typed values; scheduling and gameplay rules remain outside this crate.

A typed `Batch` applies sparse insertions and removals atomically. All registrations, validators, and identities pass before any row changes. `Batch::projected<T>` borrows the final typed value without publishing and rejects duplicate edits.

`ReadSet` retains selected typed opening values, absence, entity presence, and predicate-scoped membership. It clones only selected rows; membership enumerates its component column and tracks IDs rather than member values. Predicate inputs must be retained immutably by the owner. Changes outside the recorded dependencies remain valid.

Declare an owner's write keys in a `ReadFootprint`, stage those writes, then immediately call `ReadSet::bind(store, batch, &writes)`. Binding rejects unadmitted writes to protected rows and captures exact declared edits and projected membership. After every owner has staged, validate all returned `ReadGuard`s against the final batch and current store before one application. Entity dependencies guard canonical and projected presence; multiple owners may attach their own components to one explicitly created entity. Exclusive component footprints remain owner policy. Owner configuration, runtime witnesses, and domain validation remain private to the owner.

For an owner-exclusive newly allocated record, call `ReadSet::exclusive_entity(store, id)` before staging. Binding requires canonical absence and a nonempty set of declared component insertions; the guard rejects extra keys, duplicate edits, and removals before or after binding. Owners may declare a compound shape explicitly when several components belong on that identity. Ordinary `entity` witnesses retain flexible shared creation.

Batch indexes retain submission order and include duplicate edits. Typed row projection, edit counts, and footprint checks use indexed lookups. Scoped membership checks enumerate their component column plus that type's staged edits. `cargo bench -p game-ecs --bench transaction_guards` measures 1,000, 2,000, and 4,000 selected rows and writes, including staging, binding, final validation, and batch publication.

Query caches retain an opaque generation token. Structural changes replace that token; value updates preserve it. Tokens are derived, excluded from saves, and have no numeric counter to exhaust. Cached queries retain old tokens until refreshed, so allocation reuse cannot make a stale query appear current.

The private backend wraps components for the chosen engine. Consumers use ordinary Rust data, borrowed references, and game IDs; engine handles, derives, query types, and worker pools stay behind the adapter. Query bindings are runtime caches and must be rebuilt for a cloned or restored store.

Run `cargo bench -p game-ecs --bench sparse_iteration` to compare sparse iteration with all-ID probing as unrelated populations increase. That baseline uses public lookups, so its timings include lookup overhead and do not measure the exact previous backend. It measures iterator scaling, not complete-game performance.
