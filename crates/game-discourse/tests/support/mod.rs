#![allow(dead_code)]
use game_core::*;
use game_sim::*;
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Meet(pub PersonId, pub PersonId);
impl Request for Meet {
    const KEY: &'static str = "test.discourse-meet";
    type Response = ();
}
pub fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_testimony::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_nutrition::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_conflict::plugin(&mut b).unwrap();
    game_pressure::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_institutions::plugin(&mut b).unwrap();
    game_communities::plugin(&mut b).unwrap();
    b.plugin("discourse-fixture", 1).unwrap();
    game_discourse::plugin(&mut b).unwrap();
    b.request::<Meet>(|tx, r| {
        tx.emit(
            game_social_life::Encounter {
                first: r.0,
                second: r.1,
            },
            vec![r.0, r.1],
        )
    })
    .unwrap();
    b.request::<ObservedFeeding>(|tx, r| {
        tx.emit(
            game_physiology::Fed {
                inventory: None,
                caregiver: r.caregiver,
                patient: r.patient,
                good: game_cargo::FOOD,
                quantity: 10,
            },
            vec![r.caregiver, r.patient],
        )
    })
    .unwrap();
    b
}
pub fn setup() -> (Host, Vec<PersonId>, AreaId) {
    setup_with_food_effect(0)
}
pub fn setup_with_food_effect(health: i32) -> (Host, Vec<PersonId>, AreaId) {
    let mut h = builder().build(7, 1).unwrap();
    h.advance(SimTime(30 * 365 * DAY.0), 1).unwrap();
    let mut objects = game_objects::standard().unwrap();
    if health != 0 {
        objects
            .items
            .values_mut()
            .find(|o| o.id == game_cargo::FOOD)
            .unwrap()
            .food
            .as_mut()
            .unwrap()
            .effects
            .health = health;
        objects.stamps.clear();
    }
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Meeting place".into(),
        })
        .unwrap();
    let mut people = vec![];
    for _ in 0..4 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        h.dispatch(game_social_life::Register {
            person: p,
            name: format!("Person {}", p.0),
        })
        .unwrap();
        h.dispatch(game_relationships::Initialize {
            person: p,
            beliefs: Default::default(),
            values: Default::default(),
        })
        .unwrap();
        people.push(p);
    }
    (h, people, area)
}
pub fn profile(h: &mut Host, p: PersonId, values: &[(&str, i32)]) {
    h.dispatch(game_traits::Set {
        person: p,
        profile: values.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
    })
    .unwrap();
}
pub fn value(h: &Host, p: PersonId) -> i32 {
    h.get::<game_relationships::Social>()
        .unwrap()
        .opinion(p, "care")
}
pub fn stance(h: &mut Host, p: PersonId, amount: i32) {
    for _ in 0..amount.abs() / 5 {
        h.dispatch(game_relationships::LearnValue {
            person: p,
            concept: "care".into(),
            outcome: amount.signum() * 5,
        })
        .unwrap();
    }
}
pub fn say(
    h: &mut Host,
    speaker: PersonId,
    listener: PersonId,
    stance: i32,
    approach: game_discourse::Approach,
) {
    h.dispatch(game_discourse::Advocate {
        speaker,
        listener,
        concept: "care".into(),
        stance,
        approach,
    })
    .unwrap();
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ObservedFeeding {
    pub caregiver: PersonId,
    pub patient: PersonId,
}
impl Request for ObservedFeeding {
    const KEY: &'static str = "test.discourse-observed-feeding";
    type Response = ();
}
