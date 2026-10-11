# game-property

The owner stores claims, incidents, stock provenance, personal attitudes, dated knowledge, pending actions, and request deadlines as registered components in the shared Store. `Runtime` retains only its receipt cursor. Property provenance tags partition actual cargo quantities; they never grant spendable stock or access.

Owner methods accept the Store and ordinary typed actor, contact, and stock facts. `PreparedProperty` compares its opening rows and relevant report-participant membership before staging sparse writes. Standalone `commit` admits one batch before publishing its cursor and effects. For a cross-owner transaction, `stage_into` appends property rows to the host batch; finish its returned `PropertyFinalization` only after the combined batch succeeds in the same serialized window.

`validate_runtime` checks owned references and dated records. `Runtime::validate_with_quantities` additionally checks native provenance row count and partition totals against quantities supplied by the cargo owner. Registration checks intrinsic values without reading another owner's state. Tests compose independently registered stores, including a foreign column that can reject combined admission.
