# game-ecology

This crate tracks natural deposits and undeveloped plots, including their access, custody, and observed views. Each calendar day, it uses explicit day facts to renew deposits according to weather, growth, and decay, and to recover plot soil. Weather is supplied by the environment; crops, economics, and weather systems are outside this crate.

Gathering preparation checks coordinator-supplied actor facts, access, carrying capacity, worker slots, and existing reservations, then returns a timed plan. Collection preparation checks the completed job and produces an effect for resource stock and collected goods, including spring-water contamination. The coordinator supplies actual actors, cargo, and tools, then applies goods and health effects.
