use game_body::*;
use game_core::*;
use game_sim::*;
use std::collections::BTreeMap;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct FailedAdvance;
impl Request for FailedAdvance {
    const KEY: &'static str = "test.failed-advance";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_soma::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<FailedAdvance>(|tx, _| {
        tx.call(game_soma::Advance)?;
        Err(Error::Rejected("discard the phase".into()))
    })
    .unwrap();
    b
}
#[test]
fn sparse_living_updates_preserve_eager_clocks_rollback_readers_and_reload() {
    let mut host = builder().build(5, 1).unwrap();
    let rules = host.get::<game_soma::Bodies>().unwrap().rules.clone();
    let mut expected = BTreeMap::new();
    for i in 0..100 {
        host.advance(SimTime(i * 17), 1).unwrap();
        let p = host
            .dispatch(game_soma::Create {
                kind: Kind::Mortal,
                born: host.now(),
            })
            .unwrap();
        if i < 96 {
            host.dispatch(game_soma::Damage {
                person: p,
                amount: 1000,
            })
            .unwrap();
        }
        expected.insert(p, host.get::<game_soma::Bodies>().unwrap().people[&p]);
    }
    for step in 1..=8 {
        let at = SimTime(step * MINUTE.0 + 7000);
        host.advance(at, 1).unwrap();
        let retained = host.get::<game_soma::Bodies>().unwrap().clone();
        let retained_wire = serde_json::to_value(&retained).unwrap();
        let before = host.save().unwrap();
        assert!(host.dispatch(FailedAdvance).is_err());
        assert_eq!(host.save().unwrap(), before);
        host.dispatch(game_soma::Advance).unwrap();
        for body in expected.values_mut() {
            *body = advance_body(body, &rules, at);
        }
        let bodies = host.get::<game_soma::Bodies>().unwrap();
        assert_eq!(bodies.living().count(), 4);
        assert_eq!(
            serde_json::to_value(bodies).unwrap()["people"],
            serde_json::to_value(&expected).unwrap()
        );
        assert_eq!(serde_json::to_value(&retained).unwrap(), retained_wire);
        let save = host.save().unwrap();
        host = builder()
            .load(&save, if step % 2 == 0 { 4 } else { 1 })
            .unwrap();
        assert_eq!(host.save().unwrap(), save);
    }
}

#[test]
fn an_all_deceased_population_roundtrips_after_an_empty_living_phase() {
    let mut host = builder().build(8, 1).unwrap();
    let p = host
        .dispatch(game_soma::Create {
            kind: Kind::Mortal,
            born: host.now(),
        })
        .unwrap();
    host.dispatch(game_soma::Damage {
        person: p,
        amount: 1000,
    })
    .unwrap();
    let initial = host.get::<game_soma::Bodies>().unwrap().people[&p];
    host.advance(SimTime(2 * MINUTE.0 + 13), 1).unwrap();
    host.dispatch(game_soma::Advance).unwrap();
    let bodies = host.get::<game_soma::Bodies>().unwrap();
    assert_eq!(bodies.living().count(), 0);
    let eager = advance_body(&initial, &bodies.rules, host.now());
    assert_eq!(
        serde_json::to_value(bodies).unwrap()["people"][p.0.to_string()],
        serde_json::to_value(eager).unwrap()
    );
    let save = host.save().unwrap();
    assert_eq!(builder().load(&save, 4).unwrap().save().unwrap(), save);
}
