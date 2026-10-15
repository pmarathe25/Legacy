# Numeric decision experiments

This tool compares the game's exact utility selection with packed records, tiled numeric columns, AVX2, and optional GPU compute. It is a measurement tool; the simulation keeps one authoritative state and its owner-checked commit path.

```sh
cargo run --release -p game-compute-experiment --locked --offline
LEGACY_COMPUTE_POPULATIONS=1000,10000,20000 cargo run --release -p game-compute-experiment --features gpu --locked --offline
cargo test -p game-compute-experiment --all-features --locked --offline
```

The fixture supplies 4, 16, or 64 candidates per person. It compares every output with `game_characters::choose`, including signed saturation, stable ID tie-breaking, urgency, current-choice hysteresis, absent choices, and irregular tail sizes. These are numerical fixtures, not a complete economy or GPU implementation of knowledge discovery, routing, arbitration, or action execution. The [production workloads](../performance/README.md) measure those implemented features together.

**CPU layouts:** The packed path uses the production 56-byte `Candidate` record. Columns keep seven arrays of IDs, five signed numeric terms, and survival flags: 49 bytes per candidate. The person-major layout vectorizes candidate arithmetic before selection. The choice-major layout vectorizes scoring and arbitration across people, with 128- and 4,096-person tiles and bounded stack buffers. Workers share one bounded Rayon pool. The portable path permits compiler vectorization; the AVX2 path uses runtime detection and compiles the same arithmetic for that instruction set. Release disassembly must contain vector instructions before treating it as SIMD.

**GPU:** The optional `gpu` feature uses wgpu 27 with Vulkan and native 64-bit integer support. It selects a hardware adapter and prints its identity and limits; software adapters do not count as GPU measurements. Unsupported devices retain the CPU experiment. This dependency does not enter the game CLI or production feature crates. Device buffer limits use available hardware capacity, capped at 512 MiB per binding. Choice-major input coalesces adjacent GPU threads' reads; only buffer offsets use 32-bit arithmetic, while values and IDs retain all 64 bits.

Device creation, pipeline setup, and reusable buffer allocation are reported separately. Each full evaluation packs directly into upload staging memory, dispatches, waits, maps readback, constructs output, and checks exact equality. `upload_enqueue_ms` measures staging allocation; `packing_ms` includes filling and closing that upload view. `submit_readback_ms` includes deferred transfer and driver work. `resident_dispatch_ms` amortizes 32 identical dispatches with one readback: it is a favorable residency experiment without intervening simulation commits. It cannot be substituted for full round-trip cost.

The GPU receives recorded numeric inputs only. Provenance, text, unknown schemas, and sparse histories remain CPU data. A future accelerator integration would require independent decision phases with revision-tagged inputs, bounded buffers, CPU fallback, stable conflict arbitration, and measured end-to-end benefit.

The current numeric observation representation and packed decision inputs provide a path to those batches. They preserve stale knowledge and keep additional authored crops, traits, and objects as data. The existing [ECS comparison](../performance/README.md#storage-choice) also measures Bevy archetypes and Legion against persistent dense leaves; this experiment studies temporary packed views rather than adding another world implementation.

## Measured layouts

Five-run medians on the i7-8550U / Intel UHD 620 laptop, Rust 1.98.1, release with thin LTO. All outputs matched the production integer selection rule.

For 20,000 people with 16 candidates each:

| Path | Evaluation ms |
| --- | ---: |
| Packed records, one CPU worker | 5.41 |
| Choice-major AVX2 columns, four workers | 1.54 |
| GPU round trip from numeric columns | 12.61 |
| GPU resident dispatch, amortized over 32 dispatches | 2.13 |

Building the choice-major columns from packed records cost another 15.93 ms. This projection cost is excluded from the evaluation column and applies before both columnar CPU and GPU evaluation. The GPU path packs directly into upload staging memory, avoiding an additional full host input buffer. Reported device buffers totaled about 18.0 MiB for this case; driver/staging allocations are additional.

At 10,000 people / 16 candidates, the same one-worker choice-major loop took 2.54 ms with portable code and 1.01 ms with runtime AVX2. Release disassembly contains `vpaddq`, `vpsubq`, and `vpcmpgtq` inside both vector kernels. More workers and wider vectors do not improve every case; cache behavior, tile size, and memory bandwidth matter.

At 100,000 people / 64 candidates, packed CPU evaluation took 69.9 ms, four-worker choice-major AVX2 took 25.1 ms, and the GPU round trip took 501.8 ms. Even the favorable resident GPU experiment took 44.5 ms. These results concern exact 64-bit utility arithmetic on this integrated GPU; they do not rule out other kernels or hardware.

**Decision:** Keep production candidates numeric and compact. Retain columnar CPU/GPU experiments for work that can reuse an already numeric view. Repacking every small decision phase or moving this workload to the GPU would add cost. No graphics or GPU dependency enters the simulation or CLI.

### Component-level archetypes

A temporary Bevy ECS 0.20.0 adapter stored candidate IDs, actor ordinals, five numeric terms, and survival flags in separate table components. It bulk-created entities in reverse logical order and reused a cached serial query. Outputs still matched stable IDs exactly. In a paired five-run comparison at 20,000 people / 16 candidates, packed records took 3.69 ms, Bevy components 4.49 ms, and one-worker person-major arithmetic columns 3.53 ms. These paired timings come from a separate comparison executable and should not be mixed with the GPU table.

That case has one populated archetype and temporary candidate entities. It does not rank native ECS schedulers or a complete ECS game. The existing storage comparison also covers structural changes, sparse records, and snapshots. Neither comparison justifies adding a second authoritative world. The temporary archetype adapter and dependency are absent from the workspace.

The CPU runtime-dispatch approach follows the [Rust architecture documentation](https://doc.rust-lang.org/core/arch/); GPU capability checks use wgpu's [native integer feature](https://docs.rs/wgpu/27.0.1/wgpu/struct.Features.html#associatedconstant.SHADER_INT64).
