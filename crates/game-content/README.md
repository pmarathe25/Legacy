`game-content` loads authored game definitions from TOML, so content can be edited without rebuilding the game. Use `read` to load a file or `parse` to parse TOML text into a typed `Document<T>`; both check its ID and version. The receiving feature validates whether the definitions make sense for its mechanics.

A document can produce a `Stamp` containing its ID, version, and fingerprint. The loader does not save data: game saves store the stamp alongside the full definitions, preserving the content used for that game.
