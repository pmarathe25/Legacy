# Physical production plans

> Historical note: references below to removed experimental hosts and their fixtures describe the previous implementation. See the [current runner](../game-economy-cli/README.md) for supported scenarios and commands.

Translate real recipe starts, completions and output delivery into all-or-nothing physical lot transactions. Job inputs keep their original expiry; output lots are dated at actual completion and remain in job custody until delivery. The caller validates actor decisions, registers empty custody, admits these transactions, and publishes job outcomes with the enclosing world generation. The fused host does not yet use this adapter.
