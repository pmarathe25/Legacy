use game_core::*;
use game_food_memory::with_stores;
use game_knowledge::{Fact, FactValue, Facts, StoreLocation};
use std::sync::Arc;

fn insert(facts: &mut Facts, key: &str, value: FactValue) {
    facts.insert(
        key.into(),
        Fact {
            value: Arc::new(value),
            source: PersonId(1),
            observed: SimTime(0),
            revision: 0,
            confidence: 100,
        },
    );
}
fn view(
    person: PersonId,
    facts: &Facts,
    objects: &game_objects::Objects,
) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    with_stores(person, facts, objects, |s| {
        (
            s.all().map(|s| s.id.0).collect(),
            s.meals().map(|s| s.id.0).collect(),
            s.carried(person).map(|s| s.id.0).collect(),
        )
    })
}
#[test]
fn changes_aliases_world_switches_and_rolled_back_evidence_keep_the_same_possibilities() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let person = PersonId(1);
    let mut facts = Facts::default();
    insert(
        &mut facts,
        "store:4",
        FactValue::Stock([(grain, 20)].into()),
    );
    insert(
        &mut facts,
        "store:04",
        FactValue::Stock([(grain, 0)].into()),
    );
    insert(
        &mut facts,
        "store-location:4",
        FactValue::Location(StoreLocation {
            area: AreaId(2),
            capacity: 100,
            mass: 20,
            carrier: Some(person),
        }),
    );
    assert_eq!(
        view(person, &facts, &objects),
        (vec![4, 4], vec![4, 4], vec![4, 4])
    );
    with_stores(person, &facts, &objects, |s| {
        assert_eq!(
            s.among([InventoryId(99), InventoryId(4)]),
            vec![InventoryId(4), InventoryId(4)]
        );
    });
    let original = facts.clone();
    insert(&mut facts, "store:4", FactValue::Stock([(grain, 0)].into()));
    insert(
        &mut facts,
        "store-location:4",
        FactValue::Location(StoreLocation {
            area: AreaId(3),
            capacity: 100,
            mass: 0,
            carrier: None,
        }),
    );
    assert_eq!(view(person, &facts, &objects), (vec![4, 4], vec![], vec![]));
    // Restoring a transaction snapshot must also restore its derived possibilities.
    assert_eq!(
        view(person, &original, &objects),
        (vec![4, 4], vec![4, 4], vec![4, 4])
    );
    with_stores(person, &facts, &objects, |s| {
        assert_eq!(
            s.all().next().unwrap().location.as_ref().unwrap().area,
            AreaId(3)
        );
        // Nested actor queries must not retain a mutable cache borrow.
        assert_eq!(view(PersonId(2), &original, &objects).1, vec![4, 4]);
        assert_eq!(view(person, &original, &objects).1, vec![4, 4]);
    });
    let empty = Facts::default();
    assert_eq!(view(person, &empty, &objects), (vec![], vec![], vec![]));
    assert_eq!(view(person, &original, &objects).1, vec![4, 4]);
    let mut wire = serde_json::to_value(&objects).unwrap();
    wire["items"]["grain"]["food"] = serde_json::Value::Null;
    let inedible = serde_json::from_value(wire).unwrap();
    assert!(view(person, &original, &inedible).1.is_empty());
    assert_eq!(view(person, &original, &objects).1, vec![4, 4]);
}
#[test]
fn an_alias_without_canonical_stock_cannot_invent_food_or_location() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let mut facts = Facts::default();
    insert(
        &mut facts,
        "store:+4",
        FactValue::Stock([(grain, 20)].into()),
    );
    insert(
        &mut facts,
        "store-location:+4",
        FactValue::Location(StoreLocation {
            area: AreaId(2),
            capacity: 100,
            mass: 20,
            carrier: Some(PersonId(1)),
        }),
    );
    assert_eq!(
        view(PersonId(1), &facts, &objects),
        (vec![4], vec![], vec![])
    );
    insert(
        &mut facts,
        "store:4",
        FactValue::Stock([(grain, 20)].into()),
    );
    assert_eq!(view(PersonId(1), &facts, &objects).1, vec![4, 4]);
    assert!(view(PersonId(1), &facts, &objects).2.is_empty());
}
#[test]
fn evicting_inactive_character_indexes_does_not_delete_their_evidence() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let mut original = Facts::default();
    insert(
        &mut original,
        "store:4",
        FactValue::Stock([(grain, 20)].into()),
    );
    assert_eq!(view(PersonId(1), &original, &objects).1, vec![4]);
    let empty = Facts::default();
    for id in 2..1100 {
        assert!(view(PersonId(id), &empty, &objects).0.is_empty());
    }
    assert_eq!(view(PersonId(1), &original, &objects).1, vec![4]);
    assert!(original.contains_key("store:4"));
}

