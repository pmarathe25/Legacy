# Physical production plans

Translate recipe starts, completions and output delivery into all-or-nothing physical lot transactions. Job inputs keep their original expiry; output lots are dated at actual completion and remain in job custody until delivery. The caller validates actor decisions, registers empty custody, admits these transactions, and publishes job outcomes with the enclosing world generation.

Work can account for elapsed milliseconds with an exact persisted remainder and calculate its next completion deadline. This is an owner-level primitive: the economy still advances and publishes effects at whole boundaries, and does not yet offer partial-boundary world advancement.
