# Spatial

`game-spatial` owns plain `Location` components in the host's shared store. An entity occupies an area, travels or stops on a road, or follows a carrier's location. It never keeps a second position ledger.

Carrier resolution follows actual anchor rows and rejects missing anchors or cycles. Batch resolution reads borrowed rows in stable ID order. A road remains in its departure cell until arrival; carried entities use their terminal carrier's cell.

Intrinsic validation checks identifiers and geometry. Explicit topology and time facts check context. Movement choices, travel speed, physical carrying, consent, and arrival scheduling belong to other features.

The common area graph path function uses directed positive-duration roads, checked cumulative costs and stable lexical ties. Characters and Transport bind their own known graphs and interpret the resulting path.

`LocalRoster` builds once for an immutable query phase. It separates raw stationary identities, carrier-resolved area placement, coarse cells, and undirected road candidates. It holds an immutable Store borrow; drop it before any Location value change, birth, carrier reassignment or restore, then rebuild. It is a derived index rather than persistent state. Exact road-point comparison remains with the movement owner.
