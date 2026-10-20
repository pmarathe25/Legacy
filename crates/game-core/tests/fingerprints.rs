use game_core::*;
use std::fmt::Write;
fn legacy(text: &str) -> u64 {
    text.bytes().fold(14695981039346656037, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}
#[test]
fn streamed_fingerprints_keep_persisted_identities_for_unicode_and_formatted_fields() {
    assert_eq!(fingerprint(""), legacy(""));
    for i in 0..512 {
        let text = format!("é:{i}:🙂:{}:{}", "name".repeat(i % 37), i64::MIN);
        assert_eq!(fingerprint(&text), legacy(&text));
        let mut hash = Fingerprint::default();
        for ch in text.chars() {
            hash.write_char(ch).unwrap();
        }
        assert_eq!(hash.finish(), legacy(&text));
    }
}
