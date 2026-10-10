# game-core

`game-core` gives the game a shared vocabulary for referring to people, places, armies, resources, and other entities. Its distinct `u64` ID types, integer simulation times and quantities, and serializable value types let simulation code exchange state consistently.

The crate defines people's locations, including travel and stopped-on-road states, plus road-position validity checks, stock knowledge, vitals, life stages, and practical skills. Skills expose mastery and practice values, with a fixed-rate type for tracking progress across differently sized time slices. This crate defines shared data; it does not run the simulation.
