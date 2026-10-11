# game-openings

Defines starting settlements and playable openings as validated TOML: names, local links, stock lots, resource deposits, workshop bindings, and starting property attitudes. Population scaling and symbolic references are resolved before startup.

This crate provides authored plans. Feature owners create and operate the actual entities. Saves retain the resolved definitions and their content stamps, so resuming does not require the original files.
