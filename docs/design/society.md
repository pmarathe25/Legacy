# Society, power, and government

**Scope:** Emergent government, sharing, property, theft, coercion, invented institutions, learned values, and bounded reasoning from precedents are required. Detailed procedures remain to be developed with the supporting systems; see [D7](decisions.md#d7-social-and-institutional-depth).

## The central rule

A government is a collection of people, offices, resources, agreements, and institutions that other people support, tolerate, or resist. Do not select a global `GovernmentType` that determines everyone's behavior.

A society may become communal because people contribute goods and respect shared access, or become coercive because powerful people take goods and compel labor. These are outcomes of actions and relationships. A label in the UI describes those outcomes; it does not cause them.

Ownership, sharing, and enforcement must work before a formal government exists. Formal institutions make practices more consistent but still depend on people carrying them out.

## Bounded social invention

People invent arrangements within the facts, experiences, and actions represented by the simulation. The full social model includes these three capabilities:

| Capability | Required behavior | Boundary |
| --- | --- | --- |
| Invented institutions and rules | Recognize problems, construct new combinations of conditions, rights, obligations, roles, and procedures, propose them, and respond to adoption, refusal, and consequences. | Individual arrangements need not be authored, but their operations must have executable meanings in the simulation. |
| Learned values | Acquire attachments to new practices, groups, and ideals grounded in modeled experience; let those values change goals, decisions, and social responses. | Naming an ideal does not create a physiological need or an unmodeled mental capability. |
| Reasoning from precedents | Compare structured cases, weigh conflicting represented principles, generalize to unfamiliar combinations, and revise judgments from experience. | Use available knowledge and uncertain estimates. Unrestricted human-like interpretation of arbitrary natural-language principles is outside scope. |

Definitions are structured data with stable identities and explicit meanings. Display names and explanations describe them; generated prose does not create behavior. Rules can define new social permissions and duties, but fulfilling or enforcing them still requires actual people, actions, and resources. They cannot add physical abilities or bypass conservation and consent rules.

Distinguish representing an idea, proposing it autonomously, and evaluating its consequences. A rule interpreter alone does not implement invention. Start with bounded candidate construction and evaluation once needs, knowledge, agreements, and disputes exist. Choose the concrete search and case-comparison algorithms through implementation and behavioral tests; the kernel does not need a general programming language or language model.

A useful test follows a person recognizing a recurring problem, proposing an arrangement absent from the authored institution list, others adopting or rejecting it, behavior changing, and consequences affecting subsequent choices. Also test a learned value changing a later decision and a relevant precedent changing judgment of a new case. Merely generating a new name or selecting a complete predefined institution does not satisfy these tests.

Share immutable concept/rule definitions, index relevant facts and events, and run expensive proposal search only when due. Keep each person's beliefs, value strengths, and known precedents separate. Bound search by deterministic simulation rules rather than available wall-clock time. Benchmark proposal bursts and accumulated concepts as well as everyday rule evaluation. Performance at 10,000 people remains a target to measure.

Technological invention is deferred until production exists, when a concrete domain such as farming or metallurgy can be evaluated. Ordinary recipes, learning, production, and equipment remain in scope. Social procedures may combine existing observations and evidence; discovering new physical sensing or verification techniques belongs to that later technology decision.

## Beliefs, knowledge, and social response

Each character has personal attitudes toward exclusive ownership, sharing, obligation, authority, violence, and reciprocity. These are distinct from personality: a disciplined person can consistently support either communal or private property.

Characters also track directed relationships: trust, affection, fear, respect, grievances, and remembered favors. Trust in one person is distinct from support for an institution. A character's assessment is based on what they witnessed or learned, rather than omniscient access to the event log.

Proposed behavior:

1. A character observes an action or receives a report.
2. They interpret it using their beliefs, interests, relationships, and confidence in the report.
3. They update their assessment of the actors and relevant institutions.
4. Their changed assessment affects later cooperation, reporting, retaliation, and political support.

For example, taking grain may be condemned as theft, defended as reclaiming shared food, or accepted as tribute. Physical events are objective; social interpretation is observer-dependent. Do not assign a universal criminal or bad-person flag.

Beliefs change more slowly than immediate trust. Repeated outcomes and social influence can change them, but a single failed harvest must not instantly convert an entire settlement to a different social order. Exact learning rates need tuning after the behavioral model is agreed.

## Ownership claims and access

Use the [economy model](economy.md) to distinguish possession, claims, and access. An ownership claim records a claimant, property, asserted rights, supporting evidence, and recognition by people or institutions. Conflicting claims can coexist.

A claim does not create an invisible barrier around goods. Physical access depends on custody, containers, locks, presence, guards, and resistance. An unauthorized taking can succeed physically even when a court would condemn it.

Sharing is an owner's or custodian's action or standing permission. Supported arrangements include gifts, household access, communal membership, ration entitlements, rental, lending, and permission for a specific use. Do not make every settlement resident automatically entitled to every government stockpile.

Customary claims can precede written records. Communal property can have a group claimant and membership rules. Use rights can belong to someone other than the claimant of the underlying land or tool.

## Negotiation and agreements

Characters and organizations negotiate terms such as those defined in [the economy design](economy.md#agreements). Those terms support the initial economic loop and extend through the bounded invention model above. Offers need executable consequences rather than labels or descriptive text alone.

An offer can be accepted, rejected, or countered. Parties assess feasibility, expected benefit, trust, alternatives, risk, and enforcement. A needy person may accept a poor bargain; a threatened person may comply without considering the bargain legitimate.

Promising a transfer does not perform it. Default, forgiveness, renegotiation, and enforcement are ordinary simulation outcomes. Reputation and personal relationships can support agreements before courts exist.

## Institutions and offices

An institution records:

- its purpose, jurisdiction, members, and funding;
- membership and exit rules;
- recognized offices and the powers attached to them;
- how proposals become decisions;
- how officeholders are appointed, removed, and replaced;
- its policies, enforcement procedures, and active disputes;
- the people who currently recognize or oppose its authority.

Initial decision procedures can be leader decision, unanimity, majority vote, or delegated decision within a mandate. These are useful early examples for councils, courts, guilds, households, armies, and governments. Do not treat them as an exhaustive enum of permissible future institutions.

An officeholder can only commit resources or make agreements within their actual authority. Their signature does not bind a competing faction that rejects that authority. Distinguish a valid institutional procedure from whether its result is obeyed.

An institution needs a creation path: people propose a purpose and rules, negotiate participation and contributions, and begin operating when the founding terms are met. Changes, splinter groups, dissolution, and replacement use the same political process. Do not require the player to create every institution.

## Laws, disputes, and enforcement

A law needs a jurisdiction, subjects, conditions, obligations or prohibitions, deciding authority, and response procedure. Initial examples cover property, access, contributions, labor obligations, assault, and agreement disputes. Represent invented laws through the bounded model above and test their actual effects through the same actions and social responses as other rules. Settle detailed adoption, amendment, and adjudication procedures when implementing those behaviors.

The enforcement chain is explicit:

```text
event → observation/report → complaint → investigation
      → decision → enforcement request → action → social response
```

An unreported theft does not automatically summon guards. A judgment does not automatically move goods. Investigators, witnesses, custodians, and enforcers have their own motives and may fail, refuse, lie, or favor someone.

Possible resolutions include return of goods, compensation, a new agreement, restricted access, loss of office, exclusion, confinement, or force. Their implementation depends on available people, places, and resources. Punishment severity and the supported confinement model remain design decisions.

## Coercion and organized force

Intimidation compares perceived ability to harm, willingness to act, available allies, protection, and the target's alternatives. A threat can lead to compliance, refusal, flight, appeal for help, or violence. It must not reduce to the physically strongest nearby person automatically owning everything.

Use the same injury and conflict rules for theft resistance, guards enforcing a judgment, rebellion, and war. A request to seize property becomes a task for an actual person or unit. A ruler cannot confiscate distant goods by editing the ownership ledger.

Repeated successful coercion can establish tribute and labor obligations, while also increasing fear, grievances, flight, and resistance. Collective defense and legitimacy can counter personal strength.

## The ruler and political authority

**Confirmed:** People can reject or depose the reincarnating ruler. The player continues as that person and may seek to rebuild authority.

Keep these records separate:

- the enduring reincarnation lineage;
- the current person's identity and abilities;
- offices held by that person;
- personal assets and claims;
- institutions and their property;
- recognition, loyalty, and relationships.

Recognition by the guardians does not automatically restore a throne, transfer an estate, or compel obedience. A child incarnation does not inherit adult political agency merely because the player controls them.

The realm interface exposes information and actions available through relationships and offices. Requests, priorities, appointments, laws, taxation, and spending enter the world through those channels. When authority is lost, the relevant powers disappear or become contested; direct control of the character remains.

## Delegation and continuity

Mayors and governors receive mandates with a jurisdiction, budget or spending rights, goals, limits, reporting duties, and removal procedure. They interpret instructions according to competence, beliefs, loyalty, and interests.

Institutions continue acting while the ruler travels, dies, or grows up. Succession and regency depend on their own rules and political struggles. The guardians' duty to the reincarnating person is distinct from serving any particular government.

## Acceptance scenarios

- Identical food stocks produce different sharing outcomes when beliefs and relationships differ.
- A communal store restricts an outsider while allowing members access under its rules.
- Theft can physically succeed without making every character aware of it.
- An owner can refuse a request, and a hungry requester can seek another source, bargain, steal, or use force.
- A judgment remains unenforced if no willing and capable agent carries it out.
- An unpopular ruler loses practical authority without ending player control or erasing institutions.
- A ruler's death does not transfer government assets to the next infant incarnation.
