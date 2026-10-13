# game-openings

Defines starting settlements and playable openings as validated TOML: names, local links, stock lots, resource deposits, workshop bindings, starting property attitudes, and named starting customs. Population scaling and symbolic references are resolved before startup.

This crate provides authored plans. Feature owners create and operate the actual entities. Saves retain the resolved definitions and their content stamps, so resuming does not require the original files.

A starting custom describes source access, a custody fund, recognized claim rights and founders, charter procedures and office powers, local access rules, delivery contributions, and optional initial fulfillment timing. Openings select a custom by key from their settlement. Symbolic resident and asset references resolve to native Property, Society and Agreements inputs; the assembly constructs those starting records through their owners.
