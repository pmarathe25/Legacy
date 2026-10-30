# Agreements

Explicit, mutually accepted promises for future delivery of physically carried goods. Proposals and acceptance are distinct. Delivery plans become fulfilled only when the host has successfully transferred the goods; expired active promises default. The owner is data-defined and uses stable identities and generation-local events.

`content/parallel/agreements.toml` demonstrates manual proposals, recipient decisions, and partial delivery. Set `[[agreements.policies]] person = ...` and `honor = true` to let the fused host attempt a debtor's accepted agreements. `content/parallel/agreements_auto.toml` demonstrates this policy. Candidates use the goods actually carried at the start of a tick; each available person can participate in at most one selected exchange. Deadlines are indexed, and unfinished accepted agreements default after the due tick.

`minimum_honor_trust` optionally makes the debtor assess their own directed trust in each creditor before ordinary automatic fulfillment. An observed change in trust can therefore lead them to withhold one promise while fulfilling another. An optional `fear_honor_threshold` lets their personally experienced fear motivate fulfillment even if ordinary honoring is disabled; neither fear nor compliance grants approval. These choices require the opinions owner. The person can later withhold delivery as their fear subsides, and neither the promise nor the policy physically forces delivery.

`content/parallel/agreement_pressure.toml` demonstrates a person delivering a carried good because of their directed fear while ordinary honoring is disabled.

An eligible recipient may opt in to automatically accepting proposals by setting `accept = true` in their policy. The proposal must still be current, both people must be available together, and fulfillment uses a later committed tick. `content/parallel/agreements_negotiation.toml` shows acceptance followed by delivery.
