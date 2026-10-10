# game-session

`game-session` gives one authoritative simulation a single host thread and a bounded queue of control requests. The host handles observations, command submissions, pause, save, load, and debug queries, then advances the simulation between requests in bounded intervals. A run returns a handle whose result reports when and why advancement stopped.

The crate depends on `game-api` and accepts any `SessionEngine` implementation. This port keeps session hosting independent of game-world types while serializing access to the engine and its saved state.
