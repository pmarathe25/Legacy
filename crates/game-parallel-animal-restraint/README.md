# Individual animal restraint

Owns physical holds and independent capture and release preferences. A hold links a living animal to a physically present person; it is not an ownership record or a promise of obedience. Animals can yield, resist, or be overpowered, and an actual escape attempt can weaken and break a hold. An individual can choose to release a local animal, including one held by someone else, or act from fear or felt obligation. Simultaneous releases contend per animal and take precedence over escape attempts. A handler can independently choose to physically resist one release attempt per boundary, including under pressure; the releaser can overcome that effort. The host must reconcile holds against committed person and animal locations and publish the resulting owner with the rest of the world.

Policy and physical-hold generations are shared immutably between boundaries. Use `policies_mut`, `release_policies_mut`, `defense_policies_mut`, and `holds_mut` to detach a generation before editing it; this preserves prior drafts and snapshots. Serialized content remains ordinary lists.

Run `cargo test -p game-parallel-animal-restraint steady_holds_benchmark -- --ignored --nocapture` with `LEGACY_ANIMALS` and `LEGACY_WORKERS` to measure preparation and reconciliation of unchanged holds. This isolated measurement does not include the rest of the simulation.
