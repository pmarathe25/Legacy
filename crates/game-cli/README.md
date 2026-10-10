# game-cli

`game-cli` provides a text interface for playing and inspecting a simulation. The executable creates an opening from command-line options, then reads commands interactively, from standard input, or from a script. Players can inspect status, submit gameplay commands, advance simulated time, and save or load a session.

The library parses text into shared `game-api` commands and sends them to `game-world`; it also formats observations and results as JSON. Hosted time advances use `game-session`, while the adapter can execute directly against a composed world. The CLI handles input and display; simulation rules remain in the backend.
