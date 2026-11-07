use game_autowork::{Autowork, Policy};
use game_production::{Amount, Inventory, Production, Recipe, Stock};

#[test]
fn ordered_and_unordered_busy_people_exclude_the_same_workers() {
    let people = (1..=4).map(|id| (id, Some(1), 100, 0)).collect::<Vec<_>>();
    let production = Production {
        max_work_hunger: 20,
        goods: vec![],
        recipes: vec![Recipe {
            id: 1,
            name: "tool".into(),
            ticks: 1,
            inputs: vec![Amount {
                good: 1,
                quantity: 1,
            }],
            outputs: vec![Amount {
                good: 2,
                quantity: 1,
            }],
        }],
        recipe_categories: vec![],
        inventories: vec![Inventory {
            region: 1,
            stocks: vec![
                Stock {
                    good: 1,
                    quantity: 10,
                    capacity: 10,
                },
                Stock {
                    good: 2,
                    quantity: 0,
                    capacity: 10,
                },
            ],
        }],
        jobs: vec![],
        events: vec![],
    };
    let auto = Autowork {
        next_id: 1,
        policies: (1..=4)
            .map(|person| Policy {
                person,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            })
            .collect(),
        requests: None,
    };
    let ordered = auto
        .plan_validated(&people, &production, &[2, 4], None)
        .unwrap()
        .unwrap();
    let shuffled = auto
        .plan_validated(&people, &production, &[4, 2, 2], None)
        .unwrap()
        .unwrap();
    assert_eq!(ordered.actors, vec![1, 3]);
    assert_eq!(ordered.actors, shuffled.actors);
    assert_eq!(ordered.requests, shuffled.requests);
    assert_eq!(ordered.next_id, shuffled.next_id);
}
