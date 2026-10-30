use crate::PackedJobs;
use game_parallel_production::{
    Amount, Good, Inventory, Job, JobPhase, PersonSnapshot, Production, Stock,
};
fn fixture(capacity: u64) -> (Production, Vec<PersonSnapshot>) {
    let count = 9_000;
    let jobs = (0..count)
        .map(|id| Job {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            remaining: 1 + id % 2,
            phase: JobPhase::Working,
            reserved: vec![Amount {
                good: 1,
                quantity: 3,
            }]
            .into(),
            outputs: vec![
                Amount {
                    good: 2,
                    quantity: 1 + id % 3,
                },
                Amount {
                    good: 3,
                    quantity: 1,
                },
            ]
            .into(),
        })
        .collect();
    let people = (0..count)
        .map(|id| (id + 100, Some(1), 100, if id % 11 == 0 { 20 } else { 0 }))
        .collect();
    let production = Production {
        max_work_hunger: 10,
        goods: (1..=3)
            .map(|id| Good {
                id,
                name: format!("good{id}"),
                food: None,
            })
            .collect(),
        recipes: vec![],
        recipe_categories: vec![],
        inventories: vec![Inventory {
            region: 1,
            stocks: vec![
                Stock {
                    good: 1,
                    quantity: 0,
                    capacity,
                },
                Stock {
                    good: 2,
                    quantity: 0,
                    capacity,
                },
                Stock {
                    good: 3,
                    quantity: 0,
                    capacity,
                },
            ],
        }],
        jobs,
        events: vec![],
    };
    (production, people)
}
#[test]
fn packed_ticks_match_exact_jobs_events_and_inventory_for_abundant_and_contested_outputs() {
    for capacity in [100_000, 5_000] {
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let (mut original, people) = fixture(capacity);
            let mut packed = PackedJobs::from_jobs(&original.jobs);
            let mut inventory = original.inventories.clone();
            assert_eq!(packed.jobs(), original.jobs);
            pool.install(|| {
                for tick in 1..=4 {
                    let draft = original.prepare_validated(people.clone(), tick);
                    let packed_draft =
                        packed.prepare(&people, &inventory, original.max_work_hunger, tick);
                    draft.commit(&mut original);
                    packed_draft.commit(&mut packed, &mut inventory);
                    assert_eq!(packed.jobs(), original.jobs);
                    assert_eq!(inventory, original.inventories);
                    assert_eq!(packed.events(), original.events);
                }
            });
        }
    }
}

