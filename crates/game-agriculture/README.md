# Agriculture

`game-agriculture` owns crop phases, soil, cultivation labor, and harvest output. `Field`, `Crop`, and `Soil` are plain components keyed by site in the host's shared store. `Runtime` keeps definitions, jobs, and the consumed day; it never owns a second component store. Cached query bindings remain outside saved state. Crop definitions live in `content/agriculture/crops.toml`; a crop refers to a stable definition ID.

The daily growth system joins crops with their soil and area-local weather facts. Missing components or weather skip growth. Cultivation work advances at its own completion deadlines; tending, irrigation, fallow, and harvest work without climate. Starting preparation and planting needs temperature or calendar facts.

Systems return prepared component changes and material effects. They never write another feature's inventories. The optional `content` feature reads TOML at startup; saved runtime metadata retains definitions and its content stamp; the host saves components through the registered schema.

Interval preparation retains only active labor rows and matched daily growth rows. A supplied executor can evaluate independent growth inputs in parallel. Commit checks the rows and local positions it read before publishing; an idle interval prepares no component writes and never runs daily growth.

The host registers agricultural components, binds queries to its store, and refreshes those bindings after structural changes. Other features can attach their own components to the same entities; agriculture writes only its prepared crop and soil rows.

Soil is shared data owned by `game-land`. Agriculture registers field and crop components, joins them with the existing soil, and applies crop-specific growth, irrigation, and harvest effects. Converting land to a field retains its soil on the same entity.
