# game-cargo

`game-cargo` owns inventory values for area stock and carried compartments. Each records its good, quantity, capacity, access, grant history, and spoil time. The crate prepares timed pickup and loading jobs, or contact handoffs between holders, using explicit facts about actor capability, source contact, access, permission, unreserved stock, receiver contact and access or acceptance, body room, and transport limits.

Carry planning reuses the first eligible compartment in stable ID order. If none qualifies, it validates a proposed empty compartment through the same load admission before returning it. Preparation only describes an admitted job: it does not allocate a compartment, reserve stock, transfer goods, or interrupt activity. Execution must recheck actual stock.
