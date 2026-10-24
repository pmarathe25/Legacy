# Focused-attention PGO training

`focused.json` contains the three independent training runs used for the focused
attention compiler profile: seeds 17, 29 and 43. Paths resolve relative to the
plan. The warm 32-person profile is shared with the history tools; the large
profile freezes the 1,000-founder, 32-area configuration from the measured study.
It includes the complete world definition because content overlays replace a
whole document. Benchmark seeds 19, 37 and 23 are excluded from training.

From the repository root, with the Rust toolchain and matching LLVM tools installed:

```sh
RUSTUP_TOOLCHAIN=1.97.1 CARGO_NET_OFFLINE=true \
LEGACY_MEMORY_INDEX=0 LEGACY_APPRAISAL_CACHE=0 \
python3 tools/performance/pgo.py \
  --training tools/performance/training/focused.json \
  --output target/history/focused-pgo --build-jobs 4 --discard-build-cache
```

The output directory must be new. The executable is
`target/history/focused-pgo/game-history`. Training compiles and runs the actual
simulation; it can take several minutes. For build settings and measured results,
see [the candidate throughput report](../CANDIDATE_THROUGHPUT.md).
