use game_parallel::{CropPhase, World};
use game_parallel_blocks::Engine;
use game_parallel_fused_physical::Kernel;

fn world() -> World {
    World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers.toml"
    ))
    .unwrap()
}
#[test]
fn only_volunteers_work_and_all_hosts_agree() {
    let mut reference = world();
    let mut block = Engine::compile(reference.clone()).unwrap();
    let mut fused = Kernel::compile(reference.clone()).unwrap();
    reference.step().unwrap();
    block.step().unwrap();
    fused.step().unwrap();
    assert_eq!(reference, block.export());
    assert_eq!(reference, fused.export());
    let fields = &reference.agriculture.as_ref().unwrap().farms[0].fields;
    assert_eq!(
        fields
            .iter()
            .filter(|field| field.phase == CropPhase::Growing)
            .count(),
        1
    );
}
#[test]
fn withdrawing_and_volunteering_change_future_work_and_invalidate_drafts() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world()).unwrap();
                let draft = kernel.prepare().unwrap();
                kernel.set_farm_volunteer(1, false).unwrap();
                assert!(kernel.commit(draft).is_err());
                kernel.step().unwrap();
                assert!(
                    kernel.export().agriculture.unwrap().farms[0]
                        .fields
                        .iter()
                        .all(|f| f.phase == CropPhase::Fallow)
                );
                kernel.set_farm_volunteer(2, true).unwrap();
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&kernel.export()).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.agriculture.as_ref().unwrap().farms[0]
                        .fields
                        .iter()
                        .filter(|f| f.phase == CropPhase::Growing)
                        .count(),
                    1
                );
                result.validate().unwrap();
                result
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn invalid_and_duplicate_volunteers_are_rejected() {
    let mut state = world();
    state.agriculture.as_mut().unwrap().volunteers = Some(vec![1, 1]);
    assert!(state.validate().is_err());
    state.agriculture.as_mut().unwrap().volunteers = Some(vec![999]);
    assert!(state.validate().is_err());
}
#[test]
fn concentrated_volunteer_labor_is_worker_invariant() {
    use game_parallel::{Field, Person};
    let mut state = world();
    state.rules.food_capacity = 100_000;
    state.regions[0].food = 100_000;
    state.regions[0].people = (1..=10_000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    let agriculture = state.agriculture.as_mut().unwrap();
    agriculture.volunteers = Some((1..=5_000).collect());
    agriculture.farms[0].fields = (1..=6_000)
        .map(|id| Field {
            id,
            phase: CropPhase::Fallow,
            work: 0,
        })
        .collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                assert_eq!(
                    result.agriculture.as_ref().unwrap().farms[0]
                        .fields
                        .iter()
                        .filter(|field| field.phase == CropPhase::Growing)
                        .count(),
                    5_000
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn residents_choose_to_volunteer_and_an_explicit_refusal_persists() {
    let state = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap();
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    let mut editable = state.clone();
    assert!(editable.step().is_err());
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.set_farm_volunteer(1, false).unwrap();
                let before = kernel.export();
                let draft = kernel.prepare().unwrap();
                assert_eq!(kernel.export(), before);
                kernel.commit(draft).unwrap();
                let first = kernel.export();
                let farm = first.agriculture.as_ref().unwrap();
                assert_eq!(farm.volunteers.as_deref(), Some(&[2][..]));
                assert_eq!(farm.refusals, vec![1]);
                assert!(
                    farm.farms[0]
                        .fields
                        .iter()
                        .all(|field| field.phase == CropPhase::Fallow)
                );
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&first).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let second = kernel.export();
                assert_eq!(
                    second.agriculture.as_ref().unwrap().volunteers.as_deref(),
                    Some(&[2][..])
                );
                assert_eq!(
                    second.agriculture.as_ref().unwrap().farms[0]
                        .fields
                        .iter()
                        .filter(|field| field.phase == CropPhase::Growing)
                        .count(),
                    1
                );
                kernel.set_farm_volunteer(1, true).unwrap();
                let final_state = kernel.export();
                assert!(
                    final_state
                        .agriculture
                        .as_ref()
                        .unwrap()
                        .refusals
                        .is_empty()
                );
                final_state.validate().unwrap();
                final_state
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn invalid_automatic_volunteer_rules_and_conflicting_refusals_are_rejected() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap();
    state
        .agriculture
        .as_mut()
        .unwrap()
        .recruitment
        .as_mut()
        .unwrap()
        .one_in = 0;
    assert!(state.validate().is_err());
    state
        .agriculture
        .as_mut()
        .unwrap()
        .recruitment
        .as_mut()
        .unwrap()
        .one_in = 1;
    state.agriculture.as_mut().unwrap().volunteers = Some(vec![1]);
    state.agriculture.as_mut().unwrap().refusals = vec![1];
    assert!(state.validate().is_err());
}
#[test]
fn social_participants_do_not_volunteer_in_the_same_tick() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap();
    state.autonomy = None;
    state.regions[0].food = 10;
    state.agriculture = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap()
    .agriculture;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(
        result
            .social
            .as_ref()
            .unwrap()
            .events
            .iter()
            .any(|event| (event.a, event.b) == (1, 2))
    );
    assert!(
        result
            .agriculture
            .as_ref()
            .unwrap()
            .volunteers
            .as_ref()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn failed_lifecycle_tick_does_not_publish_volunteering() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    state.agriculture = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap()
    .agriculture;
    state.families.as_mut().unwrap().members[0].age = u64::MAX;
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}
#[test]
fn automatic_volunteering_is_deterministic_for_concentrated_populations() {
    use game_parallel::Person;
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap();
    state.rules.food_capacity = 100_000;
    state.regions[0].food = 100_000;
    state.regions[0].people = (1..=20_000)
        .map(|id| Person {
            id,
            health: 100,
            hunger: 0,
        })
        .collect();
    state.regions[1].people.clear();
    state
        .agriculture
        .as_mut()
        .unwrap()
        .recruitment
        .as_mut()
        .unwrap()
        .one_in = 3;
    state
        .agriculture
        .as_mut()
        .unwrap()
        .recruitment
        .as_mut()
        .unwrap()
        .leave_one_in = 7;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                for _ in 0..3 {
                    kernel.step().unwrap();
                }
                let state = kernel.export();
                let count = state
                    .agriculture
                    .as_ref()
                    .unwrap()
                    .volunteers
                    .as_ref()
                    .unwrap()
                    .len();
                assert!(count > 5_000 && count < 20_000);
                state.validate().unwrap();
                state
            })
    };
    assert_eq!(run(1), run(4));
}
#[test]
fn recruitment_respects_minimum_work_age_and_player_control() {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap();
    state.agriculture = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap()
    .agriculture;
    let families = state.families.as_mut().unwrap();
    families.rules.min_work_age = 12;
    families
        .members
        .iter_mut()
        .find(|member| member.id == 2)
        .unwrap()
        .age = 11;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    let first = kernel.export();
    let volunteers = first
        .agriculture
        .as_ref()
        .unwrap()
        .volunteers
        .as_ref()
        .unwrap();
    assert!(!volunteers.contains(&1));
    assert!(!volunteers.contains(&2));
    assert!(volunteers.contains(&3) && volunteers.contains(&4));
    kernel.step().unwrap();
    let second = kernel.export();
    let volunteers = second
        .agriculture
        .as_ref()
        .unwrap()
        .volunteers
        .as_ref()
        .unwrap();
    assert!(!volunteers.contains(&1));
    assert!(volunteers.contains(&2));
}
#[test]
fn volunteer_reviews_follow_the_authored_interval() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap();
    state
        .agriculture
        .as_mut()
        .unwrap()
        .recruitment
        .as_mut()
        .unwrap()
        .interval = 2;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert!(
        kernel
            .export()
            .agriculture
            .unwrap()
            .volunteers
            .unwrap()
            .is_empty()
    );
    kernel.step().unwrap();
    assert_eq!(
        kernel.export().agriculture.unwrap().volunteers.unwrap(),
        vec![1, 2]
    );
    kernel.step().unwrap();
    assert!(
        kernel.export().agriculture.unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Growing)
    );
}
#[test]
fn opted_in_residents_can_leave_and_reconsider_without_creating_an_explicit_refusal() {
    let mut state = World::from_toml(include_str!(
        "../../../../content/parallel/farming_volunteers_auto.toml"
    ))
    .unwrap();
    state
        .agriculture
        .as_mut()
        .unwrap()
        .recruitment
        .as_mut()
        .unwrap()
        .leave_one_in = 1;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let joined = kernel.export();
                assert_eq!(
                    joined.agriculture.as_ref().unwrap().volunteers.as_deref(),
                    Some(&[1, 2][..])
                );
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&joined).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                let left = kernel.export();
                let farm = left.agriculture.as_ref().unwrap();
                assert!(farm.volunteers.as_ref().unwrap().is_empty());
                assert!(farm.refusals.is_empty());
                assert!(
                    farm.farms[0]
                        .fields
                        .iter()
                        .all(|field| field.phase == CropPhase::Fallow)
                );
                kernel.step().unwrap();
                let rejoined = kernel.export();
                assert_eq!(
                    rejoined.agriculture.as_ref().unwrap().volunteers.as_deref(),
                    Some(&[1, 2][..])
                );
                rejoined.validate().unwrap();
                rejoined
            })
    };
    assert_eq!(run(1), run(4));
}
