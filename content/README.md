# Content

The current simulation reads authored goods and recipes from `objects/` and `workshop/`. Its scenario TOML files live in `scenarios/`; the [runner documentation](../crates/game-economy-cli/README.md) identifies the supported starts and commands. That directory also contains reference scenarios for earlier experimental hosts; those are not necessarily accepted by the current runner. The [content importer](../crates/game-content-import/README.md) documents which object and recipe properties are currently simulated.

Other content directories are retained as source material for future features; their presence does not mean the current engine loads or implements them.
