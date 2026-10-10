# Agriculture

`game-agriculture` models fields, crop rules, cultivation jobs, and harvests. A field moves through fallow, prepared, planting, growing, ripe, or failed phases. Preparation and planting require labor; growth accumulates weather-adjusted degree days and tracks water stress, tending, and soil fertility. Weather arrives as elapsed-day facts from `game-environment`.

Beginning work checks explicit facts for worker capability, tool and field availability, area, seed or water stocks, and output access and room. It returns prepared field and job changes plus any seed or irrigation input consumption; completion returns field changes and harvest output. The coordinating assembly applies stock effects to inventories.
