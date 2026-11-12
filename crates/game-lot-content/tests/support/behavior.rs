use super::*;
use game_production_model::{FoodProfile, Stock};
use std::collections::BTreeMap;

fn goods() -> Vec<Good> {
    vec![
        Good {
            id: 1,
            name: "fresh food".into(),
            food: Some(FoodProfile {
                portion: 1,
                hunger_relief: 1,
                nutrients: BTreeMap::new(),
                health_effect: 0,
                shelf_days: Some(2),
            }),
        },
        Good {
            id: 2,
            name: "stone".into(),
            food: None,
        },
    ]
}

#[test]
fn personal_gathering_outputs_are_fresh_and_require_unique_actors() {
    let catalog = Catalog::compile(&goods(), 10).unwrap();
    let output = Gathered {
        action: 1,
        person: 2,
        good: 1,
        quantity: 1,
    };
    let rows = prepare_gathered(&catalog, &[output], 7).unwrap();
    assert_eq!(rows[0].creations[0].to, Place::Person(2));
    assert_eq!(rows[0].creations[0].lot.expires_at, Some(27));
    assert!(
        prepare_gathered(
            &catalog,
            &[
                output,
                Gathered {
                    action: 2,
                    ..output
                }
            ],
            7
        )
        .is_err()
    );
    assert!(
        prepare_gathered(
            &catalog,
            &[
                output,
                Gathered {
                    person: 3,
                    ..output
                }
            ],
            7
        )
        .is_err()
    );
}
#[test]
fn fresh_goods_use_authored_life_and_stock_capacity() {
    let catalog = Catalog::compile(&goods(), 10).unwrap();
    let inventories = vec![Inventory {
        region: 8,
        stocks: vec![
            Stock {
                good: 1,
                quantity: 3,
                capacity: 5,
            },
            Stock {
                good: 2,
                quantity: 2,
                capacity: 4,
            },
        ],
    }];
    let state = catalog.fresh_world(&inventories, 7).unwrap();
    assert_eq!(state.containers[0].lots[0].expires_at, Some(27));
    assert_eq!(state.containers[1].lots[0].expires_at, None);
    assert_eq!(
        catalog
            .creation(1, Place::JobOutput(2), 1, 4, 9)
            .unwrap()
            .lot
            .expires_at,
        Some(29)
    );
    assert!(
        catalog
            .creation(1, Place::JobOutput(2), 1, 4, u64::MAX)
            .is_err()
    );
    assert!(
        catalog
            .creation(1, Place::Stock { region: 8, good: 2 }, 1, 4, 9)
            .is_err()
    );
    assert!(catalog.fresh(3, 1, 0).is_err());
}

#[test]
fn imported_food_shelf_life_follows_the_physical_goods() {
    use game_lots::{Movement, Store};
    use game_production::Production;
    use std::path::Path;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../content");
    let mut production = Production {
        max_work_hunger: 1,
        goods: vec![],
        recipes: vec![],
        recipe_categories: vec![],
        inventories: vec![Inventory {
            region: 1,
            stocks: vec![],
        }],
        jobs: vec![],
        events: vec![],
    };
    game_content_import::augment(
        &mut production,
        &root.join("objects"),
        &root.join("workshop"),
        600_000,
        10_000,
        1,
    )
    .unwrap();
    let good = production
        .goods
        .iter()
        .find(|good| {
            good.food
                .as_ref()
                .and_then(|food| food.shelf_days)
                .is_some()
        })
        .unwrap();
    let id = good.id;
    let shelf = u64::from(good.food.as_ref().unwrap().shelf_days.unwrap());
    production.inventories[0]
        .stocks
        .iter_mut()
        .find(|stock| stock.good == id)
        .unwrap()
        .quantity = 3;
    let catalog = Catalog::from_tick_millis(&production.goods, 86_400_000).unwrap();
    let mut state = catalog.fresh_world(&production.inventories, 0).unwrap();
    let person = Place::Person(1);
    state
        .containers
        .push(Container::new(person, 3, vec![]).unwrap());
    state.containers.sort_unstable_by_key(|row| row.place);
    let mut store = Store::compile(state).unwrap();
    if shelf > 1 {
        let draft = store
            .prepare(
                1,
                &[Movement {
                    batch: None,
                    item: None,
                    id: 1,
                    from: Place::Stock {
                        region: 1,
                        good: id,
                    },
                    to: Some(person),
                    good: id,
                    quantity: 2,
                }],
            )
            .unwrap();
        let (moves, _) = store.commit(draft).unwrap();
        assert!(moves[0].accepted);
        assert_eq!(moves[0].lots[0].expires_at, Some(shelf));
    }
    while store.export().tick < shelf {
        let draft = store.prepare(1, &[]).unwrap();
        let (_, spoiled) = store.commit(draft).unwrap();
        if !spoiled.is_empty() {
            assert_eq!(store.export().tick, shelf);
            assert_eq!(spoiled.iter().map(|row| row.quantity).sum::<u64>(), 3);
        }
    }
    assert!(store.export().containers.iter().all(|row| row.used == 0));
}

#[test]
fn fractional_days_round_expiry_up_and_overflow_is_rejected() {
    let catalog = Catalog::from_tick_millis(&goods(), 7 * 60 * 60 * 1000).unwrap();
    assert_eq!(catalog.fresh(1, 1, 7).unwrap().expires_at, Some(14));
    assert!(Catalog::from_tick_millis(&goods(), 0).is_err());
    assert!(Catalog::compile(&goods(), u64::MAX).is_err());
}

#[test]
fn one_harvest_can_create_fresh_tagged_cohorts_only_in_its_region() {
    let catalog = Catalog::compile(&goods(), 10).unwrap();
    let mut seed = catalog
        .creation(2, Place::Stock { region: 1, good: 2 }, 2, 3, 7)
        .unwrap();
    seed.lot.batch = Some(42);
    let mut action = Transaction {
        id: 1,
        movements: vec![],
        creations: vec![
            catalog
                .creation(1, Place::Stock { region: 1, good: 1 }, 1, 4, 7)
                .unwrap(),
            seed,
        ],
    };
    assert!(validate_growth(&catalog, &[action.clone()], 7).is_ok());
    action.creations[1].to = Place::Stock { region: 2, good: 2 };
    assert!(validate_growth(&catalog, &[action.clone()], 7).is_err());
    action.creations[1].to = Place::Stock { region: 1, good: 2 };
    action.creations[0].lot.expires_at = Some(99);
    assert!(validate_growth(&catalog, &[action], 7).is_err());
}
