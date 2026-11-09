# Individual cultivation

People choose field work according to their own preferences and pressures. Planting consumes actual seed; crops grow while the worker is free; harvest creates dated goods only if physical storage admits them. Fields are physical state, not an institution or employer.

When `inherit_births` and family birth enrollment are enabled, a newborn may receive a recorded parent's farming inclination. No pressure is copied, children remain subject to the normal age rules, and each person can change their own inclination.

Optional `requests` describe personal requests for one planting or harvest action on a particular field. An authored asker names a person and field, an interval and a lifetime; a request is made only while both people are available at that field's regional stock endpoint. The recipient's own `help_value`, `effort`, and weights for directed affinity, trust, fear and felt obligation decide whether they accept or refuse. These weights may be zero or negative. A request imposes no obligation and promises no future work or ownership. Busy recipients can leave requests unanswered until they expire. A controlled recipient chooses whether to accept or decline; acceptance attempts the physical action.

The player view lists addressed requests; use `respond-cultivation REQUEST accept|decline` to answer one.

Requests become actionable on a later boundary. Each attempted action reserves its worker and field alongside ordinary farming and uses the same actual seed, claims and storage arbitration. Latest-boundary events distinguish refusal, expiry, a changed field, physical failure and fulfillment; only an accepted physical receipt reports fulfillment. Pending requests and personal preferences are saved with agriculture.

A requester who personally witnesses fulfillment, refusal, or failed physical work may interpret each through their own work-affinity sensitivities. Failure has its own optional signed reaction and is not a refusal; an unwitnessed outcome does not change the requester's feelings. No obligation is created by the request.

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
