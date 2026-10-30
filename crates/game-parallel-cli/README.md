# Parallel CLI

Experimental administrative interface to the independent parallel engine. Run `cargo run -p game-parallel-cli -- content/parallel/autonomy.toml` and enter `help` for available commands. It is not yet the ruler-bound, knowledge-limited game interface.

The workshop fixture supports `inventory REGION`, `work ID WORKER REGION RECIPE UNITS`, `cancel-work ID` and `assign-work ID WORKER`.
The `goods PERSON`, `pickup-good PERSON GOOD QUANTITY`, and `deposit-good PERSON GOOD QUANTITY` commands manage nonfood goods when goods cargo is enabled.
The `offer`, `offers`, `accept`, `decline`, and `withdraw` commands expose explicitly accepted goods exchanges. Run `help` for their arguments.
