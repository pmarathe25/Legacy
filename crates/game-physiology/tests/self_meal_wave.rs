mod support;
use game_core::*;
use game_physiology::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
struct Scalar(Vec<ConsumeEdible>);
impl Request for Scalar {
    const KEY: &'static str = "test.scalar-self-meals";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Reject(ConsumeSelfMeals);
impl Request for Reject {
    const KEY: &'static str = "test.reject-self-meals";
    type Response = ();
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Hooks {
    trigger: Option<PersonId>,
    target: Option<PersonId>,
    calls: u64,
    reject_fed: bool,
    watch: Option<PersonId>,
    observed_hunger: Option<u64>,
    observed_satisfaction: Option<i32>,
}
impl Resource for Hooks {
    const KEY: &'static str = "test.self-meal-hooks";
}
#[derive(Clone, Serialize, Deserialize)]
struct Configure(Hooks);
impl Request for Configure {
    const KEY: &'static str = "test.configure-self-meals";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Sense(PersonId);
impl Request for Sense {
    const KEY: &'static str = "test.sense-self-meals";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Capture;
impl Request for Capture {
    const KEY: &'static str = "test.capture-self-meals";
    type Response = ();
}

fn builder(react: bool) -> Builder {
    builder_with_items(react, false)
}
fn builder_with_items(react: bool, items: bool) -> Builder {
    let mut b = support::builder();
    if items {
        game_items::plugin(&mut b).unwrap();
    }
    b.plugin("self-meal-test", 1).unwrap();
    b.resource::<Hooks>().unwrap();
    b.request::<Configure>(|tx, r| {
        *tx.edit::<Hooks>()? = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Sense>(|tx, r| {
        let state = tx.get::<Hooks>()?.clone();
        tx.edit::<Hooks>()?.calls += 1;
        if state.trigger == Some(r.0) {
            tx.call(game_physiology::Damage {
                person: state.target.unwrap(),
                amount: 1,
            })?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Capture>(|tx, _| {
        let state = tx.get::<Hooks>()?;
        if let (Some(person), None) = (state.watch, state.observed_hunger) {
            let hunger = tx.get::<Bodies>()?.people[&person].hunger;
            let satisfaction = game_vitality::condition(tx, person).map(|(_, value)| value);
            tx.edit::<Hooks>()?.observed_hunger = Some(hunger);
            tx.edit::<Hooks>()?.observed_satisfaction = satisfaction;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Scalar>(|tx, r| {
        for meal in r.0 {
            tx.call(meal)?;
        }
        Ok(())
    })
    .unwrap();
    b.request::<Reject>(|tx, r| {
        tx.call(r.0)?;
        Err(Error::Rejected("rollback meal wave".into()))
    })
    .unwrap();
    if react {
        b.subscribe::<Eating>("test.meal-sense", |tx, e| tx.call(Sense(e.person)))
            .unwrap();
        b.subscribe::<Fed>("test.meal-fed", |tx, _| {
            require(!tx.get::<Hooks>()?.reject_fed, "rejected feeding")?;
            tx.call(Capture)
        })
        .unwrap();
    }
    b
}

#[test]
fn first_fed_observer_sees_all_successful_body_updates() {
    let (mut source, first, second, store) = fixture(true);
    source
        .dispatch(game_body::Exert {
            person: second,
            energy: 1_000_000,
            fatigue: 0,
        })
        .unwrap();
    source
        .dispatch(Configure(Hooks {
            watch: Some(second),
            ..Default::default()
        }))
        .unwrap();
    let checkpoint = source.save().unwrap();
    let meals = vec![meal(first, store), meal(second, store)];
    let mut scalar = builder(true).load(&checkpoint, 1).unwrap();
    scalar.dispatch(Scalar(meals.clone())).unwrap();
    source
        .dispatch(ConsumeSelfMeals {
            meals,
            minimum_parallel: 1,
        })
        .unwrap();
    let after = source.get::<Bodies>().unwrap().people[&second].hunger;
    assert_eq!(source.get::<Hooks>().unwrap().observed_hunger, Some(after));
    assert!(scalar.get::<Hooks>().unwrap().observed_hunger.unwrap() > after);
}

#[test]
fn first_fed_observer_sees_all_nutritional_mood_effects() {
    fn rich_builder() -> Builder {
        let mut b = builder(true);
        game_traits::plugin(&mut b).unwrap();
        game_vitality::plugin(&mut b).unwrap();
        game_nutrition::plugin(&mut b).unwrap();
        b
    }
    let mut host = rich_builder().build(17, 1).unwrap();
    let mut objects = game_objects::standard().unwrap();
    objects
        .items
        .values_mut()
        .find(|o| o.id == game_cargo::FOOD)
        .unwrap()
        .food
        .as_mut()
        .unwrap()
        .effects
        .mood = 40;
    host.dispatch(game_objects::Install(objects)).unwrap();
    host.advance(SimTime(40 * 365 * DAY.0), 1).unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "nutrition wave".into(),
        })
        .unwrap();
    let first = host
        .dispatch(game_soma::Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let second = host
        .dispatch(game_soma::Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    for person in [first, second] {
        host.dispatch(game_transport::Place { person, area })
            .unwrap();
        host.dispatch(game_vitality::Register {
            person,
            born: SimTime(0),
            span: None,
            base_health: 100,
        })
        .unwrap();
    }
    host.dispatch(game_vitality::Enable).unwrap();
    host.dispatch(game_nutrition::Enable).unwrap();
    let store = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 10_000,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 1000,
    })
    .unwrap();
    host.dispatch(Configure(Hooks {
        watch: Some(second),
        ..Default::default()
    }))
    .unwrap();
    let checkpoint = host.save().unwrap();
    let meals = vec![meal(first, store), meal(second, store)];
    let mut scalar = rich_builder().load(&checkpoint, 1).unwrap();
    scalar.dispatch(Scalar(meals.clone())).unwrap();
    host.dispatch(ConsumeSelfMeals {
        meals,
        minimum_parallel: 1,
    })
    .unwrap();
    let satisfaction = host.get::<game_vitality::Vitality>().unwrap().people[&second]
        .condition(host.now())
        .1;
    assert!(satisfaction > 0);
    assert_eq!(
        host.get::<Hooks>().unwrap().observed_satisfaction,
        Some(satisfaction)
    );
    assert_eq!(
        scalar.get::<Hooks>().unwrap().observed_satisfaction,
        Some(0)
    );
    let saved = host.save().unwrap();
    assert_eq!(
        rich_builder().load(&saved, 4).unwrap().save().unwrap(),
        saved
    );
}
fn fixture(react: bool) -> (Host, PersonId, PersonId, InventoryId) {
    fixture_with_items(react, false)
}
fn fixture_with_items(react: bool, items: bool) -> (Host, PersonId, PersonId, InventoryId) {
    let mut host = builder_with_items(react, items).build(7, 1).unwrap();
    host.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    let area = host
        .dispatch(game_land::CreateArea {
            name: "shared meal".into(),
        })
        .unwrap();
    let first = host
        .dispatch(game_soma::Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    let second = host
        .dispatch(game_soma::Create {
            kind: Kind::Mortal,
            born: SimTime(0),
        })
        .unwrap();
    for person in [first, second] {
        host.dispatch(game_transport::Place { person, area })
            .unwrap();
    }
    let inventory = host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 10_000,
        })
        .unwrap();
    host.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity: 1000,
    })
    .unwrap();
    (host, first, second, inventory)
}

#[test]
fn individual_stock_notifications_preserve_named_item_consumption() {
    let (mut host, first, second, store) = fixture_with_items(false, true);
    host.dispatch(game_cargo::Consume {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 980,
    })
    .unwrap();
    let item = host
        .dispatch(game_items::Mark {
            person: first,
            inventory: store,
            good: game_cargo::FOOD,
            label: "particular meal".into(),
        })
        .unwrap();
    let checkpoint = host.save().unwrap();
    let meals = vec![meal(first, store), meal(second, store)];
    let mut scalar = builder_with_items(false, true)
        .load(&checkpoint, 1)
        .unwrap();
    scalar.dispatch(Scalar(meals.clone())).unwrap();
    host.dispatch(ConsumeSelfMeals {
        meals,
        minimum_parallel: 1,
    })
    .unwrap();
    assert_eq!(
        host.get::<game_items::Items>().unwrap(),
        scalar.get::<game_items::Items>().unwrap()
    );
    assert_eq!(
        host.get::<game_items::Items>().unwrap().objects[&item].inventory,
        None
    );
    assert_eq!(
        host.get::<game_cargo::Cargo>().unwrap(),
        scalar.get::<game_cargo::Cargo>().unwrap()
    );
    let saved = host.save().unwrap();
    assert_eq!(
        builder_with_items(false, true)
            .load(&saved, 4)
            .unwrap()
            .save()
            .unwrap(),
        saved
    );
}
fn meal(person: PersonId, inventory: InventoryId) -> ConsumeEdible {
    ConsumeEdible {
        person,
        inventory,
        good: game_cargo::FOOD,
        quantity: 10,
    }
}

#[test]
fn owner_wave_matches_physical_outcomes_and_replays_across_workers() {
    let (mut host, first, second, store) = fixture(false);
    for person in [first, second] {
        host.dispatch(game_body::Exert {
            person,
            energy: 250_000,
            fatigue: 60_000,
        })
        .unwrap();
    }
    let checkpoint = host.save().unwrap();
    let meals = vec![meal(first, store), meal(second, store)];
    let mut scalar = builder(false).load(&checkpoint, 1).unwrap();
    scalar.dispatch(Scalar(meals.clone())).unwrap();
    let mut reference = None;
    for workers in [1, 4, 12] {
        let mut wave = builder(false).load(&checkpoint, workers).unwrap();
        wave.dispatch(ConsumeSelfMeals {
            meals: meals.clone(),
            minimum_parallel: 1,
        })
        .unwrap();
        assert_eq!(
            wave.get::<game_cargo::Cargo>().unwrap(),
            scalar.get::<game_cargo::Cargo>().unwrap()
        );
        assert_eq!(
            wave.get::<Bodies>().unwrap(),
            scalar.get::<Bodies>().unwrap()
        );
        assert_eq!(
            wave.get::<game_digestion::Digestion>().unwrap(),
            scalar.get::<game_digestion::Digestion>().unwrap()
        );
        let saved = wave.save().unwrap();
        if let Some(expected) = &reference {
            assert_eq!(&saved, expected);
        } else {
            reference = Some(saved.clone());
        }
        assert_eq!(
            builder(false).load(&saved, 4).unwrap().save().unwrap(),
            saved
        );
    }
}

#[test]
fn intervening_damage_retries_scalar_without_duplicate_effects() {
    for forward in [false, true] {
        let (mut source, first, second, store) = fixture(true);
        source
            .dispatch(Configure(Hooks {
                trigger: Some(if forward { first } else { second }),
                target: Some(if forward { second } else { first }),
                ..Default::default()
            }))
            .unwrap();
        let checkpoint = source.save().unwrap();
        let meals = vec![meal(first, store), meal(second, store)];
        let mut scalar = builder(true).load(&checkpoint, 1).unwrap();
        scalar.dispatch(Scalar(meals.clone())).unwrap();
        source
            .dispatch(ConsumeSelfMeals {
                meals,
                minimum_parallel: 1,
            })
            .unwrap();
        assert_eq!(source.save().unwrap(), scalar.save().unwrap());
        assert_eq!(source.get::<Hooks>().unwrap().calls, 2);
    }
}

#[test]
fn a_scalar_meal_hook_forces_ordered_execution() {
    fn hooked_builder() -> Builder {
        let mut b = builder(false);
        b.before::<ConsumeEdible>("test.meal-hook", |tx, meal| tx.call(Sense(meal.person)))
            .unwrap();
        b
    }
    let (source, first, second, store) = fixture(false);
    let checkpoint = source.save().unwrap();
    let meals = vec![meal(first, store), meal(second, store)];
    let mut scalar = hooked_builder().load(&checkpoint, 1).unwrap();
    let mut wave = hooked_builder().load(&checkpoint, 4).unwrap();
    scalar.dispatch(Scalar(meals.clone())).unwrap();
    wave.dispatch(ConsumeSelfMeals {
        meals,
        minimum_parallel: 1,
    })
    .unwrap();
    assert_eq!(wave.save().unwrap(), scalar.save().unwrap());
    assert_eq!(wave.get::<Hooks>().unwrap().calls, 2);
}

#[test]
fn failures_rollback_stock_rows_notices_and_identity_counters() {
    let (mut host, first, second, store) = fixture(true);
    let meals = vec![meal(first, store), meal(second, store)];
    let before = host.save().unwrap();
    assert!(
        host.dispatch(Reject(ConsumeSelfMeals {
            meals: meals.clone(),
            minimum_parallel: 1
        }))
        .is_err()
    );
    assert_eq!(host.save().unwrap(), before);
    host.dispatch(Configure(Hooks {
        reject_fed: true,
        ..Default::default()
    }))
    .unwrap();
    let before = host.save().unwrap();
    assert!(
        host.dispatch(ConsumeSelfMeals {
            meals,
            minimum_parallel: 1
        })
        .is_err()
    );
    assert_eq!(host.save().unwrap(), before);
}
