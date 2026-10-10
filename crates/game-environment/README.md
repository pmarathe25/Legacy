# game-environment

This crate owns calendar time, seasons, and deterministic daily weather. Its saved runtime holds its own epoch, seed, calendar, climate, and current weather.

The runtime owns local weather cells keyed by area. Each cell applies an authored temperature offset and rain and evaporation scales to the seeded daily forcing. It sets its next deadline and prepares each cell's elapsed-day weather before publishing the next day. Commit rejects stale or altered preparations. Other features read local observations and completed-day facts; this crate needs no crops, resources, or people.

Weather updates run at daily boundaries. Shorter intervals update only the runtime epoch, without scanning or copying the cell table. Prepared daily changes guard and update individual cells; full-runtime snapshots are an explicit inspector/test operation.

The optional `content` feature loads calendar and climate definitions from TOML, such as `content/environment/temperate.toml`. Saves retain those definitions and their validated content fingerprint, so later edits do not change an existing world's weather.
