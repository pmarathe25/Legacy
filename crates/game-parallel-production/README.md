# Parallel production

This owner loads goods, recipes, inventories and worker jobs from data. Starting a batch reserves all inputs atomically; each active worker is unavailable to farming, social encounters and travel. Work advances from the tick-start snapshot. Inputs remain held when a worker cannot continue, and an eligible replacement may resume the job. Cancellation restores inputs only if all fit.

Finished goods remain in the job until every output fits in the destination inventory. Capacity claims are ordered by job ID using parallel segmented prefixes. To keep arbitration parallel and atomic for recipes with multiple outputs, prefixes conservatively count earlier rejected claims; this can leave space unused for a tick and can starve a later job while an earlier blocked job persists. Food is a separate owner and is not a production good. JSON snapshots retain all reservations and completed outputs.

`content/parallel/workshop.toml` is an authored example. The experimental CLI supports `inventory REGION`, `work ID WORKER REGION RECIPE UNITS`, `cancel-work ID` and `assign-work ID WORKER`.
