# Agriculture

`game-agriculture` owns crop phases, soil, cultivation labor, and harvest output. `Field`, `Crop`, and `Soil` are separate components keyed by site. Crop definitions live in `content/agriculture/crops.toml`; a crop refers to a stable definition ID.

The daily growth system joins crops with their soil and area-local weather facts. Missing components or weather skip growth. Cultivation work advances at its own completion deadlines; tending, irrigation, fallow, and harvest work without climate. Starting preparation and planting needs temperature or calendar facts.

Systems return prepared component changes and material effects. They never write another feature's inventories. The optional `content` feature reads TOML at startup; saved worlds retain their definitions and content stamp.

Interval preparation retains only active labor rows and matched daily growth rows. A supplied executor can evaluate independent growth inputs in parallel. Commit checks the rows and local positions it read before publishing; an idle interval prepares no component writes and never runs daily growth.
