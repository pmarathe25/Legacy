# game-preservation

Edible goods occupy FIFO batches with expiration times. The `shelf-days:N` object tag selects shelf life; untagged food defaults to thirty days. Transfers retain original expiration, including moves into caches. Workshop transformations consume old inputs and create newly made outputs with the output's shelf life.

Cargo publishes typed stock changes after physical accounting. This owner tracks batches and schedules expiration by shared date buckets. Spoilage consumes real cargo; failed transfers restore quantities and ages together. Save validation checks tracked food against every edible inventory quantity. `Enable` starts tracking existing food.
