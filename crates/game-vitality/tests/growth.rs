use game_core::*;
use game_sim::*;
use game_vitality::*;
const YEAR: u64 = 365 * DAY.0;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_traits::plugin(&mut b).unwrap();
    plugin(&mut b).unwrap();
    b
}
fn setup() -> Host {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_traits::Install(
        game_traits::load(&mut game_content::Store::standard()).unwrap(),
    ))
    .unwrap();
    for person in [PersonId(1), PersonId(2)] {
        h.dispatch(Register {
            person,
            born: SimTime(0),
            span: Some(SimDuration(80 * YEAR)),
            base_health: 100,
        })
        .unwrap();
    }
    h
}
#[test]
fn growth_distinguishes_infants_children_and_adults_without_rewriting_potential() {
    let mut h = setup();
    let person = PersonId(1);
    h.dispatch(game_traits::Set {
        person,
        profile: [("strength".into(), 8000), ("intelligence".into(), 6000)].into(),
    })
    .unwrap();
    let profile = h.get::<game_traits::Traits>().unwrap().people[&person].clone();
    let infant = h.query(Assess { person }).unwrap().unwrap();
    h.advance(SimTime(6 * YEAR), 1).unwrap();
    let child = h.query(Assess { person }).unwrap().unwrap();
    h.advance(SimTime(24 * YEAR), 1).unwrap();
    let adult = h.query(Assess { person }).unwrap().unwrap();
    for key in ["strength", "intelligence", "size", "foresight"] {
        assert!(infant.current_traits[key] < child.current_traits[key]);
        assert!(child.current_traits[key] < adult.current_traits[key]);
    }
    assert!(child.current_traits["learning"] > adult.current_traits["learning"]);
    assert_eq!(adult.current_traits["strength"], 8000);
    assert_eq!(
        h.get::<game_traits::Traits>().unwrap().people[&person],
        profile
    );
}
#[test]
fn childhood_conditions_leave_acquired_effects_after_current_condition_recovers() {
    let mut h = setup();
    let deprived = PersonId(1);
    let supported = PersonId(2);
    h.dispatch(Conditions(vec![(deprived, 500)])).unwrap();
    for quarter in 0..72 {
        h.advance(SimTime(quarter * YEAR / 4), 1).unwrap();
        h.dispatch(Experience {
            person: deprived,
            stress: 1000,
            satisfaction: -100,
        })
        .unwrap();
    }
    h.advance(SimTime(18 * YEAR), 1).unwrap();
    h.dispatch(Conditions(vec![(deprived, 1000)])).unwrap();
    let saved = h.save().unwrap();
    h = builder().load(&saved, 4).unwrap();
    h.advance(SimTime(24 * YEAR), 1).unwrap();
    let bad = h.query(Assess { person: deprived }).unwrap().unwrap();
    let good = h.query(Assess { person: supported }).unwrap().unwrap();
    assert_eq!(bad.physical, good.physical);
    assert_eq!(bad.stress, 0);
    assert!(bad.current_traits["intelligence"] < good.current_traits["intelligence"]);
    let life = &h.get::<Vitality>().unwrap().people[&deprived];
    assert!(
        life.disposition(500, "patience", h.now())
            < h.get::<Vitality>().unwrap().people[&supported].disposition(500, "patience", h.now())
    );
    assert!(bad.upbringing.distress() > 500);
    assert_eq!(
        h.get::<game_traits::Traits>()
            .unwrap()
            .ability(deprived, "intelligence"),
        1000
    );
}
#[test]
fn accumulated_childhood_conditions_do_not_depend_on_review_frequency() {
    let mut coarse = setup();
    let mut daily = setup();
    let person = PersonId(1);
    for h in [&mut coarse, &mut daily] {
        h.dispatch(Conditions(vec![(person, 500)])).unwrap();
        h.dispatch(Experience {
            person,
            stress: 300,
            satisfaction: 0,
        })
        .unwrap();
    }
    coarse.advance(SimTime(200 * DAY.0), 1).unwrap();
    coarse.dispatch(Conditions(vec![(person, 500)])).unwrap();
    for day in 1..=200 {
        daily.advance(SimTime(day * DAY.0), 1).unwrap();
        daily.dispatch(Conditions(vec![(person, 500)])).unwrap();
    }
    assert_eq!(
        coarse.get::<Vitality>().unwrap(),
        daily.get::<Vitality>().unwrap()
    );
}
