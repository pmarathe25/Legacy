# game-preservation

Tracks food quantities by expiry date and removes spoiled stock through
`game-cargo`. Transfers retain remaining shelf life. Storage conditions rescale
that remaining life without making old food fresh.

`Preservation` owns the batch ledger and expiry calendar. Each live date has one
scheduled callback shared by its inventory/good targets. Consuming or moving the
last matching batch removes that target; an empty date cancels its callback.
Temperature refreshes replace the previous appointments. Expiration processes at
most 128 targets per transaction and schedules the remainder at the same time.

Earlier saves rebuild the calendar from their actual batches on the first food
operation. The owner cancels its previous expiry callbacks and schedules only
those still needed. Rebuilding preserves quantities and expiry times. Callback
allocation changes can affect later identifiers, so historical studies label the
migration as a model change.

Public requests are `Enable`, `Refresh`, and the optional `Conditions` provider;
`Spoiled` reports removed stock. Food definitions come from `game-objects`.
`game-food-storage` supplies storage and temperature effects, while
`game-maturation` owns fermentation and curing. Preservation changes inventories
only through cargo requests.

`cargo test -p game-preservation` covers consumption, shared dates, transfers,
changing conditions, failed transactions, legacy calendars, and save/load across
worker counts. Composed maturation and campaign tests cover physical storage
and aging during transport.

Expiry appointments share each date's target set independently, so changing one
date does not copy neighboring appointment sets. Removed sets retain ordinary
rollback and deterministic bounded expiry processing.
