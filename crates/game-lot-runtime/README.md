# Dated production runtime

A standalone physical production owner executes dated input reservation, person-directed work, interruption, output delivery, expiry, and save/load. It is an integration step toward the fused game host, not yet the authoritative owner of the whole simulation. No social agreement obliges a worker to continue.

`prepare_with_growth` accepts harvest actions from a trusted environmental owner. It checks that output is freshly dated and enters regional stock, then arbitrates those actions with all other custody changes. Ordinary external transfers cannot create goods; callers must derive harvest quantities and timing from validated environmental state.

Construction can stage empty `Site` custody with the physical action that fills it. The runtime retires rejected holders within the same draft, and subsequent recovery uses normal lot movements, preserving original expiry and atomic rollback.

`prepare_partitioned_flat_transfers` handles eligible simple actions from different owners in one batch. It assigns the same ordered identities as normal composition before resolving shared endpoints, then returns the original action and movement identifiers in receipts. Active production work and unsupported shapes fall back to the normal runtime.

Movement events own their receipt batch and expose borrowed views through `iter` and `par_iter`; callers can use `into_owned` when receipts must be retained individually. The batch remains usable after the runtime commits or is dropped. Saves contain the committed world rather than transient receipt storage.
