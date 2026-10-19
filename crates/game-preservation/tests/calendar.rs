use game_core::*;
use game_preservation::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
struct Rate(u32);
impl Default for Rate {
    fn default() -> Self {
        Self(1000)
    }
}
impl Resource for Rate {
    const KEY: &'static str = "test.rate";
}
#[derive(Clone, Serialize, Deserialize)]
struct SetRate {
    inventory: InventoryId,
    rate: u32,
    fail: bool,
}
impl Request for SetRate {
    const KEY: &'static str = "test.set-rate";
    type Response = ();
}
fn builder() -> Builder {
    let mut b = Builder::new();
    game_objects::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_preservation::plugin(&mut b).unwrap();
    b.plugin("test", 1).unwrap();
    b.resource::<Rate>().unwrap();
    b.request::<Conditions>(|tx, _| Ok(tx.get::<Rate>()?.0))
        .unwrap();
    b.request::<SetRate>(|tx, r| {
        tx.edit::<Rate>()?.0 = r.rate;
        tx.call(Refresh {
            inventory: r.inventory,
        })?;
        require(!r.fail, "discard storage change")
    })
    .unwrap();
    b
}
fn world() -> Host {
    let mut h = builder().build(1, 1).unwrap();
    h.dispatch(game_objects::Install(game_objects::standard().unwrap()))
        .unwrap();
    h.dispatch(Enable).unwrap();
    h
}
fn inventory(h: &mut Host, quantity: u64) -> InventoryId {
    let inventory = h
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Area(AreaId(1)),
            capacity: 10000,
        })
        .unwrap();
    h.dispatch(game_cargo::Deposit {
        inventory,
        good: game_cargo::FOOD,
        quantity,
    })
    .unwrap();
    inventory
}
fn run(h: &mut Host, at: SimTime) {
    while h.advance(at, 128).unwrap().stop == Stop::Budget {}
}
fn timers(h: &Host) -> usize {
    let world: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    world["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|j| j["request"]["key"] == "preservation.expire")
        .count()
}
#[test]
fn consumption_retires_only_appointments_without_remaining_food() {
    let mut h = world();
    let first = inventory(&mut h, 20);
    let second = inventory(&mut h, 10);
    let expiry = h.get::<Preservation>().unwrap().batches[&first][&game_cargo::FOOD][0].expires;
    h.dispatch(game_cargo::Consume {
        inventory: first,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    assert_eq!(h.get::<Preservation>().unwrap().due[&expiry].len(), 2);
    h.dispatch(game_cargo::Consume {
        inventory: first,
        good: game_cargo::FOOD,
        quantity: 10,
    })
    .unwrap();
    assert_eq!(h.get::<Preservation>().unwrap().due[&expiry].len(), 1);
    assert_eq!(timers(&h), 1);
    run(&mut h, expiry);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&second].amount(game_cargo::FOOD),
        0
    );
    assert_eq!(h.get::<Preservation>().unwrap().spoiled, 10);
    assert!(h.get::<Preservation>().unwrap().due.is_empty());
    assert_eq!(timers(&h), 0);
}
#[test]
fn temperature_changes_keep_one_live_expiry_and_rollback_failed_rescheduling() {
    let mut h = world();
    let inv = inventory(&mut h, 20);
    let original = h.get::<Preservation>().unwrap().batches[&inv][&game_cargo::FOOD][0].expires;
    for step in 1..=100 {
        run(&mut h, SimTime(step * MINUTE.0));
        h.dispatch(SetRate {
            inventory: inv,
            rate: if step % 2 == 0 { 1000 } else { 8000 },
            fail: false,
        })
        .unwrap();
        assert_eq!(h.get::<Preservation>().unwrap().due.len(), 1);
        assert_eq!(timers(&h), 1);
    }
    let before = h.save().unwrap();
    assert!(
        h.dispatch(SetRate {
            inventory: inv,
            rate: 8000,
            fail: true
        })
        .is_err()
    );
    assert_eq!(h.save().unwrap(), before);
    let mut loaded = builder().load(&before, 4).unwrap();
    let expiry = h.get::<Preservation>().unwrap().batches[&inv][&game_cargo::FOOD][0].expires;
    assert!(expiry >= original);
    for host in [&mut h, &mut loaded] {
        run(host, expiry);
    }
    assert_eq!(h.save().unwrap(), loaded.save().unwrap());
    assert_eq!(h.get::<Preservation>().unwrap().spoiled, 20);
}
#[test]
fn transferring_the_last_portion_removes_the_empty_source_from_the_calendar() {
    let mut h = world();
    let first = inventory(&mut h, 20);
    let second = inventory(&mut h, 1);
    let expiry = h.get::<Preservation>().unwrap().batches[&first][&game_cargo::FOOD][0].expires;
    h.dispatch(game_cargo::Transfer {
        from: first,
        to: second,
        good: game_cargo::FOOD,
        quantity: 20,
    })
    .unwrap();
    assert_eq!(h.get::<Preservation>().unwrap().due[&expiry].len(), 1);
    assert_eq!(timers(&h), 1);
    run(&mut h, expiry);
    assert_eq!(h.get::<Preservation>().unwrap().spoiled, 21);
}
#[test]
fn a_shared_expiry_larger_than_the_work_batch_finishes_without_lost_food() {
    let mut h = world();
    let mut expiry = SimTime(0);
    for _ in 0..140 {
        let inv = inventory(&mut h, 1);
        expiry = h.get::<Preservation>().unwrap().batches[&inv][&game_cargo::FOOD][0].expires;
    }
    assert_eq!(timers(&h), 1);
    run(&mut h, expiry);
    assert_eq!(h.get::<Preservation>().unwrap().spoiled, 140);
    assert!(h.get::<Preservation>().unwrap().due.is_empty());
    assert_eq!(timers(&h), 0);
}
#[test]
fn legacy_calendars_are_rebuilt_without_changing_food_and_failed_migration_rolls_back() {
    let mut h = world();
    let inv = inventory(&mut h, 20);
    let mut saved: serde_json::Value = serde_json::from_str(&h.save().unwrap()).unwrap();
    let state = saved["resources"][Preservation::KEY]
        .as_object_mut()
        .unwrap();
    state.remove("timers");
    state.remove("indexed");
    state.get_mut("due").unwrap()[SimTime(100 * DAY.0).0.to_string()] =
        serde_json::json!([[inv.0, game_cargo::FOOD.0]]);
    let id = saved["next_id"].as_u64().unwrap();
    saved["next_id"] = (id + 1).into();
    saved["jobs"].as_array_mut().unwrap().push(serde_json::json!({"id":id,"at":100*DAY.0,"priority":5,"request":{"key":"preservation.expire","payload":null}}));
    let mut old = builder().load(&saved.to_string(), 1).unwrap();
    assert_eq!(timers(&old), 2);
    let before = old.save().unwrap();
    assert!(
        old.dispatch(SetRate {
            inventory: inv,
            rate: 8000,
            fail: true
        })
        .is_err()
    );
    assert_eq!(old.save().unwrap(), before);
    let food = old.get::<Preservation>().unwrap().batches.clone();
    old.dispatch(Refresh { inventory: inv }).unwrap();
    assert_eq!(old.get::<Preservation>().unwrap().batches, food);
    assert_eq!(old.get::<Preservation>().unwrap().due.len(), 1);
    assert_eq!(timers(&old), 1);
    let saved = old.save().unwrap();
    assert_eq!(builder().load(&saved, 4).unwrap().save().unwrap(), saved);
}
