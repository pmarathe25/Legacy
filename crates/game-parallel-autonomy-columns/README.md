# Persistent autonomy review columns

Immutable identity-ordered actor metadata and reusable review-clock scratch avoid
allocating whole Situation, Actor and optional-decision vectors each tick. A
validated `SituationView` reads committed physical state directly; a records
callback reads the prior committed knowledge generation. Routes are compiled once.
Only chosen decisions and their eventual movement receipts are materialized.

Original actor order, review cooldowns without movement, dispositions, route tie
breaks, rejected/unavailable outcomes, newborn activation times, and discarded
preparations retain reference semantics. Import/export is the editable boundary.
