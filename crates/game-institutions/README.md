# game-institutions

Founding, governance procedures (`Personal`, `Decide`, `Vote`, `Arrange`), offices, and rule adoption vs. physical enactment.

**Why it matters:** Institutional rules are social arrangements, not laws of physics. Separating rule adoption (a vote or decree) from physical enactment (whether a custodian actually grants access) lets authority, compliance, and defiance emerge naturally.

## Responsibilities

- **Institution & Office Registry:** Tracks founded institutions, membership rolls, recognized offices, and governance procedures independently of the player's dynastic lineage.
- **Governance Procedures:** Supports individual custody (`Personal`), recognized leader decisions (`Decide`), ballot procedures (`Vote`), negotiated arrangements (`Arrange`), and recorded ongoing practices (`RecordPractice`).
- **Adoption vs. Enactment Separation:** Distinguishes adopting a rule proposal (`RuleAdopted`) from practically enacting its permission changes in `game-property` (`Proposal::enacted`).
- **Custodian Commitment Checks (`CanCommit`):** Queries upper social owners (`game-communities`) to verify whether a custodian still follows a decision-maker before committing their property permissions.

## Boundaries

- **No Invisible Forcefields:** An institutional decision or office title never creates a magical barrier preventing a physical act—it only updates recognized `game-property` permissions when enacted.
- **No Non-Member Compulsion:** Proposals cannot bind people outside the institution's membership.

## How It Works

- **Executable Permission Consequences:** When a proposal passes via `Decide` or `Vote`, `game-institutions` checks `CanCommit` for each affected custodian. Compliant custodians update `game-property` access rules (`Proposal::enacted`), while defiant custodians leave the rule adopted on paper only.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers institution records, office tables, and procedure/proposal handlers in `game-assembly`.
- **Requests & Events:** Exposes founding, office assignment, `Decide`, `Vote`, `Arrange`, `RecordPractice`, and `CanCommit` hooks; emits `RuleAdopted` and membership/procedure events.
- **Callers:** Used by `game-communities`, `game-affiliation`, `game-realm`, `game-naming`, and `game-justice`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-property`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-institutions`) verify procedure transitions, ballot tallying, office succession, custodian refusal via `CanCommit`, and save/load consistency.
