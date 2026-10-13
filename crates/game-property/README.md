# game-property

The owner stores physical inventory custody, claims, incidents, stock provenance, personal attitudes, dated knowledge, pending actions, and request deadlines as registered components in the shared Store. `Runtime` retains only its receipt cursor. Property provenance tags partition actual cargo quantities; they never grant spendable stock or access.

Owner methods accept the Store and ordinary typed actor, contact, and stock facts. `PreparedProperty` compares its opening rows and relevant report-participant membership before staging sparse writes. Standalone `commit` admits one batch before publishing its cursor and effects. For a cross-owner transaction, `stage_into` appends property rows to the host batch; finish its returned `PropertyFinalization` only after the combined batch succeeds in the same serialized window.

`validate_runtime` checks owned references and dated records. `Runtime::validate_with_quantities` additionally checks native provenance row count and partition totals against quantities supplied by the cargo owner. Registration checks intrinsic values without reading another owner's state. Tests compose independently registered stores, including a foreign column that can reject combined admission.

InventoryCustody records physical control independently of claims. Startup binds authored custody declarations to native inventory entities; prepared custody changes guard the opening row. validate_custody rejects unknown inventory and person references.

`prepare_material_transfer` composes Cargo's closing admission with conserved claim provenance. `prepare_material_receipt` reacts to an already admitted Cargo transfer before publication; callers reserve its exact event sequence and reconcile earlier receipts first. `PreparedProperty::stage_material_into` executes admitted taking and permission effects through Cargo. Validate returned Cargo guards and `PropertyFinalization::validate_batch` after every owner has staged, then publish one batch and finish the cursor. Water reactions can join the same batch through Health's receiving interface.

`observer_facts` owns the interpretation of local custody, personal taking permission, and autonomous defense eligibility from factual contact and player-control inputs. Property relationship effects are typed receiving effects for Characters and can be staged before the same canonical publication.

`prepare_loss_receipt` consumes the Cargo owner's actual loss from current provenance. A pending receipt prefix retains earlier production, consumption, and birth changes in the same prepared publication; the reserved loss event advances the cursor once. Rejected combined admission preserves both the original portions and receipt cursor.
