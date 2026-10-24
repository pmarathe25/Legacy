use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Assess(game_signals::Notice);
impl Request for Assess {
    const KEY: &'static str = "test.assess-signal";
    type Response = game_signals::Assessment;
}
#[derive(Clone, Serialize, Deserialize)]
struct Observe {
    observer: PersonId,
    person: PersonId,
    filtered: bool,
    reject: bool,
}
impl Request for Observe {
    const KEY: &'static str = "test.care-evidence";
    type Response = bool;
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_signals::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.request::<Assess>(|tx, r| game_signals::assess(tx, r.0))
        .unwrap();
    b.request::<Observe>(|tx, r| {
        let changed = game_care_perception::notice_signal(tx, r.observer, r.person, r.filtered);
        require(!r.reject, "rollback after observation")?;
        Ok(changed)
    })
    .unwrap();
    b
}
fn setup() -> (Host, PersonId, PersonId) {
    let mut h = builder().build(7, 1).unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: MINUTE,
        hunger_per_tick: 10,
        fatigue_per_tick: 0,
        deprivation: 10000,
        ..Default::default()
    }))
    .unwrap();
    h.dispatch(game_signals::Enable).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "home".into(),
        })
        .unwrap();
    let mut people = vec![];
    for _ in 0..2 {
        let person = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        people.push(person);
    }
    (h, people[0], people[1])
}
fn observe(h: &mut Host, observer: PersonId, person: PersonId, filtered: bool) -> bool {
    h.dispatch(Observe {
        observer,
        person,
        filtered,
        reject: false,
    })
    .unwrap()
}
#[test]
fn pure_assessment_and_owner_application_match_the_original_notice() {
    let (mut h, observer, person) = setup();
    h.dispatch(game_physiology::Strain {
        person,
        amount: 100,
    })
    .unwrap();
    let before = h.save().unwrap();
    let assessment = h
        .query(Assess(game_signals::Notice { observer, person }))
        .unwrap();
    assert_eq!(
        h.save().unwrap(),
        before,
        "pure assessment changed the world"
    );
    let mut ordinary = builder().load(&before, 4).unwrap();
    let perception = ordinary
        .dispatch(game_signals::Notice { observer, person })
        .unwrap();
    assert_eq!(assessment.perception(), &perception);
    h.dispatch(game_signals::ApplyAssessment(assessment))
        .unwrap();
    assert_eq!(h.save().unwrap(), ordinary.save().unwrap());
}
#[test]
fn applying_an_old_assessment_preserves_newer_personal_learning() {
    let (mut h, observer, person) = setup();
    let old = h
        .query(Assess(game_signals::Notice { observer, person }))
        .unwrap();
    h.dispatch(game_physiology::Strain {
        person,
        amount: 100,
    })
    .unwrap();
    let newer = h
        .dispatch(game_signals::Notice { observer, person })
        .unwrap();
    assert_ne!(old.perception(), &newer);
    let before = h.save().unwrap();
    let mut ordinary = builder().load(&before, 4).unwrap();
    ordinary
        .dispatch(game_signals::Notice { observer, person })
        .unwrap();
    h.dispatch(game_signals::ApplyAssessment(old)).unwrap();
    assert_eq!(h.save().unwrap(), ordinary.save().unwrap());
}
#[test]
fn filtering_records_rest_cues_without_requesting_food_attention() {
    let (mut h, observer, person) = setup();
    h.dispatch(game_physiology::Strain {
        person,
        amount: 100,
    })
    .unwrap();
    assert!(!observe(&mut h, observer, person, true));
    let p = &h.get::<game_signals::Signals>().unwrap().perceptions[&observer][&person];
    assert_eq!(p.guessed_need.as_deref(), Some("rest"));
    assert_eq!(p.cue, "drooping");
    assert!(observe(&mut h, observer, person, false));
}
#[test]
fn only_changed_food_evidence_wakes_but_timestamps_and_rollback_are_preserved() {
    let (mut h, observer, person) = setup();
    h.dispatch(game_physiology::Start).unwrap();
    while h.advance(SimTime(MINUTE.0), 128).unwrap().stop == Stop::Budget {}
    assert!(observe(&mut h, observer, person, true));
    let saved = h.save().unwrap();
    assert!(
        h.dispatch(Observe {
            observer,
            person,
            filtered: true,
            reject: true
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), saved);
    while h.advance(SimTime(MINUTE.0 + 1), 128).unwrap().stop == Stop::Budget {}
    assert!(!observe(&mut h, observer, person, true));
    let memory = &h.get::<game_signals::Signals>().unwrap().perceptions[&observer][&person];
    assert_eq!(memory.at, h.now());
    assert_eq!(memory.food.unwrap().0, h.now());
    let mut restored = builder().load(&h.save().unwrap(), 4).unwrap();
    for host in [&mut h, &mut restored] {
        while host.advance(SimTime(2 * MINUTE.0), 13).unwrap().stop == Stop::Budget {}
        assert!(observe(host, observer, person, true));
    }
    assert_eq!(h.save().unwrap(), restored.save().unwrap());
}
#[test]
fn failed_observations_keep_the_ordinary_prompt() {
    let (mut h, observer, _) = setup();
    assert!(observe(&mut h, observer, PersonId(u64::MAX), true));
}
