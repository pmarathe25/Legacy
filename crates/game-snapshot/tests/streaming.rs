use game_snapshot::*;
use serde::Deserialize;
use std::{
    io::{self, Cursor, Read},
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Metadata {
    version: u32,
    #[serde(default)]
    flags: Vec<String>,
}
fn uint(d: &str) -> Result<u64, String> {
    serde_json::from_str(d).map_err(|e| e.to_string())
}
fn codecs() -> [(&'static str, Decoder<u64, String>); 2] {
    [("a", uint), ("b", uint)]
}
struct Bytewise<'a>(&'a mut Cursor<Vec<u8>>);
impl Read for Bytewise<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let limit = out.len().min(1);
        self.0.read(&mut out[..limit])
    }
}
#[test]
fn strings_and_bytewise_streams_accept_field_reordering_and_escaped_keys() {
    for input in [
        r#"{"version":1,"flags":["\u0078"],"resources":{"a":4,"b":9}}"#,
        r#"{"resources":{"b":9,"\u0061":4},"flags":["x"],"version":1}"#,
    ] {
        let a: Decoded<Metadata, _> = from_str(input, codecs()).unwrap();
        let mut reader = Cursor::new(input.as_bytes().to_vec());
        let b: Decoded<Metadata, _> = from_reader(Bytewise(&mut reader), codecs()).unwrap();
        assert_eq!(
            a.metadata,
            Metadata {
                version: 1,
                flags: vec!["x".into()]
            }
        );
        assert_eq!(a.metadata, b.metadata);
        assert_eq!(a.resources, b.resources);
        assert_eq!(a.resources["a"], 4);
        assert_eq!(a.resources["b"], 9);
    }
}
#[test]
fn malformed_payloads_duplicate_fields_and_trailing_input_are_rejected() {
    for input in [
        r#"{"version":1}"#,
        r#"{"resources":{}}"#,
        r#"{"version":1,"resources":{"unknown":3}}"#,
        r#"{"version":1,"resources":{"a":1,"a":2}}"#,
        r#"{"version":1,"resources":{"a":1,"\u0061":2}}"#,
        r#"{"version":1,"version":2,"resources":{}}"#,
        r#"{"version":1,"resources":{},"resources":{}}"#,
        r#"{"version":1,"resources":{"a":-1}}"#,
        r#"{"version":1,"resources":{"a":18446744073709551616}}"#,
        r#"{"version":1,"resources":{"a":false}}"#,
        r#"{"version":1,"resources":{"a":[1,2}}"#,
        r#"{"version":1,"extra":{},"resources":{}}"#,
        r#"{"version":1,"resources":{}} {}"#,
    ] {
        assert!(
            from_str::<Metadata, _, _>(input, codecs()).is_err(),
            "accepted {input}"
        );
        assert!(
            from_reader::<Metadata, _, _>(input.as_bytes(), codecs()).is_err(),
            "accepted {input}"
        );
    }
}
#[test]
fn a_resource_decoder_runs_before_the_rest_of_the_file_is_read() {
    let input = format!(
        "{{\"resources\":{{\"a\":false}},\"version\":1,\"flags\":[\"{}\"]}}",
        "x".repeat(100_000)
    );
    let mut reader = Cursor::new(input.into_bytes());
    assert!(from_reader::<Metadata, _, _>(&mut reader, codecs()).is_err());
    assert!(
        reader.position() < 100,
        "reader buffered an entire world before decoding"
    );
}
static LIVE: AtomicUsize = AtomicUsize::new(0);
struct Tracked;
impl Drop for Tracked {
    fn drop(&mut self) {
        LIVE.fetch_sub(1, Ordering::SeqCst);
    }
}
fn tracked(d: &str) -> Result<Tracked, String> {
    let _: u64 = serde_json::from_str(d).map_err(|e| e.to_string())?;
    LIVE.fetch_add(1, Ordering::SeqCst);
    Ok(Tracked)
}
#[test]
fn decoding_and_metadata_failures_drop_all_previously_loaded_resources() {
    let codecs: [(&'static str, Decoder<Tracked, String>); 2] = [("a", tracked), ("b", tracked)];
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    for input in [
        r#"{"version":1,"resources":{"a":1,"b":false}}"#,
        r#"{"version":false,"resources":{"a":1,"b":2}}"#,
    ] {
        assert!(from_reader::<Metadata, _, _>(input.as_bytes(), codecs).is_err());
        assert_eq!(LIVE.load(Ordering::SeqCst), 0);
    }
    let loaded: Decoded<Metadata, _> =
        from_str(r#"{"version":1,"resources":{"a":1,"b":2}}"#, codecs).unwrap();
    assert_eq!(LIVE.load(Ordering::SeqCst), 2);
    drop(loaded);
    assert_eq!(LIVE.load(Ordering::SeqCst), 0);
}
#[test]
fn reader_errors_never_expose_partly_restored_values() {
    struct Failing;
    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken reader"))
        }
    }
    let error = from_reader::<Metadata, _, _>(Failing, codecs())
        .err()
        .unwrap();
    assert!(error.is_io());
}
