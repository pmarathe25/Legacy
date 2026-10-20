# game-lexicon

Pure sound/meaning associations and bounded reverse lookup. Communication owns
learning, hearing and demonstrations; this crate indexes only the personal
vocabulary supplied by its caller. `game-communication` reexports `Association`.

The decoder retains at most 128 recent lexicons and 32,768 supplied forms and a 4 MiB data budget in total.
Weak record versions invalidate on vocabulary edits, rollback and world switches.
Short sounds use inline 64-bit keys, including length and zero phonemes; forms and
meaning ordinals stay contiguous. Strings are packed into shared text pools, separate from hot search keys.
Returned meanings retain a text-pool slice and survive cache eviction.
Long sound sequences retain the complete comparison path. Highest strength and
then lexical meaning break homophone ties. Oversized vocabularies use a full scan.

The shared-meaning query avoids constructing surface text for title predicates.
It does not reveal any associations absent from the supplied personal vocabulary.

Run `cargo test -p game-lexicon` for prefix/zero/long sounds, cache pressure,
multiple personal vocabularies, revisions and comparison against complete scans.
Dependencies: `game-core`, `game-storage`, `serde`. No plugin or world resource.
