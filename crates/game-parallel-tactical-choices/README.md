# Autonomous tactical choices

Individuals who choose to pursue an opponent propose bounded movement from the committed map. Contested destinations are resolved by deterministic per-round priority; an occupied starting position cannot be taken, so accepted movements are disjoint. Choices never move the controlled player.

When direct sight is blocked, each fighter independently searches a visibility graph built from corners of up to 16 nearby obstacles. All obstacles still block movement and sight. This bounded search can miss a route in a complex map, but does not permit movement through a wall.
