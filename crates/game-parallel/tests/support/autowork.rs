use game_parallel::{Amount, CropPhase, StartWork, World};
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/autowork.toml")).unwrap()
}
fn quantity(w: &World, good: u32) -> u64 {
    w.production.as_ref().unwrap().inventories[0]
        .stocks
        .iter()
        .find(|s| s.good == good)
        .unwrap()
        .quantity
}
#[test]
fn idle_people_reserve_inputs_spend_setup_interval_and_finish_later() {
    let mut w = world();
    w.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    w.step().unwrap();
    assert_eq!(quantity(&w, 1), 3);
    assert_eq!(w.autowork.as_ref().unwrap().next_id, 102);
    assert!(
        w.production
            .as_ref()
            .unwrap()
            .jobs
            .iter()
            .all(|job| job.remaining == 2)
    );
    assert!(
        w.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Fallow)
    );
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.step().unwrap();
    assert_eq!(quantity(&w, 2), 0);
    w.step().unwrap();
    assert_eq!(quantity(&w, 2), 4);
    assert!(w.production.as_ref().unwrap().jobs.is_empty());
    w.step().unwrap();
    assert_eq!(w.autowork.as_ref().unwrap().next_id, 103);
    assert_eq!(quantity(&w, 1), 0);
}
#[test]
fn limited_inputs_grant_only_the_lowest_person_without_spending_others_labor() {
    let mut w = world();
    w.production.as_mut().unwrap().inventories[0].stocks[0].quantity = 3;
    w.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    w.step().unwrap();
    let jobs = &w.production.as_ref().unwrap().jobs;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].worker, 1);
    assert_eq!(quantity(&w, 1), 0);
    assert_eq!(
        w.agriculture.as_ref().unwrap().farms[0]
            .fields
            .iter()
            .filter(|f| f.phase == CropPhase::Growing)
            .count(),
        1
    );
}
#[test]
fn output_and_new_input_reservations_compose_without_overwriting_stock() {
    let mut w = world();
    w.autowork
        .as_mut()
        .unwrap()
        .policies
        .retain(|p| p.person == 2);
    let recipe = &mut w.production.as_mut().unwrap().recipes[0];
    recipe.outputs = vec![Amount {
        good: 1,
        quantity: 4,
    }];
    w.start_work(&[StartWork {
        id: 1,
        worker: 1,
        region: 1,
        recipe: 1,
        units: 1,
    }])
    .unwrap();
    w.autowork.as_mut().unwrap().policies[0].enabled = false;
    w.step().unwrap();
    w.autowork.as_mut().unwrap().policies[0].enabled = true;
    w.step().unwrap();
    assert_eq!(quantity(&w, 1), 7);
    assert_eq!(w.production.as_ref().unwrap().jobs[0].worker, 2);
}
#[test]
fn job_id_exhaustion_and_late_family_failure_publish_no_reservations() {
    let mut w = world();
    w.autowork.as_mut().unwrap().next_id = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
    let family =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.regions = family.regions;
    w.routes = family.routes;
    w.families = family.families;
    w.autowork.as_mut().unwrap().next_id = 100;
    w.families
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|m| m.id == 3)
        .unwrap()
        .age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}
#[test]
fn a_worker_who_dies_during_setup_leaves_reserved_goods_in_the_job() {
    let mut w = world();
    w.autowork
        .as_mut()
        .unwrap()
        .policies
        .retain(|p| p.person == 1);
    w.rules.food_growth = 0;
    w.rules.hunger_per_tick = 1;
    w.rules.starvation_threshold = 1;
    w.rules.starvation_damage = 100;
    w.regions[0].food = 0;
    w.step().unwrap();
    assert_eq!(w.regions[0].people[0].health, 0);
    assert_eq!(quantity(&w, 1), 6);
    let job = &w.production.as_ref().unwrap().jobs[0];
    assert_eq!(job.worker, 1);
    assert_eq!(job.reserved[0].quantity, 3);
    let before = job.remaining;
    w.step().unwrap();
    assert_eq!(w.production.as_ref().unwrap().jobs[0].remaining, before);
}

#[test]
fn multi_input_claims_are_atomic_even_when_conservative_priority_blocks_later_work() {
    use game_parallel::Recipe;
    let mut w = world();
    let production = w.production.as_mut().unwrap();
    production.inventories[0].stocks[0].quantity = 3;
    production.recipes.push(Recipe {
        id: 2,
        name: "combine".into(),
        ticks: 1,
        inputs: vec![
            Amount {
                good: 1,
                quantity: 3,
            },
            Amount {
                good: 2,
                quantity: 1,
            },
        ],
        outputs: vec![Amount {
            good: 2,
            quantity: 1,
        }],
    });
    w.autowork.as_mut().unwrap().policies[0].recipe = 2;
    w.step().unwrap();
    assert!(w.production.as_ref().unwrap().jobs.is_empty());
    assert_eq!(quantity(&w, 1), 3);
    assert_eq!(quantity(&w, 2), 0);
}
#[test]
fn duplicate_worker_policies_are_rejected_without_mutation() {
    let mut w = world();
    let duplicate = w.autowork.as_ref().unwrap().policies[0].clone();
    w.autowork.as_mut().unwrap().policies.push(duplicate);
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn concentrated_autonomous_reservations_are_worker_and_layout_independent() {
    use game_parallel::{Person, WorkPolicy};
    let mut initial = world();
    let count = 5_000u64;
    initial.regions[0].people = (100..100 + count)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    initial.autowork.as_mut().unwrap().policies = (100..100 + count)
        .map(|id| WorkPolicy {
            person: id,
            recipe: 1,
            enabled: true,
            disregard_claims: false,
        })
        .collect();
    let stock = &mut initial.production.as_mut().unwrap().inventories[0].stocks[0];
    stock.capacity = count * 3;
    stock.quantity = (count / 2) * 3;
    let mut results = Vec::new();
    for workers in [1, 4, 8] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = initial.clone();
        if workers > 1 {
            w.regions[0].people.reverse();
            w.autowork.as_mut().unwrap().policies.reverse();
        }
        pool.install(|| w.step()).unwrap();
        w.regions[0].people.sort_unstable_by_key(|p| p.id);
        w.autowork
            .as_mut()
            .unwrap()
            .policies
            .sort_unstable_by_key(|p| p.person);
        assert_eq!(
            w.production.as_ref().unwrap().jobs.len(),
            count as usize / 2
        );
        assert!(
            w.production
                .as_ref()
                .unwrap()
                .jobs
                .iter()
                .all(|job| job.worker < 100 + count / 2)
        );
        assert_eq!(quantity(&w, 1), 0);
        w.validate().unwrap();
        results.push(w);
    }
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn homogeneous_multi_input_grants_the_intersection_of_resource_prefixes() {
    let mut state = world();
    let production = state.production.as_mut().unwrap();
    production.recipes[0].inputs.push(Amount {
        good: 2,
        quantity: 1,
    });
    production.inventories[0].stocks[1].quantity = 1;
    state.autowork.as_mut().unwrap().policies.reverse();
    state.step().unwrap();
    let production = state.production.as_ref().unwrap();
    assert_eq!(production.jobs.len(), 1);
    assert_eq!(production.jobs[0].worker, 1);
    assert_eq!(quantity(&state, 1), 6);
    assert_eq!(quantity(&state, 2), 0);
}
