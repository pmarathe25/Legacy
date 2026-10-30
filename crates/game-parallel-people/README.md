# Parallel people owner

Contains the existing `game-parallel` physical data and region-owned ecology,
hunger, meal arbitration and health phase. `game-parallel` reexports `Rules`,
`Person` and `Region`, so existing callers and authored worlds retain their API.
This extraction changes no physical arithmetic or scheduling. It keeps the kernel
small enough to compose additional independently owned phases without expanding
its source budget.
