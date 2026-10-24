# game-nurture

`game-nurture` plans and executes voluntary food acquisition, travel, and feeding errands for dependent children and incapacitated wards.

**Why it matters:** Infants and young children cannot forage or walk roads on their own; caregivers must perceive their hunger cues, fetch or harvest food without eating it themselves, carry it back, and serve them.

## Responsibilities

- **Dependent food errands (`Assist`):** Evaluates food needs for both assigned household wards and co-located dependent biological children (even if another parent is the primary assigned caregiver).
- **Multi-stage acquisition & delivery:** Acquires food via [`game-provisioning`](../game-provisioning/README.md) (comparing ready stores against harvest/travel time), carries it along known roads to the beneficiary's remembered location, and invokes `game_physiology::Serve`.
- **Perceived need estimation:** Uses [`game-communication`](../game-communication/README.md) infant cries, nonverbal cues, and elapsed-time anticipation rather than reading the child's private hunger integer.

## Boundaries

- **Stateless policy module:** Owns no independent `game-sim` plugin or resource; registers its request handlers inside [`game-care`](../game-care/README.md) and persists delivery intentions in [`game-households`](../game-households/README.md).

## How It Works

- **Imperfect caregiver perception:** When communication is enabled, a successful serving reduces the caregiver's perceived need estimate by expected relief, while a declined offer gives observable feedback—allowing caregivers to help without an omniscient hunger oracle.
- **Abandonment on missing ward:** If a returning caregiver arrives and finds the beneficiary is no longer at the remembered area, or if the caregiver's own urgent needs supervene, the delivery intention ends cleanly.

## Game Integration

- **Plugin / Entry Point:** Registered by [`game-care::plugin`](../game-care/README.md) (`register(&mut Builder)`).
- **Requests & Events:** Handles `Assist` and step-continuation callbacks; submits proposals via [`game-choice`](../game-choice/README.md).
- **Callers:** Invoked through [`game-care`](../game-care/README.md) and [`game-households`](../game-households/README.md).

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-choice`](../game-choice/README.md), [`game-communication`](../game-communication/README.md), [`game-nutrition`](../game-nutrition/README.md), [`game-households`](../game-households/README.md), [`game-provisioning`](../game-provisioning/README.md), [`game-characters`](../game-characters/README.md), [`game-physiology`](../game-physiology/README.md), [`game-vitality`](../game-vitality/README.md), [`game-traits`](../game-traits/README.md), [`game-relationships`](../game-relationships/README.md), [`game-cargo`](../game-cargo/README.md), [`game-objects`](../game-objects/README.md), [`game-property`](../game-property/README.md), [`game-knowledge`](../game-knowledge/README.md), [`game-transport`](../game-transport/README.md), [`game-lineage`](../game-lineage/README.md).
- **Testing:** Exercised by `cargo test -p game-care`, `cargo test -p game-households`, and `cargo test -p game-campaign`.

Food-specific evidence now survives competing pain or rest cues. Observed feeding and hunger intervals refine a personal appetite estimate. Caregivers can restart an interrupted delivery toward the last observed location, including after saving, without learning a remote child's true location. Nourishment can satisfy an ongoing need signal; acceptance weighs the recipient's appetite, preferences, and competing commitment.

Care urgency now includes anticipated consequences of personally perceived need, with reasoning and foresight controlling that anticipation. This allows a concerned caregiver to interrupt an absorbing interest before a dependent is harmed. Altruism, compassion, relationships, learned duty, personal needs, and mistaken observations still affect the choice; an indifferent caregiver can decline care.

An observer's intelligence and foresight can increase anticipated urgency for a
visibly immature dependent. This affects care valuation only after the observer has
formed a food hypothesis. The signal interpreter's existing cognitive threshold is
unchanged: a selfless but incapable individual can still fail to infer hunger, and
an indifferent but intelligent individual can still decline care. Food hypotheses,
care techniques, and learned commitments are acquired rather than genetic content.

Neighbors can offer care without kinship or an existing commitment. Candidates include personally known family, voluntary beneficiaries, and bounded local encounters. Actual need must still be inferred or communicated; low cognitive ability can prevent the food hypothesis. Several people may help the same person, and a caregiver may help several others over time.

Food-care choices report `Reconsider` when neither feeding nor physical work made progress. A declined serving emits `care.feeding-failed` with the observed rejection reason; the ordinary chooser can try alternatives and apply its existing failure learning. Pending travel/gathering and actual feeding remain successful progress.

A delayed food review rechecks that the caregiver is at an area before observing or planning local delivery. During travel or after stopping on a road it returns `Reconsider`, preserving any delivery intention, journey and supplies for later choices.

The opt-in routine policy `preserve_on_care_completion` distinguishes a person's
own action completion from external evidence. When the actor retains a routine
and has no food-delivery intention, the completion listener leaves that routine
intact; the ordinary character completion wake still runs. Existing deliveries,
heard signals, self-feeding, injuries, arrivals and other evidence keep their
ordinary interrupt path. This can defer a fresh search for someone to help until
the routine expires, and repeated actions can have different effects from a new
choice. The default is false and omitted from saves. It changes the model, not
just execution speed.

Care candidate scope, need inference, and concern valuation now live in
[`game-care-perception`](../game-care-perception/README.md). Nurture retains
planning, delivery and notification policy. `Rules::care_signal_changes_only`
optionally filters heard-signal prompts using changed food evidence. All ordinary
notices still execute; an existing delivery always keeps its prompt. This is a
separate model experiment, disabled by default, with no bounded reaction delay
for a need that was not identified as new evidence.

The whole-decision frame experiment prepares this provider across actors by
deferring its sensing updates and invalid-intention cleanup. Pure assessment and
concern remain with `game-care-perception`; delivery and execution remain here.
Cleanup checks that the stored intention still matches before removing it.
Ordinary and provider-only frame reviews retain live observation and cleanup.
