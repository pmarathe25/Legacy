# Compact family receipts

Immutable fixed-size lifecycle event rows retain exact legacy kind/people ordering.
`Frame::canonical` publishes a prepared receipt generation; cloning a frame keeps
old receipts independent of later drafts. `Frame::import` preserves initial authored
order. Expand to heap-bearing `FamilyEvent` values only for snapshots. Lineage and
newborn enrollment can read typed rows without expansion.

This owner does not mutate lifecycle state or decide transaction publication.
