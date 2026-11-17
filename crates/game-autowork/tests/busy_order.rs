use game_autowork::{Autowork, Policy};
use game_production::{Amount, Inventory, Production, Recipe, Stock};

#[test]
fn multiple_recipes_are_offered_but_an_unselected_plan_does_not_choose() {
    let people = vec![(1, Some(1), 100, 0), (2, Some(1), 100, 0)];
    let recipe = |id| Recipe {
        id,
        name: format!("recipe-{id}"),
        ticks: 1,
        inputs: vec![Amount {
            good: 1,
            quantity: 1,
        }],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    };
    let production = Production {
        max_work_hunger: 20,
        goods: vec![],
        recipes: vec![recipe(1), recipe(2)],
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
    let policy = |person, recipe| Policy {
        person,
        recipe,
        enabled: true,
        disregard_claims: false,
    };
    let auto = Autowork {
        next_id: 1,
        policies: vec![policy(2, 1), policy(1, 2), policy(1, 1)],
        requests: None,
    };
    let expected = vec![(1, 1), (1, 2), (2, 1)];
    for workers in [1, 4] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let opportunities = auto.opportunities(&people, &production, &[], None).unwrap();
                assert_eq!(
                    opportunities
                        .iter()
                        .map(|row| (row.person, row.recipe))
                        .collect::<Vec<_>>(),
                    expected
                );
                let plan = auto.plan(&people, &production, &[], None).unwrap().unwrap();
                assert_eq!(plan.actors, vec![2]);
                assert_eq!(plan.requests[0].recipe, 1);
                let mut duplicate = auto.clone();
                duplicate.policies.push(policy(1, 2));
                assert!(duplicate.validate(&people, &production).is_err());
            });
    }
}

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

    // A scarce stock does not erase anyone's personal opportunity before
    // physical admission; only one later start can obtain the material.
    let mut scarce = production.clone();
    scarce.inventories[0].stocks[0].quantity = 1;
    let opportunities = auto
        .opportunities(&people, &scarce, &[4, 2, 2], None)
        .unwrap();
    assert_eq!(
        opportunities
            .iter()
            .map(|row| row.person)
            .collect::<Vec<_>>(),
        vec![1, 3]
    );
    let admitted = auto
        .plan_validated(&people, &scarce, &[2, 4], None)
        .unwrap()
        .unwrap();
    assert_eq!(admitted.actors, vec![1]);
}

#[test]
#[ignore = "manual work opportunity scaling benchmark"]
fn concentrated_work_opportunity_benchmark() {
    let count = 100_000;
    let people: Vec<_> = (1..=count).map(|id| (id, Some(1), 100, 0)).collect();
    let auto = Autowork {
        next_id: 1,
        policies: (1..=count)
            .map(|person| Policy {
                person,
                recipe: 1,
                enabled: true,
                disregard_claims: false,
            })
            .collect(),
        requests: None,
    };
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
                    quantity: 1,
                    capacity: count,
                },
                Stock {
                    good: 2,
                    quantity: 0,
                    capacity: count,
                },
            ],
        }],
        jobs: vec![],
        events: vec![],
    };
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let start = std::time::Instant::now();
            let rows = pool
                .install(|| auto.opportunities(&people, &production, &[], None))
                .unwrap();
            times.push(start.elapsed());
            assert_eq!(rows.len(), count as usize);
            if let Some(expected) = &reference {
                assert_eq!(&rows, expected);
            } else {
                reference = Some(rows);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} work opportunities",
            times[1]
        );
    }
}
