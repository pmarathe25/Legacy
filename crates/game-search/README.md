# game-search

Tracks finite searches for a previously encountered person. `Seek` visits areas
reachable through the searcher's remembered roads, preferring the last observed
location and then nearer unvisited places. It checks for the target only where
the searcher is present. New local observations can renew a search; no remote
position query chooses its destination.

Search trails persist across saving and loading and stop after 64 visited areas.
Food-care intentions use this owner when a beneficiary has left the remembered
meeting place. `cargo test -p game-search` covers limited knowledge and replay.

`FindGoods` is the acquisition fallback for unmet livelihood supply goals.
It visits personally remembered stock or patches along known roads, inspects
stores on arrival, and can move finite goods into a carried pack. Existing claim
reluctance and physical taking consequences apply. If no source is remembered,
it can ask a nearby character through `game-information`, recognize a new
information need, or explore known road frontiers. Neither another character's
private memory nor actual distant stock selects a destination. Goods searches
retain bounded visited-area trails and clear the deceased's state.

Acquisition returns a typed choice outcome. Failed local inspection can retain
negative stock evidence while the chooser considers another action.
