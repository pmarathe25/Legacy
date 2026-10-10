# game-cargo

`game-cargo` owns inventory values for area stock and carried compartments. Each records its good, quantity, capacity, access, grant history, and spoil time. It admits timed pickup, loading, and contact handoffs from explicit actor, contact, access, permission, acceptance, body-room, and transport facts. Planning identifies a job and eligible compartment; goods move only when that job completes and closing admission succeeds.

Carry planning reuses the first eligible compartment in stable ID order, or validates a proposed empty compartment through the same admission. At closing, timed loads and scheduled transfers use scoped arbitration over authoritative stocks. Opening spend budgets prevent same-phase re-spending of arrivals; current occupied compartment space and carrying room constrain incoming goods. Preparation guards the referenced stocks, and a successful commit applies each stock effect once.
