# Persistent personal knowledge

Identity-ordered committed/scratch blocks retain bounded records across ticks.
Four records fit inline; larger authored capacities spill only as actually used,
without allocating `population * max_records` slots. Scratch copies reuse capacity.
Original mind order and dead-mind record order are preserved on export.

The validated host supplies `ObservationView` reads from committed physical state;
no full-population observation adapter is needed. Encounter reports always read
prior committed partner records. `Draft` tokens are runtime/generation bound;
newborn enrollment invalidates outstanding drafts and appends empty minds in
canonical birth order. Snapshots alone reconstruct editable `Knowledge` values.

`Runtime::begin` splits an immutable committed `Read` from mutable scratch
`Preparation`. The fused host prepares social encounters and new knowledge in
parallel with autonomy reading the prior knowledge generation. Physical reads
come directly from committed blocks; export and active-life invocation queries
are the only public projections. Newborns enroll directly after publication.
