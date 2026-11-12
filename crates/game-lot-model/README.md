# Physical lots

Serializable physical custody, per-cohort expiry, and transfer values for the parallel goods owner. Recipe requirements remain quantities; physical lots retain their original expiry through movement.

Optional nonzero batch identifiers distinguish divisible cohorts and are part
of their merge key. Batch metadata is owned by the feature that creates it;
the lot model only preserves the opaque identifier. Unique item IDs and batch
IDs cannot be combined on one lot.
Movements may specify one batch or leave it unspecified for FEFO selection;
batch selection and unique-item selection are mutually exclusive.