#[test]
fn direct_block_admission_matches_atomic_serialized_admission() {
    use game_parallel_production::{Recipe, Start};
    use game_parallel_production_admission::Context;
    let count = 9_000;
    let (_, people) = fixture(100_000);
    let recipes = vec![Recipe {
        id: 1,
        name: "boards".into(),
        ticks: 2,
        inputs: vec![Amount {
            good: 1,
            quantity: 3,
        }],
        outputs: vec![
            Amount {
                good: 2,
                quantity: 2,
            },
            Amount {
                good: 3,
                quantity: 1,
            },
        ],
    }];
    let inventories = vec![Inventory {
        region: 1,
        stocks: vec![
            Stock {
                good: 1,
                quantity: count * 6,
                capacity: count * 6,
            },
            Stock {
                good: 2,
                quantity: 0,
                capacity: count * 4,
            },
            Stock {
                good: 3,
                quantity: 0,
                capacity: count * 2,
            },
        ],
    }];
    let requests: Vec<_> = (0..count)
        .rev()
        .map(|id| Start {
            id,
            worker: id + 100,
            region: 1,
            recipe: 1,
            units: 1 + (id % 2) as u32,
        })
        .collect();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut normal_jobs = Vec::new();
        let mut normal_inventory = inventories.clone();
        let mut packed = PackedJobs::from_jobs(&[]);
        let mut packed_inventory = inventories.clone();
        pool.install(|| {
            let normal = Context {
                jobs: &normal_jobs,
                recipes: &recipes,
                inventories: &normal_inventory,
                people: &people,
            }
            .prepare(&requests)
            .unwrap();
            let direct = packed
                .prepare_start(&recipes, &packed_inventory, &people, &requests)
                .unwrap();
            normal.commit(&mut normal_jobs, &mut normal_inventory);
            direct.commit(&mut packed, &mut packed_inventory);
        });
        assert_eq!(packed.jobs(), normal_jobs);
        assert_eq!(packed_inventory, normal_inventory);
        let before = packed.jobs();
        assert!(
            packed
                .prepare_start(&recipes, &packed_inventory, &people, &requests)
                .is_err()
        );
        assert_eq!(packed.jobs(), before);
        let last = Start {
            id: count + 1,
            worker: 100,
            region: 1,
            recipe: 1,
            units: 1,
        };
        assert!(
            packed
                .prepare_start(&recipes, &packed_inventory, &people, &[last])
                .is_err()
        );
    }
    let insufficient = vec![Inventory {
        region: 1,
        stocks: vec![
            Stock {
                good: 1,
                quantity: 1,
                capacity: 1,
            },
            Stock {
                good: 2,
                quantity: 0,
                capacity: count * 4,
            },
            Stock {
                good: 3,
                quantity: 0,
                capacity: count * 2,
            },
        ],
    }];
    let packed = PackedJobs::from_jobs(&[]);
    assert!(
        packed
            .prepare_start(&recipes, &insufficient, &people, &requests)
            .is_err()
    );
    assert!(packed.is_empty());
}

#[test]
fn block_prefix_arbitration_matches_multi_region_multi_output_priorities() {
    let (mut original, mut people) = fixture(4_000);
    original.jobs.reverse();
    original.inventories = (1..=3)
        .map(|region| Inventory {
            region,
            stocks: vec![
                Stock {
                    good: 1,
                    quantity: 0,
                    capacity: 8_000,
                },
                Stock {
                    good: 2,
                    quantity: 0,
                    capacity: 1_000 + region * 100,
                },
                Stock {
                    good: 3,
                    quantity: 0,
                    capacity: 900 + region * 50,
                },
            ],
        })
        .collect();
    for job in &mut original.jobs {
        job.region = 1 + job.id % 3;
        job.remaining = 1;
        if job.id % 5 == 0 {
            job.outputs.truncate(1);
        }
    }
    for person in &mut people {
        person.1 = Some(1 + (person.0 - 100) % 3);
    }
    let mut packed = PackedJobs::from_jobs(&original.jobs);
    let mut stocks = original.inventories.clone();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        let draft = original.prepare_validated(people.clone(), 1);
        let staged = packed.prepare(&people, &stocks, original.max_work_hunger, 1);
        draft.commit(&mut original);
        staged.commit(&mut packed, &mut stocks);
    });
    assert_eq!(packed.jobs(), original.jobs);
    assert_eq!(stocks, original.inventories);
    assert_eq!(packed.events(), original.events);
}

#[test]
fn sparse_material_reservations_debit_only_requested_stocks() {
    use std::collections::HashMap;
    let mut inventories: Vec<_> = (1..=3)
        .map(|region| Inventory {
            region,
            stocks: (1..=128)
                .map(|good| Stock {
                    good,
                    quantity: 20,
                    capacity: 100,
                })
                .collect(),
        })
        .collect();
    let needs = HashMap::from([((1, 1), 2), ((1, 128), 5), ((3, 64), 7)]);
    crate::apply_needs(&mut inventories, &needs);
    for inventory in inventories {
        for stock in inventory.stocks {
            assert_eq!(
                stock.quantity,
                20 - needs
                    .get(&(inventory.region, stock.good))
                    .copied()
                    .unwrap_or(0) as u64
            );
        }
    }
}
