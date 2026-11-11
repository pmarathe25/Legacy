# Crates

The workspace contains the simulation and its typed feature owners. [game-economy](game-economy/README.md) composes physical state, personal decisions and feature owners into staged, validated simulation boundaries. [game-economy-player](game-economy-player/README.md) exposes the player-facing view, and [game-economy-cli](game-economy-cli/README.md) provides the text interface.

Individual crate READMEs describe their responsibilities and behavioral boundaries. Content definitions live in the repository's `content/` directory.

[game-mechanics](game-mechanics/README.md) provides a standalone parallel apparatus experiment owner. It is not yet composed into the interval host or connected to physical item custody.
