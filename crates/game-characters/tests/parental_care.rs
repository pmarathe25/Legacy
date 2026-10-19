mod support;
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Distraction(PersonId);
impl Request for Distraction {
    const KEY: &'static str = "test.distraction";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Done(PersonId);
impl Request for Done {
    const KEY: &'static str = "test.distraction-done";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel(ScheduleId);
impl Request for Cancel {
    const KEY: &'static str = "test.distraction-cancel";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = support::builder();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_households::plugin(&mut b).unwrap();
    game_family_life::plugin(&mut b).unwrap();
    game_communication::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_care::plugin(&mut b).unwrap();
    b.plugin("distraction", 1).unwrap();
    b.request::<Distraction>(|tx, r| {
        let until = tx.now().after(HOUR)?;
        let job = tx.schedule(until, 20, Done(r.0))?;
        tx.start_action(r.0, "pastime", Some(until), Cancel(job))
    })
    .unwrap();
    b.request::<Done>(|tx, r| tx.finish_action(r.0, "pastime"))
        .unwrap();
    b.request::<Cancel>(|tx, r| {
        tx.unschedule(r.0);
        Ok(())
    })
    .unwrap();
    game_choice::provider(&mut b, "test.competing-interest", |tx, e| {
        if game_vitality::can_manipulate(tx, e.person) {
            game_choice::offer(
                tx,
                e.person,
                "pleasant-distraction",
                game_choice::motive(0, 0, 130, 0, 0),
                Distraction(e.person),
            )?;
        }
        Ok(())
    })
    .unwrap();
    b
}
fn setup(altruism: i32) -> (Host, PersonId, PersonId) {
    let mut h = builder().build(11, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    h.dispatch(game_physiology::Configure(game_physiology::Rules {
        tick: HOUR,
        hunger_per_tick: 15,
        fatigue_per_tick: 2,
        harm_per_tick: 5,
        ..Default::default()
    }))
    .unwrap();
    let rules = game_characters::Rules {
        review: HOUR,
        ..Default::default()
    };
    h.dispatch(game_characters::Configure(rules)).unwrap();
    h.dispatch(game_vitality::Enable).unwrap();
    h.dispatch(game_communication::Enable).unwrap();
    h.advance(SimTime(30 * 365 * DAY.0), 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "Home".into(),
        })
        .unwrap();
    let mut parents = vec![];
    for _ in 0..2 {
        let p = h
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person: p, area })
            .unwrap();
        parents.push(p);
    }
    let parent = parents[0];
    h.dispatch(game_traits::Set {
        person: parent,
        profile: [
            ("altruism".into(), altruism),
            ("empathy".into(), 1000),
            ("compassion".into(), 1000),
            ("intelligence".into(), 6000),
            ("foresight".into(), 4000),
            ("metabolism".into(), 250),
        ]
        .into(),
    })
    .unwrap();
    h.dispatch(game_physiology::Express { person: parent })
        .unwrap();
    h.dispatch(game_social_life::Register {
        person: parent,
        name: "Parent".into(),
    })
    .unwrap();
    h.dispatch(game_relationships::Initialize {
        person: parent,
        beliefs: Default::default(),
        values: Default::default(),
    })
    .unwrap();
    let pack = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(parent),
            capacity: 2000,
        })
        .unwrap();
    h.dispatch(game_property::ClaimInventory {
        claimant: parent,
        inventory: pack,
        recognized_by: [parent].into(),
    })
    .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: pack,
        good: game_cargo::FOOD,
        quantity: 1000,
    })
    .unwrap();
    h.dispatch(game_households::Enable).unwrap();
    let child = h
        .dispatch(game_lifecycle::Birth {
            mother: parent,
            father: parents[1],
            caregiver: parent,
            area,
        })
        .unwrap();
    h.dispatch(game_physiology::Express { person: child })
        .unwrap();
    h.dispatch(game_characters::Register {
        person: parent,
        mind: Default::default(),
    })
    .unwrap();
    h.dispatch(game_physiology::Start).unwrap();
    h.dispatch(game_characters::Start).unwrap();
    (h, parent, child)
}
#[test]
fn a_concerned_parent_can_choose_food_care_over_an_absorbing_interest() {
    let (mut h, _, child) = setup(1000);
    let target = h.now().after(SimDuration(2 * DAY.0)).unwrap();
    let split = h.now().after(SimDuration(12 * HOUR.0)).unwrap();
    support::run(&mut h, split);
    let mut loaded = builder().load(&h.save().unwrap(), 4).unwrap();
    support::run(&mut h, target);
    support::run(&mut loaded, target);
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    let body = h.get::<game_physiology::Bodies>().unwrap().people[&child];
    assert!(
        body.alive,
        "a capable motivated parent never completed a meal for the child"
    );
    assert!(body.hunger < 120);
}
#[test]
fn an_indifferent_parent_can_keep_the_interest_and_neglect_the_child() {
    let (mut h, _, child) = setup(0);
    let target = h.now().after(SimDuration(2 * DAY.0)).unwrap();
    support::run(&mut h, target);
    assert!(!h.get::<game_physiology::Bodies>().unwrap().people[&child].alive);
}
