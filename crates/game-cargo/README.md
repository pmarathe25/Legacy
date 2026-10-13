# game-cargo

`game-cargo` owns inventory values for area stock and carried compartments. Each records its good, quantity, capacity, access, grant history, and spoil time. It admits timed pickup, loading, and contact handoffs from explicit actor, contact, access, permission, acceptance, body-room, and transport facts. Planning identifies a job and eligible compartment; goods move only when that job completes and closing admission succeeds.

Carry planning reuses the first eligible compartment in stable ID order, or validates a proposed empty compartment through the same admission. At closing, timed loads and scheduled transfers use scoped arbitration over authoritative stocks. Opening spend budgets prevent same-phase re-spending of arrivals; current occupied compartment space and carrying room constrain incoming goods. Preparation guards the referenced stocks, and a successful commit applies each stock effect once.

The `cargo/inventory` schema and typed storage ports admit inventory rows into the shared store and preserve other components on the same entity. Intrinsic validation checks capacity and nonzero references; location, actor membership, and current permissions come from their owners. The shared store is the only stock authority. Storage validation also checks each row's inventory ID against its entity ID. Transfers preflight every intermediate stock state against the registered schema before the first debit, then allow ordered receipts between guarded effects; a commit without receipt interleaving publishes final balances atomically.

Material deposits check the compartment's actual good and available room before increasing its stock. Their prepared writes can join other owners' effects in one atomic batch. Tool availability after finishing uses the remaining selected users, preserving existing users when the finishing actor releases its selection.

Tool allocations also admit prospective consumption: a producer cannot consume the last toolkit it needs or displace another worker's allocated unit. Producers choose their own preferred tool order through that interface.

Physical toolkit projection reads selected uses through small body and placement ports. Cargo excludes unavailable bodies, non-tools and misplaced uses, then allocates in stable order. It queries selected equipment and position lazily and preserves existing use independently of access permission.

Prepared compartment creation and splits validate the new identity, good, quantity, capacity, and opening source row. They stage with other owners' effects. A split preserves the source compartment's grant history. `ValidatedTransfers::stage_next_into` stages each ordered physical transfer so a receiving owner's record changes can join the same batch before its receipt is published.

Guarded transfer, consumption, deposit, and access staging returns the shared ECS `ReadGuard`. Validate it against the completed batch before publication to reject changed openings, duplicate writes, and late foreign removals. Indexed carried-row totals and bulk carried totals avoid repeated whole-column queries. Cargo also owns actual delivery-compartment projection, toolkit eligibility, equipment demand/allocation joins, and bounded public quantity choices.

`portion_available` admits body and care portions against the requested good, current balance, contact, effective access, and shared unreserved budget. `prepare_food_consumption` enforces Food; `prepare_portion_consumption` accepts the receiving owner's required good. Successful prepared consumption records the actual recipient as `last_granted` and returns a guard for the combined publication. `effective_access` and `receiver_authorization` join independent owner grants without altering inventory permissions.

`prepare_forced_split` constructs and conserves a forced compartment from a captured native source. The coercion owner supplies its placement and access; Cargo does not require receiving consent or a body-capacity grant. Applied material has capacity equal to its quantity and starts without grant or spoil scheduling metadata. Its guarded staging protects the source and reserves the new entity's exact Inventory-only shape.

`prepare_spoilage` captures the actual lost quantity and clears stock plus its spoil deadline. It preserves location, good, capacity, access, and grant history. Staged spoilage returns the common read guard so Property provenance and Health water loss can publish in the same batch. Empty stock still produces a zero-quantity loss, preserving deadline-event behavior.
