# game-transport

Carrying groups, offer inboxes, lifts, approaches, resume targets, paused approaches, and personal support knowledge are seven optional shared-store components on existing person entities. Typed ports and `register_components` own their schema and intrinsic checks. `Runtime` owns rules and the global 64-record response journal; journal records have no entity identity.

Body, contact, consent, and road facts remain explicit inputs. `BodyFacts` supplies only requested physical facts; ordinary offer and lift admission reads the connected carrying component. Available actions retain lazy local candidate enumeration. Integer load shares, real carried mass, directional road positions, consent, deadlines, and dated support views preserve their existing mechanics.

Prepared operations capture touched rows, relevant connected group/lift membership, body facts, rules, and any response-journal input. Time-sensitive plans also capture their preparation time. `stage_into` appends typed edits to a caller's batch. Apply that batch once before consuming `TransportFinalization::finish` in the same serialized transition. Standalone `commit` performs that sequence. Failed admission publishes neither row changes nor journal responses.

`validate_runtime` checks owned relationships against one supplied factual body catalog and clock. `validate_absent` rejects native transport rows when a composition has no Transport runtime.

Departure admission, carried load, joining-bearer selection, and lift participant order are owner entrypoints. Physical facts remain lazy and requested by ID.
