# game-environment

This crate owns calendar time, seasons, and deterministic daily weather. Its saved runtime holds its own epoch, seed, calendar, climate, and current weather.

The runtime sets its next deadline and prepares elapsed-day facts before publishing new weather. Commit rejects stale or altered preparations. Other features consume its small weather observations and completed-day facts; this crate needs no crops, resources, or people.

The optional `content` feature loads calendar and climate definitions from TOML, such as `content/environment/temperate.toml`. Saves retain those definitions and their validated content fingerprint, so later edits do not change an existing world's weather.
