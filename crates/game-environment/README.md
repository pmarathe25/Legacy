# game-environment

This crate maps simulation time to calendar days and seasons, and generates deterministic daily weather from the world seed, day, and seasonal climate rules. Its world state holds the calendar, climate, and current weather.

When time crosses a day boundary, the crate returns facts for the elapsed day, including its day of year and weather, then publishes the next day's weather. Independently owned systems consume these facts; this crate owns no crops or resources.
