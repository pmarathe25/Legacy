use game_core::*;
use game_facts::*;
use game_snapshot::evidence::{Archive, View as SharedView};
use std::sync::Arc;
#[test]
fn archive_shares_payloads_without_merging_personal_sources_or_stale_evidence() {
    let mut a = Facts::default();
    for i in 0..200 {
        a.insert(
            format!("stock:{i}").into(),
            Fact {
                value: Arc::new(format!("supplies {i}")),
                source: PersonId(1),
                observed: SimTime(12),
                revision: 2,
                confidence: 90,
            },
        );
    }
    let mut b = a.clone();
    b.insert(
        "stock:0".into(),
        Fact {
            value: Arc::new("new".into()),
            source: PersonId(2),
            observed: SimTime(17),
            revision: 3,
            confidence: 50,
        },
    );
    let wire = serde_json::to_vec(&Archive::capture(
        [(PersonId(1), &a), (PersonId(2), &b)].into_iter(),
    ))
    .unwrap();
    assert_eq!(
        wire,
        serde_json::to_vec(&SharedView::new(
            [(PersonId(1), &a), (PersonId(2), &b)].into_iter()
        ))
        .unwrap()
    );
    let archive: Archive<String> = serde_json::from_slice(&wire).unwrap();
    let rows = archive.restore(|_, v| v).unwrap();
    assert_eq!(rows, vec![(PersonId(1), a), (PersonId(2), b)]);
    assert!(Arc::ptr_eq(
        rows[0].1.get("stock:1").unwrap().value,
        rows[1].1.get("stock:1").unwrap().value
    ));
    assert_ne!(
        rows[0].1.get("stock:0").unwrap(),
        rows[1].1.get("stock:0").unwrap()
    );
}
#[test]
fn archive_rejects_unknown_formats_invalid_indices_and_duplicate_evidence() {
    for wire in [
        r#"{"version":2,"keys":[],"values":[],"people":{}}"#,
        r#"{"version":1,"keys":[],"values":[],"people":{"1":[[0,0,1,0,0,90]]}}"#,
        r#"{"version":1,"keys":["k"],"values":["v"],"people":{"1":[[0,0,1,0,0,90],[0,0,2,0,0,90]]}}"#,
    ] {
        assert!(
            serde_json::from_str::<Archive<String>>(wire)
                .unwrap()
                .restore(|_, v| v)
                .is_err()
        );
    }
}
use std::sync::atomic::{AtomicUsize, Ordering};
static RAW_LIVE: AtomicUsize = AtomicUsize::new(0);
#[derive(Debug, PartialEq, Eq)]
struct Payload {
    value: u64,
    raw: bool,
}
impl Clone for Payload {
    fn clone(&self) -> Self {
        if self.raw {
            RAW_LIVE.fetch_add(1, Ordering::SeqCst);
        }
        Self {
            value: self.value,
            raw: self.raw,
        }
    }
}
impl Drop for Payload {
    fn drop(&mut self) {
        if self.raw {
            RAW_LIVE.fetch_sub(1, Ordering::SeqCst);
        }
    }
}
impl<'de> serde::Deserialize<'de> for Payload {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let value = <u64 as serde::Deserialize>::deserialize(d)?;
        RAW_LIVE.fetch_add(1, Ordering::SeqCst);
        Ok(Self { value, raw: true })
    }
}
#[test]
fn restoring_evidence_releases_raw_values_after_their_last_namespace_promotion() {
    let archive: Archive<Payload> = serde_json::from_str(r#"{"version":1,"keys":["a","b","c"],"values":[1,2,999],"people":{"1":[[0,0,1,0,0,90],[1,0,1,0,0,90],[2,1,1,0,0,90]],"2":[[0,0,2,0,1,40]]}}"#).unwrap();
    assert_eq!(RAW_LIVE.load(Ordering::SeqCst), 3);
    let mut calls = vec![];
    let restored = archive
        .restore(|key, raw| {
            assert!(raw.raw);
            calls.push((key.to_owned(), RAW_LIVE.load(Ordering::SeqCst)));
            Arc::new(Payload {
                value: raw.value * 10 + u64::from(key.as_bytes()[0] - b'a'),
                raw: false,
            })
        })
        .unwrap();
    assert_eq!(RAW_LIVE.load(Ordering::SeqCst), 0);
    assert_eq!(calls, [("a".into(), 2), ("b".into(), 2), ("c".into(), 1)]);
    let first = &restored[0].1;
    let second = &restored[1].1;
    assert_eq!(first.get("a").unwrap().value.value, 10);
    assert_eq!(first.get("b").unwrap().value.value, 11);
    assert_eq!(first.get("c").unwrap().value.value, 22);
    assert!(Arc::ptr_eq(
        first.get("a").unwrap().value,
        second.get("a").unwrap().value
    ));
    assert_eq!(second.get("a").unwrap().source, PersonId(2));
    assert_eq!(second.get("a").unwrap().confidence, 40);
}
