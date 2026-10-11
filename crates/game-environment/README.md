# game-environment

This crate owns calendar time, seasons, and deterministic daily weather. Its saved runtime holds its own epoch, seed, calendar, climate, and regional weather forcing. Local cell data lives in the host's shared component store.

`LocalClimate` and `Weather` are independent components on area entities. The daily system joins them and applies an authored temperature offset and rain and evaporation scales to the seeded daily forcing. It sets its next deadline and prepares each cell's elapsed-day weather before publishing the next day. Commit rejects stale or altered preparations. Other features read local observations and completed-day facts; this crate needs no crops, resources, or people.

Weather updates run at daily boundaries. Shorter intervals update only the runtime epoch, without scanning or copying weather rows. Prepared daily changes guard and update individual cells; query bindings remain outside saved state and rebuild after clone or restore. Registering a cell preflights both components before creating or changing its entity.

The optional `content` feature loads calendar and climate definitions from TOML, such as `content/environment/temperate.toml`. Saves retain those definitions and their validated content fingerprint, so later edits do not change an existing world's weather.
