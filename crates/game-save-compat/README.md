# game-save-compat

Normalizes historical JSON ownership into a supplied current empty-world schema.
Current manifests borrow the original text; defaults are constructed only when an
older manifest needs migration. Existing payloads are preserved, and undeclared
feature state is rejected. The simulation still validates the restored state.

This crate owns historical wire compatibility, not live gameplay or content.
Assembly supplies the default snapshot and restores the normalized result.

Older character saves also receive an empty `characters.local-interaction-phase`
resource. This state is transaction-local and must be empty in every committed
checkpoint. No other missing current-owner resource is silently defaulted.

During migration, unchanged resources remain borrowed raw JSON; only affected owner payloads are expanded. This preserves opaque fields, escaping, and large numeric literals while reducing temporary memory. The final restored host still validates every resource.
