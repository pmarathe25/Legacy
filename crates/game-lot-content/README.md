# Authored physical lot content

Converts authored food shelf life into checked absolute deadlines for genuinely new goods, and creates initial per-good stock custody for a fresh world. Never use fresh-world initialization to migrate existing stock: doing so would reset its age. The fused runtime does not yet use these lots.

Animal yield is a separate trusted owner input containing only action, animal, hunter, good, and quantity identities. The runtime constructs the fresh dated lot itself and validates unique prey, hunters, and actions; the animal owner must verify the living local prey and publish its death only after physical acceptance. Ordinary external transactions still cannot create goods.
