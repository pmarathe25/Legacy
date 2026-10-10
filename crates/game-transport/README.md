# game-transport

This crate defines travel and carrying rules over supplied bodies, positions, and simulation times. It tracks road journeys, mid-road positions, and roadside approaches. Carry groups model people carrying dependent or incapacitated passengers: bearers make personal offers, and lifts require valid permission, willing bearers, shared position, and enough capacity for the passenger and their cargo.

Load is divided by each bearer's remaining capacity; overloaded groups cannot move. The rules also cover newborn attachment, stopping and resuming travel, and responses to offers of help. This crate handles physical transport of people, not general hauling for the economy.
