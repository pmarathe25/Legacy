# Parallel production output

Computes atomic output admission for completed jobs. Block-local totals admit all jobs when every stock has capacity; contested stocks use deterministic chunk-prefix claims and deny a whole job if any output cannot fit. Results are staged and do not mutate inventories.
