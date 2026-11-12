# Individual cultivation

People choose field work according to their own preferences and pressures. Planting consumes actual seed; crops grow while the worker is free; harvest creates dated goods only if physical storage admits them. Fields are physical state, not an institution or employer.

Set `track_seed_batches = true` in a cultivation scenario to retain the exact
seed cohorts actually consumed when a field is planted. These records are
prepared from accepted physical receipts, persist while a crop grows or a
harvest fails, and are removed on successful harvest. They do not yet cause
plant inheritance or put batch identities on harvests. Without tracking,
planting retains the compact consumption path. See
`content/scenarios/economy-core-seed-provenance.toml`.
That scenario also uses `[[cultivation.seed_batches]]` entries with a region,
named seed good, nonzero batch ID and quantity. Loading these entries assigns
tags to existing regional stock; their combined quantities cannot exceed that
stock. Unassigned seed remains untagged.

An optional `[[cultivation.seed_profiles]]` entry gives one person a signed
value for each named batch and a personal value for idleness. For an ordinary
autonomous planting, that person chooses an enabled batch above their idle
value or declines. The preference is a personal disposition, not a guarantee
that stock exists: physical arbitration can reject an insufficient or absent
batch, without substituting another. Ties between batches use their IDs for a
stable result. Controlled and requested cultivation still use their existing
response paths. These authored preferences do not yet learn from harvest
experience or compare seed choice with the general cross-domain choice round;
see `content/scenarios/economy-core-seed-choice.toml`.

An optional `[cultivation.seed_genetics]` assigns diploid growth-trait alleles
to batch IDs and defines a signed effect and bounds for physical growth
duration. The field's duration uses a quantity-weighted expression of the
seed cohorts actually consumed; an unrecognized batch contributes neutral
expression. This does not give a farmer access to genetic values. It is a
physical growth slice only: offspring, mutation, environmental genetic
selection and genetic harvests are still missing. See
`content/scenarios/economy-core-seed-genetics.toml`.

Optional `[cultivation.soil]` content specifies `capacity`, `loss_per_harvest`, `recovery_per_fallow`, and sorted `fields = [{ field, fertility }, ...]`. A configured field's harvest yields its crop's ordinary output multiplied by fertility over capacity, rounded down with a minimum of one unit. A successful physical harvest depletes fertility; a field that starts a boundary fallow recovers up to capacity. Failed harvests cause no depletion. Fields without a soil record keep their ordinary yield. This is a basic fertility model, not a complete treatment of soils or nutrients.

An optional `[cultivation.soil.amendment]` supplies a material good ID, `quantity`, `fertility_gain`, and individual `choices` naming a person and field. A willing, available person at the stock endpoint may spend a real quantity to improve that field; they can choose to disregard claims. Concurrent people targeting the same field are selected deterministically, and those targeting shared material compete through ordinary physical transactions. Only a successful receipt adds fertility; latest-boundary events record accepted and failed attempts.

An individual's amendment choice may instead name `pressured_by` and their own `fear_at_least` or `obligation_at_least`. Their directed feelings determine whether that pressure motivates the action; no general obligation or consent is imposed.

When `inherit_births` and family birth enrollment are enabled, a newborn may receive a recorded parent's farming inclination. No pressure is copied, children remain subject to the normal age rules, and each person can change their own inclination.

Optional `requests` describe personal requests for one planting or harvest action on a particular field. An authored asker names a person and field, an interval and a lifetime; a request is made only while both people are available at that field's regional stock endpoint. The recipient's own `help_value`, `effort`, and weights for directed affinity, trust, fear and felt obligation decide whether they accept or refuse. These weights may be zero or negative. A request imposes no obligation and promises no future work or ownership. Busy recipients can leave requests unanswered until they expire. A controlled recipient chooses whether to accept or decline; acceptance attempts the physical action.

The player view lists addressed requests; use `respond-cultivation REQUEST accept|decline` to answer one. A requester can use `withdraw-cultivation REQUEST` to tell the addressed person locally that the request is withdrawn. Withdrawal removes the request and its scoped signal; it does not physically prevent the other person from cultivating independently.

An autonomous requester may have a `[[cultivation.requests.withdrawal_preferences]]` entry with `person`, `affinity_below`, and a positive `retry_after` in boundaries. When their own affinity toward the addressed person falls below the threshold, they may locally communicate withdrawal. The saved retry delay prevents immediate repeated requests; it does not bind the recipient. Automatic withdrawal currently requires both people at the same regional stock endpoint.

Requests become actionable on a later boundary. Each attempted action reserves its worker and field alongside ordinary farming and uses the same actual seed, claims and storage arbitration. Latest-boundary events distinguish refusal, expiry, a changed field, physical failure and fulfillment; only an accepted physical receipt reports fulfillment. Pending requests and personal preferences are saved with agriculture.

An optional `[[cultivation.requests.offer_values]]` entry lets a recipient assign a signed `value_per_unit` and `max_quantity` to a particular `good` in a still-outstanding promise addressed to them by the requester. A proposed promise can influence their choice even if its maker has no goods or does not intend to deliver. The offer does not itself transfer anything or compel either person; delivered amounts, expired and withdrawn promises no longer count.

A requester who personally witnesses fulfillment, refusal, or failed physical work may interpret each through their own work-affinity sensitivities. Failure has its own optional signed reaction and is not a refusal; an unwitnessed outcome does not change the requester's feelings. No obligation is created by the request.

When a requester has a personal work-offering inclination, they can also remember their latest personally witnessed fulfillment and refusal of cultivation requests. On a later boundary that inclination may lead them to offer the observed worker a future delivery. The two bounded memories survive saving; a failed physical attempt or an unseen outcome does not count as fulfillment. The offer and any later delivery remain separate personal choices.

For example, scenario content can give person 1 an inclination to ask person 2 to cultivate field 7:

```toml
[cultivation.requests]
next_id = 1

[[cultivation.requests.askers]]
person = 1
worker = 2
field = 7
interval = 5
lifetime = 3

[[cultivation.requests.preferences]]
person = 2
help_value = 0
effort = 1
affinity_weight = 1
fear_weight = 1
```

Unspecified relationship weights default to zero. The selected action follows the field's current phase when the request is made; it becomes stale if that phase changes before work begins.

Instead of naming a fixed worker and field, an autonomous person may have a `[[cultivation.requests.discoverers]]` entry with `person`, `interval` and `lifetime`. At the regional stock endpoint they select a locally visible field and another reachable person from bounded regional indexes. They can still be refused, and selection does not imply knowledge that the required seed or capacity will remain available. A person may have either a fixed asker or a discoverer inclination, not both.

The optional `profile` feature records separate worker selection, field selection, matching, action construction, growth, assembly, and publication spans through `game-diagnostics`. These measurements do not affect simulation state.
