# game-households

Saved household records and refusal bookkeeping live in
[`game-household-state`](../game-household-state/README.md), registered here
under the existing household owner and save key.

Owns adult partnerships, kinship, revocable care commitments, and food-care intentions.
`Households.caregivers` records many caregivers per beneficiary; a caregiver can
support many people. `beneficiaries_of`, `carers_of`, and `committed` expose both
directions without scanning the population. The reverse index is rebuilt on load.

`CommitCare` records an actor's chosen commitment or withdrawal. It grants no
control over a beneficiary. `Foster` is a convenience request for offering care;
it does not require another caregiver to die. `Independent` records the beneficiary’s own self-reliance without rewriting
another person’s intentions. Actual offered help can still be declined. Death removes commitments without
transferring them to another person. Birth records kinship and location only.

Food reservations count voluntarily accepted commitments. `Tend` and periodic
reviews reconsider needs; actual assistance is chosen by `game-nurture` using
perceptions, personality, relationships, and competing concerns. An ongoing
medical procedure is separately reserved by `game-care`, rather than assigning
a household role. `CareChanged`, `Fostered`, and `Formed` expose milestones.

Campaign integration tests cover concurrent caregivers, multiple beneficiaries,
withdrawal, bereavement, voluntary help without kinship, and save/load.
