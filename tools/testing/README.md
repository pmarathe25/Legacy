# Bounded workspace tests

Run `python3 tools/testing/run.py` from the repository root. It builds all Rust
unit and integration tests, executes every listed test, then runs all doctests.
The default is one Cargo build job and one harness with one test thread, including
doctests. Cargo's
build concurrency is separate from harness concurrency: multiple large links
can exhaust memory before tests begin. Set `--build-jobs`, `--jobs` and
`--test-threads` explicitly to raise concurrency. Use repeated `-p` options
for focused development; omit them for the whole workspace.

Cargo supplies each executable and its package working directory. The runner
checks executed counts against the complete libtest inventory, rejects filtered
or incomplete results, and never uses cached timings to omit tests. Ignored tests
retain Cargo's usual behavior and appear separately in the report. Empty
harnesses are listed and skipped; they contain no tests. Failed builds, harnesses,
and doctests make the command fail.

Logs and JSON timing reports go under `target/test-runs/`. The next run schedules
previously slow harnesses first. Compilation time, harness time, and doctest time
are reported separately. Run `python3 -m unittest discover -s tools/testing/tests`
to check inventory and failure accounting. `cargo test --workspace` remains
supported.

On a shared Linux desktop, a systemd user scope can also bound the entire run,
including compiler/linker threads and child processes:

```sh
systemd-run --user --scope -p CPUQuota=100% -p MemoryHigh=1536M -p MemoryMax=2G -p MemorySwapMax=256M nice -n 15 python3 tools/testing/run.py
```

This permits one CPU's worth of work and caps memory at 2 GiB. A run exceeding the
memory limit fails within its scope. Reports still include every test; resource
limits do not reduce the test inventory.
