use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Serialize, Deserialize)]
struct Seed;
impl Request for Seed {
    const KEY: &'static str = "validation.seed";
    type Response = ();
}
impl Event for Seed {
    const KEY: &'static str = "validation.notice";
}
#[derive(Clone, Serialize, Deserialize)]
struct Done;
impl Request for Done {
    const KEY: &'static str = "validation.done";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    b.plugin("validation", 1).unwrap();
    b.request::<Done>(|_, _| Ok(())).unwrap();
    b.request::<Seed>(|tx, _| {
        tx.schedule(SimTime(10), 0, Done)?;
        tx.schedule(SimTime(20), 0, Done)?;
        for _ in 0..3 {
            tx.emit(Seed, vec![])?;
        }
        Ok(())
    })
    .unwrap();
    b
}
fn saved() -> Value {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(Seed).unwrap();
    h.advance(SimTime(5), 1).unwrap();
    serde_json::from_str(&h.save().unwrap()).unwrap()
}
fn reject(value: &Value) {
    let text = value.to_string();
    assert!(
        builder().load(&text, 1).is_err(),
        "invalid string save was accepted"
    );
    assert!(
        builder().load_from(text.as_bytes(), 4).is_err(),
        "invalid reader save was accepted"
    );
}
#[test]
fn zero_schedule_identity_is_rejected_by_both_loaders() {
    let mut value = saved();
    value["jobs"][0]["id"] = json!(0);
    reject(&value);
}
#[test]
fn journal_sequences_must_be_positive_unique_and_increasing() {
    let good = saved();
    for sequence in [0, 1, 3] {
        let mut value = good.clone();
        value["notices"][1]["sequence"] = json!(sequence);
        reject(&value);
    }
    let mut reversed = good.clone();
    reversed["notices"].as_array_mut().unwrap().reverse();
    reject(&reversed);
}
#[test]
fn invalid_clock_counters_manifests_and_duplicate_work_are_rejected() {
    let good = saved();
    for (field, value) in [
        ("next_id", json!(0)),
        ("next_notice", json!(0)),
        ("version", json!(0)),
    ] {
        let mut bad = good.clone();
        bad[field] = value;
        reject(&bad);
    }
    for (field, value) in [("at", json!(4)), ("id", good["next_id"].clone())] {
        let mut bad = good.clone();
        bad["jobs"][0][field] = value;
        reject(&bad);
    }
    let mut bad = good.clone();
    bad["jobs"][1]["id"] = bad["jobs"][0]["id"].clone();
    reject(&bad);
    let mut bad = good.clone();
    bad["notices"][0]["at"] = json!(6);
    reject(&bad);
    let mut bad = good.clone();
    bad["plugins"]["validation"] = json!(2);
    reject(&bad);
    let mut bad = good.clone();
    bad["resources"]["unknown"] = json!({});
    reject(&bad);
    // Valid histories continue identically with different worker counts and commit budgets.
    let text = good.to_string();
    let mut a = builder().load(&text, 1).unwrap();
    let mut b = builder().load_from(text.as_bytes(), 4).unwrap();
    while a.advance(SimTime(30), 1).unwrap().stop == Stop::Budget {}
    b.advance(SimTime(30), 128).unwrap();
    assert!(a.save().unwrap() == b.save().unwrap());
}
