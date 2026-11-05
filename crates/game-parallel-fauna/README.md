# Individual wildlife

This owner stages individual animals with data-defined species, diploid inherited traits, hunger, health, aging and gestation. Animals graze finite, local patches supplied by the physical resource owner; deterministic parallel allocation prevents competing animals from consuming the same food. Births use actual local parents and deterministic IDs. The host must validate the committed population and atomically publish both the returned animal generation and patch consumption.

The economic host runs wildlife after successfully staged human gathering and patch renewal, then deducts actual grazing from the same remaining patches before the tick commits. `content/parallel/economy-wildlife.toml` supplies a playable example using named food content. Human feeding, handling, capture and climate effects are not yet implemented here.