#[test]
fn numeric_stock_views_follow_personal_evidence_across_independent_forks() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let mut original = Facts::default();
    insert(
        &mut original,
        "store:4",
        FactValue::Stock([(grain, 10)].into()),
    );
    let mut a = original.clone();
    let mut b = original.clone();
    insert(&mut a, "store:4", FactValue::Stock([(grain, 20)].into()));
    insert(&mut b, "store:4", FactValue::Stock([(grain, 30)].into()));
    for (facts, amount) in [(&original, 10), (&a, 20), (&b, 30), (&original, 10)] {
        game_food_memory::with_stores(PersonId(1), facts, &objects, |s| {
            assert_eq!(
                s.get(InventoryId(4)).unwrap().stock.as_ref().unwrap()[&grain],
                amount
            );
        });
    }
    let loaded: Facts = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    game_food_memory::with_stores(PersonId(1), &loaded, &objects, |s| {
        assert_eq!(
            s.get(InventoryId(4)).unwrap().stock.as_ref().unwrap()[&grain],
            20
        );
    });
}

#[test]
fn supply_projections_do_not_retain_unrelated_personal_history() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let person = PersonId(7001);
    let mut facts = Facts::default();
    let history = Arc::new(FactValue::Other(
        serde_json::json!({"private": "x".repeat(100_000)}),
    ));
    let weak = Arc::downgrade(&history);
    facts.insert(
        "history:private".into(),
        Fact {
            value: history,
            source: person,
            observed: SimTime(0),
            revision: 0,
            confidence: 100,
        },
    );
    insert(
        &mut facts,
        "store:4",
        FactValue::Stock([(grain, 20)].into()),
    );
    insert(
        &mut facts,
        "store-location:4",
        FactValue::Location(StoreLocation {
            area: AreaId(2),
            capacity: 100,
            mass: 20,
            carrier: Some(person),
        }),
    );
    assert_eq!(view(person, &facts, &objects), (vec![4], vec![4], vec![4]));
    drop(facts);
    assert!(
        weak.upgrade().is_none(),
        "an inventory index retained unrelated history"
    );
    game_food_memory::retain_people(std::iter::empty());
}

#[test]
fn large_supplier_maps_keep_structural_updates_aliases_and_rollback_correct() {
    let objects = game_objects::standard().unwrap();
    let grain = objects.resolve("grain").unwrap();
    let person = PersonId(7002);
    let mut facts = Facts::default();
    for id in 1..=600 {
        insert(
            &mut facts,
            &format!("store:{id}"),
            FactValue::Stock([(grain, 20)].into()),
        );
        insert(
            &mut facts,
            &format!("store-location:{id}"),
            FactValue::Location(StoreLocation {
                area: AreaId(2),
                capacity: 100,
                mass: 20,
                carrier: Some(person),
            }),
        );
    }
    assert_eq!(view(person, &facts, &objects).1.len(), 600);
    let original = facts.clone();
    insert(&mut facts, "store:2", FactValue::Stock([(grain, 0)].into()));
    assert_eq!(view(person, &facts, &objects).1.len(), 599);
    assert_eq!(view(person, &original, &objects).1.len(), 600);
    let mut alias = original.clone();
    insert(
        &mut alias,
        "store:02",
        FactValue::Stock([(grain, 0)].into()),
    );
    assert_eq!(view(person, &alias, &objects).1.len(), 601);
    assert_eq!(
        view(person, &Facts::default(), &objects),
        (vec![], vec![], vec![])
    );
    assert_eq!(view(person, &original, &objects).1.len(), 600);
    game_food_memory::retain_people(std::iter::empty());
}
#[test]
fn concurrent_worlds_and_nested_queries_keep_independent_immutable_store_images() {
    let objects = Arc::new(game_objects::standard().unwrap());
    let grain = objects.resolve("grain").unwrap();
    let mut threads = vec![];
    for index in 0..4 {
        let objects = objects.clone();
        threads.push(std::thread::spawn(move || {
            let mut facts = Facts::default();
            insert(
                &mut facts,
                "store:4",
                FactValue::Stock([(grain, index + 1)].into()),
            );
            for _ in 0..100 {
                with_stores(PersonId(1), &facts, &objects, |outer| {
                    assert_eq!(
                        outer.get(InventoryId(4)).unwrap().stock.as_ref().unwrap()[&grain],
                        index + 1
                    );
                    let mut other = facts.clone();
                    insert(
                        &mut other,
                        "store:4",
                        FactValue::Stock([(grain, index + 100)].into()),
                    );
                    with_stores(PersonId(1), &other, &objects, |inner| {
                        assert_eq!(
                            inner.get(InventoryId(4)).unwrap().stock.as_ref().unwrap()[&grain],
                            index + 100
                        );
                    });
                    assert_eq!(
                        outer.get(InventoryId(4)).unwrap().stock.as_ref().unwrap()[&grain],
                        index + 1
                    );
                });
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }
}
