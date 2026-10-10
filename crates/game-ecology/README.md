# game-ecology

This crate tracks natural deposits and undeveloped plots, including their access, custody, and observed views. Daily renewal and soil recovery match each deposit or plot to its local weather cell. Weather is supplied as plain facts by any provider; this crate has no environment dependency. Missing cells preserve stock and soil, while finite gathering continues without weather. Crops and weather simulation are outside this crate.

Gathering preparation checks coordinator-supplied actor facts, access, carrying capacity, worker slots, and existing reservations, then returns a timed plan. Collection preparation checks the completed job and produces an effect for resource stock and collected goods, including spring-water contamination. The coordinator supplies actual actors, cargo, and tools, then applies goods and health effects.
