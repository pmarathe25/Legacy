# settlement-history

Separate runnable entry point for the preserved `game-history-kernel` prototype.
The primary engine rebuild is `game-parallel`; its history example is independent.

```
cargo run --release -p settlement-history -- --people 1000 --areas 8 --workers 4 --days 30 --output target/history/settlement-example
```

The output directory must not already exist. Receipts, final checkpoint and a
summary explicitly identify the experimental model. `--resume FILE` loads this
prototype's format. `--audit-legacy FILE` reports unsupported durable state;
`--import-legacy FILE` accepts only the narrow subset documented by its adapter.
Days are additional days; stopping is at a canonical boundary and may overshoot
the requested target. Periodic crash-safe output is not implemented here; use the
primary engine's history runner for that workflow. This is not a full Legacy
history runner, and no long-history speedup has been measured.
