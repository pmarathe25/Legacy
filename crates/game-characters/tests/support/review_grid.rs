use crate::support;
use game_characters::*;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Reviews(Vec<SimTime>);
impl Resource for Reviews {
    const KEY: &'static str = "test.grid-reviews";
}
#[derive(Clone, Serialize, Deserialize)]
struct Evidence(PersonId);
impl Request for Evidence {
    const KEY: &'static str = "test.grid-evidence";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    b.plugin("review-grid-test", 1).unwrap();
    b.resource::<Reviews>().unwrap();
    b.request::<Evidence>(|tx, r| tx.notify(game_choice::Changed { person: r.0 }))
        .unwrap();
    game_choice::serial_provider(&mut b, "test.grid", |tx, _| {
        let now = tx.now();
        tx.edit::<Reviews>()?.0.push(now);
        Ok(())
    })
    .unwrap();
    b
}
fn idle_world() -> (Host, PersonId, SimDuration) {
    let period = SimDuration(12 * HOUR.0);
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: period,
        hunger_per_tick: 0,
        fatigue_per_tick: 0,
        ..Default::default()
    }))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let p = h
        .dispatch(game_physiology::Create {
            kind: game_physiology::Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    h.dispatch(game_transport::Place { person: p, area })
        .unwrap();
    h.dispatch(Configure(Rules {
        review: period,
        attention: Some(AttentionRates {
            ordinary: SimDuration(1),
            evidence: SimDuration(1),
        }),
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(Register {
        person: p,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    support::run(&mut h, SimTime(period.0));
    (h, p, period)
}
#[test]
fn enabling_attention_does_not_invent_a_population_wide_initial_review() {
    let (mut h, _, period) = idle_world();
    h.dispatch(ConfigureAttention(None)).unwrap();
    h.dispatch(ConfigureAttention(Some(AttentionRates {
        ordinary: SimDuration(1),
        evidence: SimDuration(1),
    })))
    .unwrap();
    assert_eq!(
        h.scheduled()
            .filter(|j| j.request.key() == Resume::KEY)
            .count(),
        0
    );
    support::run(&mut h, SimTime(period.0));
    assert_eq!(h.get::<Reviews>().unwrap().0.len(), 1);
    support::run(&mut h, SimTime(2 * period.0));
    assert_eq!(h.get::<Reviews>().unwrap().0.len(), 2);
}
#[test]
fn a_one_tick_deferred_idle_review_does_not_lose_the_next_half_day_after_reload() {
    let (mut h, p, period) = idle_world();
    assert_eq!(h.get::<Reviews>().unwrap().0, vec![SimTime(period.0)]);
    assert!(!h.get::<Actions>().unwrap().0.contains_key(&p));
    h.dispatch(Evidence(p)).unwrap();
    support::run(&mut h, SimTime(period.0 + 1));
    assert_eq!(h.get::<Reviews>().unwrap().0.len(), 2);
    assert_eq!(
        h.get::<Minds>().unwrap().schedule.agenda.get(&p),
        Some(&SimTime(2 * period.0))
    );
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut loaded] {
        support::run(host, SimTime(2 * period.0));
        assert_eq!(host.get::<Reviews>().unwrap().0.len(), 3);
        assert_eq!(
            host.get::<Reviews>().unwrap().0.last(),
            Some(&SimTime(2 * period.0))
        );
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
}
