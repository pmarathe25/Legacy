# game-missions

Missions owns recruitment, directives, individual service choices, communication, observation, cargo attribution, and progress from committed physical receipts. Member decisions compose Logistics and Transport intentions; engagement, tactics, and reinforcement selection use Conflict facts. The assembly binds factual rows and executes these prepared effects.

`Mission` and `Recruitment` live on their stable entity IDs in the shared component store. `Knowledge` and `PendingTalk` live on person IDs. A bounded companion roster stays inside its mission record. `Runtime` contains only event and review cursors; the membership index holds derived IDs and reads live status and dated knowledge from the store.

Public entry points cover commands, available intentions, elapsed conversations, cancellation, local perception, review selection, deadlines, budgets, contextual validation, and receipt and outcome preparation. Sparse prepared changes guard the affected rows and reject foreign batch writes before publication. Physical transfer receipts qualify actual origin cargo; promised supplies alone cannot prove delivery. Reports retain their original observation date and provenance.

Owner tests use the native store directly. Composed game tests preserve local consent, supply conservation, force and guardian obligations, worker and storage ordering, equivalent time slices, and save/resume behavior.

`MissionExecution` supplies borrowed canonical state, body and scene facts, command entry points and transaction publication. The owner iterates receipts, elapsed talks, outcomes, observations and autonomous reviews through this port. `MemberExecution` binds physical intentions while the owner controls attempt and fallback progression. `AssetFacts` and `ObjectiveFacts` supply focused rows; the service owner selects the joins and local visibility.
