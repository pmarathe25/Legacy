# game-snapshot

Streams a JSON snapshot's keyed resources directly through supplied typed codecs.
String input borrows resource slices. Reader input retains at most one resource's
serialized bytes while its codec decodes it, plus decoded resources and the smaller
metadata envelope. It does not retain a second serialized copy of every resource.
String and reader entry points accept the same format and reject trailing data, missing resources, unknown codecs, and
duplicate metadata/resource keys. The metadata type owns its schema; callers
validate resource completeness, ownership, identities, and simulation invariants.

This infrastructure crate does not run features, advance clocks, or apply simulation rules. It
returns owned values only after the entire envelope parses successfully. Decoder
failure drops partially loaded values. Use a buffered reader for file input.
`game-sim` supplies registered resource codecs and validates the restored host.

Run `cargo test -p game-snapshot` for malformed input, field-order, streaming,
codec failure, and cleanup checks.

`evidence::View` and `evidence::Archive` encode shared fact keys and values while
preserving each person's private source, timestamp, revision, and confidence.
Restoration shares a promoted value only within the same key and original value.
It releases each original payload after all required namespace projections exist,
and retires consumed personal row buffers. The version-1 wire format is unchanged.
These helpers moved here from `game-facts` so snapshot formats have one owner.

`Manifest<J, N, R>` owns the version-1 runtime envelope metadata, with supplied job,
notice and receipt types. Codecs and the host retain resource and identity validation.
