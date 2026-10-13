# Spatial

`game-spatial` owns plain `Location` components in the host's shared store. An entity occupies an area, travels or stops on a road, or follows a carrier's location. It never keeps a second position ledger.

Carrier resolution follows actual anchor rows and rejects missing anchors or cycles. `resolved_anchor_with` applies the same rules to borrowed owner-prepared projections. Batch resolution reads borrowed rows in stable ID order. A road remains in its departure cell until arrival; carried entities use their terminal carrier's cell.

`prepare_location` prepares canonical placement with opening entity and Location witnesses; its guarded stager supports shared reserved entity creation and rejects conflicting final edits. Intrinsic validation checks identifiers and geometry. Explicit topology and time facts check context. Movement choices, travel speed, physical carrying, consent, and arrival scheduling belong to other features.

The common area graph path function uses directed positive-duration roads, checked cumulative costs and stable lexical ties. Characters and Transport bind their own known graphs and interpret the resulting path.

`LocalRoster` builds once for an immutable query phase. It separates raw stationary identities, carrier-resolved area placement, coarse cells, and undirected road candidates. It holds an immutable Store borrow; drop it before any Location value change, birth, carrier reassignment or restore, then rebuild. It is a derived index rather than persistent state. Exact road-point comparison remains with the movement owner.

`into_candidates` captures shared per-area and road ID lists for a declared phase that admits no Location changes. Its iterator merges at most three borrowed lists in stable ID order without copying each crowded cell per actor. Receiving owners recheck current contact and eligibility and must enforce the phase's no-change condition on the final composed batch before publication. A capture has no automatic persistent invalidation.

`prepare_audience<C>` keeps a borrowed local phase for exact raw contact or nearby area and road candidates. The caller supplies its native actor marker component. Its batch guard checks selected location and carrier chains and new or moved actor markers, while allowing changes in unrelated regions and locations belonging only to other entity kinds. Eligibility remains with the receiving owner. `capture_candidates` creates one shared candidate capture while the borrowed audience guard is retained.
