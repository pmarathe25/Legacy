# game-institutions

Founding and explicit procedures, bounded access-rule proposals, voting with executable permission consequences, offices, and removal. Institutional power is separate from the ruler lineage.

**Dependencies:** [game-core](../game-core/README.md), [game-sim](../game-sim/README.md), [game-property](../game-property/README.md).

**Interface:** [Source](src/lib.rs) defines the public types and operations. Hosted features register their state and request handlers through `plugin`. The public player API is bound by `game-assembly`; internal construction requests are privileged.

**Verification:** Run `cargo test -p game-institutions`. Tests belong to the crate and compose required owners without the application, except for client and application integration tests.

`Personal` leaves commitments with their individual custodians. `Decide` handles a recognized individual’s decision; `Vote` is reserved for actual voting procedures. `Arrange` records a change after the affected people respond, without treating compliance as approval. `RecordPractice` records permissions already enacted by the relevant holders. The optional upper-owned `CanCommit` request lets generated communities refuse practical implementation when a custodian no longer follows the decision maker. Proposals cannot commit people outside the membership.

Rule adoption and enactment have separate records. A procedure can adopt a proposal while its custodian refuses to enact it. `Proposal::enacted` records actual permission changes; `RuleAdopted` reports the outcome. A recognized decision never creates an invisible barrier preventing a physical act.
