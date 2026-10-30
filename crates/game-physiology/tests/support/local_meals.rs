use super::support;
use game_core::*;
use game_physiology::*;
use game_sim::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize, Deserialize)]
struct Hooks {
    calls: u64,
    target: Option<PersonId>,
    reject: bool,
    hunger_at_first_fed: Option<u64>,
}
impl Resource for Hooks {
    const KEY: &'static str = "test.local-meal-hooks";
}
#[derive(Clone, Serialize, Deserialize)]
struct Configure(Hooks);
impl Request for Configure {
    const KEY: &'static str = "test.configure-local-meals";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Reject(ServeLocalMeals);
impl Request for Reject {
    const KEY: &'static str = "test.reject-local-meals";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Scalar(Vec<Serve>);
impl Request for Scalar {
    const KEY: &'static str = "test.scalar-local-meals";
    type Response = Vec<OfferOutcome>;
}
fn builder() -> Builder {
    let mut b = support::builder();
    game_traits::plugin(&mut b).unwrap();
    game_vitality::plugin(&mut b).unwrap();
    game_nutrition::plugin(&mut b).unwrap();
    b.plugin("local-meal-test", 1).unwrap();
    b.resource::<Hooks>().unwrap();
    b.request::<Configure>(|tx, r| {
        *tx.edit::<Hooks>()? = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Reject>(|tx, r| {
        tx.call(r.0)?;
        Err(Error::Rejected("outer rejection".into()))
    })
    .unwrap();
    b.request::<Scalar>(|tx, r| {
        r.0.into_iter()
            .map(|meal| tx.call(TryServe(meal)))
            .collect()
    })
    .unwrap();
    b.before::<Serve>("test.actual-offer", |tx, _| {
        tx.edit::<Hooks>()?.calls += 1;
        require(!tx.get::<Hooks>()?.reject, "offered food declined")
    })
    .unwrap();
    b.subscribe::<Eating>("test.cross-participant", |tx, _| {
        if let Some(person) = tx.get::<Hooks>()?.target {
            tx.call(Damage { person, amount: 1 })?;
        }
        Ok(())
    })
    .unwrap();
    b.subscribe::<Fed>("test.settled-bodies", |tx, _| {
        if tx.get::<Hooks>()?.hunger_at_first_fed.is_none() {
            let hunger = tx
                .get::<Bodies>()?
                .people
                .rows()
                .map(|(_, body)| body.hunger)
                .sum();
            tx.edit::<Hooks>()?.hunger_at_first_fed = Some(hunger);
        }
        Ok(())
    })
    .unwrap();
    b
}
fn fixture() -> (Host, Vec<PersonId>, InventoryId) {
    let mut h = builder().build(7, 4).unwrap();
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
    h.dispatch(game_objects::Install(objects)).unwrap();
    h.advance(SimTime(40 * 365 * DAY.0), 1).unwrap();
    let area = h
        .dispatch(game_land::CreateArea {
            name: "local meals".into(),
        })
        .unwrap();
    let mut people = Vec::new();
    for _ in 0..6 {
        let person = h
            .dispatch(Create {
                kind: Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        h.dispatch(game_transport::Place { person, area }).unwrap();
        h.dispatch(game_vitality::Register {
            person,
            born: SimTime(0),
            span: None,
            base_health: 100,
        })
        .unwrap();
        people.push(person);
    }
    h.dispatch(game_vitality::Enable).unwrap();
    h.dispatch(game_nutrition::Enable).unwrap();
    let store = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(area),
            capacity: 100_000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 2000,
    })
    .unwrap();
    h.dispatch(Start).unwrap();
    let at = h.now().after(SimDuration(35 * MINUTE.0)).unwrap();
    support::run(&mut h, at);
    (h, people, store)
}
fn batch(people: &[PersonId], store: InventoryId) -> ServeLocalMeals {
    let meal = |caregiver, patient, quantity| Serve {
        caregiver,
        patient,
        inventory: store,
        good: game_cargo::FOOD,
        quantity,
    };
    ServeLocalMeals {
        meals: vec![
            meal(people[0], people[0], 1),
            meal(people[1], people[2], 1),
            meal(people[3], people[4], 1000),
            meal(people[5], people[5], 1),
        ],
        minimum_parallel: 1,
    }
}
#[test]
fn parallel_personal_meals_match_workers_and_reload_with_decline_hook_rollback() {
    let (h, people, store) = fixture();
    let saved = h.save().unwrap();
    let mut one = builder().load(&saved, 1).unwrap();
    let mut four = builder().load(&saved, 4).unwrap();
    let mut scalar = builder().load(&saved, 1).unwrap();
    scalar
        .dispatch(Scalar(batch(&people, store).meals))
        .unwrap();
    for host in [&mut one, &mut four] {
        assert_eq!(
            host.dispatch(batch(&people, store)).unwrap(),
            vec![
                OfferOutcome::Accepted,
                OfferOutcome::Accepted,
                OfferOutcome::Declined,
                OfferOutcome::Accepted
            ]
        );
        assert_eq!(host.get::<Hooks>().unwrap().calls, 3);
        assert_eq!(support::food(host, store), 1997);
        let sum = host
            .get::<Bodies>()
            .unwrap()
            .people
            .rows()
            .map(|(_, body)| body.hunger)
            .sum();
        assert_eq!(host.get::<Hooks>().unwrap().hunger_at_first_fed, Some(sum));
        assert!(
            host.get::<game_vitality::Vitality>().unwrap().people[&people[0]]
                .condition(host.now())
                .1
                > 0
        );
    }
    assert_eq!(one.save().unwrap(), four.save().unwrap());
    assert_eq!(
        one.get::<Bodies>().unwrap().people,
        scalar.get::<Bodies>().unwrap().people
    );
    assert_eq!(
        one.get::<game_digestion::Digestion>().unwrap(),
        scalar.get::<game_digestion::Digestion>().unwrap()
    );
    assert_eq!(
        one.get::<game_vitality::Vitality>().unwrap(),
        scalar.get::<game_vitality::Vitality>().unwrap()
    );
    assert_eq!(
        serde_json::to_value(one.get::<game_nutrition::Nutrition>().unwrap()).unwrap(),
        serde_json::to_value(scalar.get::<game_nutrition::Nutrition>().unwrap()).unwrap()
    );
    let mut loaded = builder().load(&four.save().unwrap(), 4).unwrap();
    one.dispatch(batch(&people, store)).unwrap();
    loaded.dispatch(batch(&people, store)).unwrap();
    assert_eq!(one.save().unwrap(), loaded.save().unwrap());
}
#[test]
fn strict_wave_rolls_back_stock_hooks_diets_and_personal_rows() {
    let (mut h, people, store) = fixture();
    let before = h.save().unwrap();
    assert!(h.dispatch(Reject(batch(&people, store))).is_err());
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(Configure(Hooks {
        target: Some(people[5]),
        ..Default::default()
    }))
    .unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(batch(&people, store)).is_err());
    assert_eq!(h.save().unwrap(), before);
}
#[test]
fn overlap_busy_recipient_and_hook_error_require_ordered_fallback() {
    let (mut h, people, store) = fixture();
    let mut overlapping = batch(&people, store);
    overlapping.meals[1].patient = people[0];
    let before = h.save().unwrap();
    assert!(h.dispatch(overlapping).is_err());
    assert_eq!(h.save().unwrap(), before);
    h.dispatch(Rest { person: people[2] }).unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(batch(&people, store)).is_err());
    assert_eq!(h.save().unwrap(), before);
    let (mut h, people, store) = fixture();
    h.dispatch(Configure(Hooks {
        reject: true,
        ..Default::default()
    }))
    .unwrap();
    let before = h.save().unwrap();
    assert!(h.dispatch(batch(&people, store)).is_err());
    assert_eq!(h.save().unwrap(), before);
}
