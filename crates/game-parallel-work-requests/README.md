# Work requests

Individuals can address requests for physical work to others. Recipients decide independently using their own preferences and relationships; sending a request does not bind anyone. Starting a job uses the production owner's ordinary material and worker reservations. Batches are validated atomically and use bounded recipient inboxes.

Each person's optional signed `affinity_weight` controls how their own affinity toward a requester affects their decision to start requested work. The composed economy also reevaluates it when deciding whether to continue. Positive, negative, and zero weights are valid; fear, felt obligation, personal helping value, and effort remain separate parts of the person's evaluation.

The controlled person can use `respond-work REQUEST_ID PERSON start|refuse` to start a requested job or refuse it. Starting still checks location, available resources, age, and other ordinary work restrictions; a failed start leaves the request available.

Optional `memory_limit` (0 disables it; maximum 32) lets an individual remember witnessed outcomes of work they requested. A started job is tracked until physical completion, interruption, or the worker's death; the requester must be alive and in the same region to observe it. A witnessed refusal is recorded as an unsuccessful approach, not a breach. A person attends to at most one such outcome per tick, and evicts their oldest memory when their own limit is full. Histories and in-progress links survive save/load. An automatic asker may prefer a locally present person with more observed successes than failures or try another person after an unsuccessful approach. Bounded local exploration can fail to find a candidate even if a more distant candidate exists. All decisions still belong to the people involved; a remembered helper can refuse later.

The experimental generated profile sets `work_memory_limit = 4`; this is a general individual memory setting, not an institution, employment relationship, or contract. These simple memories are only a first step toward learning practices from experience.
