use game_core::*;
use game_facts::{archive::Archive, *};
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
