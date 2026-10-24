# game-contracts

Runtime data and serialization contracts: resources, typed requests/events, wire
messages and resource codecs. The simulation host keeps owner checks, dispatch,
transactions, schedules and commits. `game-sim` reexports the existing public API.
There is no gameplay policy or execution path here.

Dependencies: game-core, serde, serde_json, rustc-hash.

The optional `access-trace` feature records conservative resource read/write sets
between explicit diagnostic boundaries on the calling thread. It captures missing
reads and mutable codec access, including checkpoint housekeeping. It does not
capture individual rows, scheduling, identity allocation, or other threads. The
hooks are absent from ordinary builds. Use it to investigate dependencies, not to
authorize concurrent commits or measure uninstrumented throughput.

The optional `causality` feature provides generic nested attempt scopes and JSONL
telemetry. The kernel supplies request/event/listener labels and feature owners
supply diagnostic observations; contracts contain no gameplay decisions. Without
the feature all observer closures and iterators remain unevaluated. Enable through
`game-sim/review-trace`, with an unused `LEGACY_REVIEW_TRACE` output path.


`game-drafting` is reexported as `drafting`: schema-independent dependency footprints,
versions and ordered draft execution. Serializable `Job<P>`, `Action<P>`, `Actions<P>`
and command receipts live here. The action resource preserves its packed row
journals and wire representation. Dependencies additionally include game-packed
and game-drafting; no feature or second committing engine is introduced.
