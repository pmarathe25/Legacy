# Parallel autonomous travel owner

Opted-in living local people periodically review their own committed food observations. When hungry enough, they compare their remembered local stock with remembered stock at directly connected destinations, subject to configured freshness, food and improvement thresholds. They choose the highest remembered stock, with shorter routes and destination ID breaking ties. The policy does not read actual remote stock, so stale information can lead to an unproductive journey. Outgoing routes are currently treated as known locally. Missing or stale information suppresses departure; reviews still enter a configured cooldown.

Decisions are prepared from the pre-tick snapshot and launched after the physical/family tick. A person who died or is no longer at the source receives an unavailable receipt. If the departure batch is rejected, all available requests receive rejected receipts; the already completed tick is retained. When enabled, the cargo owner's departure-provision policy applies to autonomous moves. Save files retain actor opt-in, next review, and the latest boundary's receipts; a host must stream events for history.

This is one optional behavior, not a complete NPC decision system. It does not model personal route discovery, property permission, travel cost, occupation, family duties, risk, or player control. Actor and route validation, snapshot joins, and the physical departure commit remain relevant to scaling. Use the full-tick benchmark rather than treating parallel proposal generation as proof of whole-game speedup.

Actors can specify `travel_caution` (additional observed food gain required before moving) and `hunger_urgency` (how much earlier hunger triggers a search). Both default to zero for existing content; decisions still use the individual's own possibly stale records.

Generated worlds with this owner enabled create individual review policies for non-player founders, staggered over the configured review interval. They still require their own committed local and destination reports before deciding to move.
