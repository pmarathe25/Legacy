mod support;
use game_stockpiles::*;
use support::*;
#[test]
fn surplus_moves_to_a_finite_owned_cache_and_preserves_every_unit() {
    let (mut h, p, i, area) = setup();
    deposit(&mut h, i, "cotton", 1990);
    deposit(&mut h, i, "grain", 10);
    assert!(
        h.dispatch(StoreSurplus {
            person: p,
            inventory: i,
            needed_mass: 200
        })
        .unwrap()
    );
    let cache = h.get::<Stockpiles>().unwrap().stores[&p][&area];
    assert_eq!(amount(&h, i, "cotton"), 1790);
    assert_eq!(amount(&h, cache, "cotton"), 200);
    assert_eq!(amount(&h, i, "grain"), 10);
    assert_eq!(
        h.get::<game_cargo::Cargo>().unwrap().0[&cache].capacity,
        8000
    );
    assert!(
        h.get::<game_property::Property>()
            .unwrap()
            .allowed(p, cache)
    );
    assert!(
        h.get::<game_knowledge::Knowledge>().unwrap().0[&p]
            .contains_key(game_knowledge::store_key(cache).as_str())
    );
    builder().load(&h.save().unwrap(), 4).unwrap();
}
#[test]
fn failed_storage_and_unauthorized_caching_roll_back_without_allocating_a_store() {
    let (mut h, p, i, area) = setup();
    deposit(&mut h, i, "cotton", 2000);
    let other = person(&mut h, area);
    let before = h.save().unwrap();
    assert!(
        h.dispatch(StoreSurplus {
            person: other,
            inventory: i,
            needed_mass: 20
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
    assert!(
        h.dispatch(StoreSurplus {
            person: p,
            inventory: i,
            needed_mass: 2001
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
}
#[test]
fn a_full_cache_cannot_create_another_store_or_destroy_stock() {
    let (mut h, p, i, area) = setup();
    deposit(&mut h, i, "cotton", 2000);
    h.dispatch(StoreSurplus {
        person: p,
        inventory: i,
        needed_mass: 200,
    })
    .unwrap();
    let cache = h.get::<Stockpiles>().unwrap().stores[&p][&area];
    deposit(&mut h, cache, "cotton", 7800);
    deposit(&mut h, i, "cotton", 200);
    let before = h.save().unwrap();
    assert!(
        h.dispatch(StoreSurplus {
            person: p,
            inventory: i,
            needed_mass: 10
        })
        .is_err()
    );
    assert_eq!(before, h.save().unwrap());
}
