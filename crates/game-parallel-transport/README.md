# Parallel transport owner

Owns route and traveler data, personal needs during transit, and destination arrival batching. The runtime validates and admits departures. Cargo remains attached to person identity; explicit shipment arbitration has a separate owner.

The shared routing table computes deterministic directed shortest paths per destination and provides both the next region and its leg duration.
