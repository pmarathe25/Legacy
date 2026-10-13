# game-lineage

`game-lineage` tracks the current incarnation, succession waiting state, and guardians’ targets and goals. After a death, it binds the lineage to an actual, living newborn created later with zero initial age and two parents; simultaneous eligible births resolve by person ID. Guardians sense the newborn’s location, while reaching them still requires physical travel.

The canonical `Runtime` retains mentorship rules and the stable ID of one `History` component. `History` stores ordered incarnations; its final record determines the current incarnation and waiting predecessor. Each guardian's `Guardian` component resides on their person entity. Invocation learning remains a sparse person component. `register_components` registers all three columns. Startup calls `Runtime::initialize` with the existing campaign founder entity ID, history, and founding guardians for atomic admission. The History component remains on that founder through death and succession, preserving the gameplay allocation stream.

Owner entry points prepare succession, witnessed assessment, request judgment, review priorities, care provisions, supply receipts, and invocation learning. A `PreparedLineage` exposes its typed output and stages rows into a shared batch. Its `StagedLineage` validates runtime metadata, opening rows, scoped guardian membership, and exact final writes through `game-ecs` guards. Call `finish` only after admission to obtain the prepared effects. External factual row witnesses can join the plan through `with_factual_reads`; callbacks do not choose lineage policy or update its columns.

Save the runtime and shared store together. `Runtime::validate` checks intrinsic chronology and guardian state; `validate_references` accepts person, stock, area, site and recorded-death facts through `LineageReferences` and owns the cross-feature succession and delivery checks.

Guardians build readiness from witnessed conduct and judge it according to their own traits. They can refuse requests or prioritize observed care needs. Care and accepted supply requests declare generic resource-delivery goals; the logistics planner composes their physical steps, and actual receipts track fulfillment. The world adapter applies the resulting actions.

Guardian rescue reads borrowed personal child signals and remembered stocks, checks current physical support through Transport, and returns a typed care, comfort, rest, or carrying intention. It preserves signal freshness and stock-ID selection without reading a coordinator or copying a character mind.

Guardian review also owns the priority of confinement, recovery, combat, accepted objectives and tasks, and assistance. Its factual input contains physical conditions, work and position; the adapter executes the resulting intention.
