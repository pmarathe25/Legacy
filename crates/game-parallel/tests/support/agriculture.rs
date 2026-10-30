use game_parallel::{CropPhase, World};

fn world() -> World {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/farming.toml")).unwrap();
    world.rules.food_growth = 0;
    world.rules.hunger_per_tick = 0;
    world.regions[0].people[1].health = 0;
    world
}

#[test]
fn farm_uses_seed_and_labor_and_waits_for_storage() {
    let mut w = world();
    w.step().unwrap();
    assert_eq!(w.regions[0].food, 8);
    assert_eq!(
        w.agriculture.as_ref().unwrap().farms[0].fields[0].phase,
        CropPhase::Growing
    );
    assert_eq!(
        w.agriculture.as_ref().unwrap().farms[0].fields[1].phase,
        CropPhase::Fallow
    );
    w.step().unwrap();
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.step().unwrap();
    assert_eq!(
        w.agriculture.as_ref().unwrap().farms[0].fields[0].phase,
        CropPhase::Ripe
    );
    w.regions[0].food = 96;
    w.agriculture.as_mut().unwrap().farms[0].fields.truncate(1);
    w.step().unwrap();
    assert_eq!(w.regions[0].food, 96);
    assert_eq!(
        w.agriculture.as_ref().unwrap().farms[0].fields[0].phase,
        CropPhase::Ripe
    );
    w.regions[0].food = 90;
    w.step().unwrap();
    assert_eq!(w.regions[0].food, 96);
    assert_eq!(
        w.agriculture.as_ref().unwrap().farms[0].fields[0].phase,
        CropPhase::Fallow
    );
}

#[test]
fn farm_work_is_layout_and_worker_count_independent() {
    let mut a = world();
    let mut b = a.clone();
    b.agriculture.as_mut().unwrap().farms[0].fields.reverse();
    b.regions.reverse();
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let four = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    one.install(|| {
        for _ in 0..20 {
            a.step().unwrap();
        }
    });
    four.install(|| {
        for _ in 0..20 {
            b.step().unwrap();
        }
    });
    b.regions.reverse();
    assert_eq!(a, b);
}

#[test]
fn invalid_farms_are_rejected_by_loaders() {
    let mut w = world();
    w.agriculture.as_mut().unwrap().farms[0].fields[0].work = 1;
    assert!(w.validate().is_err());
    assert!(World::from_json(&serde_json::to_string(&w).unwrap()).is_err());
}

#[test]
fn failed_family_step_rolls_back_farming() {
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.agriculture = world().agriculture;
    w.families.as_mut().unwrap().members[0].age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn invalid_farm_step_does_not_change_world() {
    let mut w = world();
    w.agriculture.as_mut().unwrap().farms[0].region = 999;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn blocked_field_does_not_prevent_other_work() {
    let mut w = world();
    let fields = &mut w.agriculture.as_mut().unwrap().farms[0].fields;
    fields[0].phase = CropPhase::Ripe;
    fields[1].phase = CropPhase::Growing;
    w.regions[0].food = 100;
    w.step().unwrap();
    let fields = &w.agriculture.as_ref().unwrap().farms[0].fields;
    assert_eq!(fields[0].phase, CropPhase::Ripe);
    assert_eq!(fields[1].work, 1);
}

#[test]
fn large_farm_matches_snapshot_reference_across_workers() {
    use game_parallel::{Field, Person};
    for food in [0, 25, 5000, 9975, 10000] {
        let mut original = world();
        original.rules.food_capacity = 10000;
        original.regions[0].food = food;
        original.regions[0].people = (1..=700)
            .map(|id| Person {
                id,
                health: 100,
                hunger: 0,
            })
            .collect();
        let farm = &mut original.agriculture.as_mut().unwrap().farms[0];
        farm.fields = (0..6000)
            .rev()
            .map(|id| Field {
                id,
                phase: match id % 3 {
                    0 => CropPhase::Fallow,
                    1 => CropPhase::Growing,
                    _ => CropPhase::Ripe,
                },
                work: 0,
            })
            .collect();
        let rules = original.agriculture.as_ref().unwrap().rules.clone();
        let expected = farm_reference(
            &original.agriculture.as_ref().unwrap().farms[0].fields,
            &rules,
            food,
            10000,
            700,
        );
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut actual = original.clone();
            pool.install(|| actual.step().unwrap());
            assert_eq!(actual.regions[0].food, expected.0);
            assert_eq!(
                actual.agriculture.take().unwrap().farms[0].fields,
                expected.1
            );
        }
    }
}

fn farm_reference(
    fields: &[game_parallel::Field],
    rules: &game_parallel::FarmRules,
    food: u32,
    capacity: u32,
    workers: usize,
) -> (u32, Vec<game_parallel::Field>) {
    let mut fields = fields.to_vec();
    fields.sort_unstable_by_key(|f| f.id);
    let mut seeds = food / rules.seed_food;
    let mut space = capacity.saturating_sub(food) / rules.harvest_food;
    let mut work = 0;
    let mut planted = 0;
    let mut harvested = 0;
    for field in &mut fields {
        let granted = match field.phase {
            CropPhase::Fallow if seeds > 0 => {
                seeds -= 1;
                true
            }
            CropPhase::Ripe if space > 0 => {
                space -= 1;
                true
            }
            CropPhase::Growing => true,
            _ => false,
        };
        if !granted {
            continue;
        }
        if work >= workers {
            continue;
        }
        work += 1;
        match field.phase {
            CropPhase::Fallow => {
                field.phase = CropPhase::Growing;
                planted += 1;
            }
            CropPhase::Ripe => {
                field.phase = CropPhase::Fallow;
                harvested += 1;
            }
            CropPhase::Growing => {
                field.work += 1;
                if field.work == rules.growing_work {
                    field.work = 0;
                    field.phase = CropPhase::Ripe;
                }
            }
        }
    }
    (
        food - planted * rules.seed_food + harvested * rules.harvest_food,
        fields,
    )
}

#[test]
fn seed_and_storage_claims_use_the_same_pre_tick_snapshot() {
    let mut w = world();
    w.regions[0].people[1].health = 100;
    w.regions[0].food = 100;
    let fields = &mut w.agriculture.as_mut().unwrap().farms[0].fields;
    fields[0].phase = CropPhase::Fallow;
    fields[1].phase = CropPhase::Ripe;
    w.step().unwrap();
    assert_eq!(w.regions[0].food, 98);
    let fields = &w.agriculture.as_ref().unwrap().farms[0].fields;
    assert_eq!(fields[0].phase, CropPhase::Growing);
    assert_eq!(fields[1].phase, CropPhase::Ripe);
    w.regions[0].food = 94;
    w.step().unwrap();
    assert_eq!(w.regions[0].food, 100);
}

#[test]
fn duplicate_field_ids_across_regions_are_rejected() {
    let mut w = world();
    let farm = &w.agriculture.as_ref().unwrap().farms[0];
    let mut other = farm.clone();
    other.region = 2;
    w.agriculture.as_mut().unwrap().farms.push(other);
    assert!(w.validate().is_err());
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}
