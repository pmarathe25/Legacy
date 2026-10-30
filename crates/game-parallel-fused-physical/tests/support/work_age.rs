use game_parallel::{CropPhase, StartWork, World};
use game_parallel_fused_physical::Kernel;
fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/work_age.toml")).unwrap()
}
#[test]
fn authored_work_age_applies_to_manual_and_automatic_jobs_across_maturity_and_reload() {
    let state = world();
    let mut kernel = Kernel::compile(state).unwrap();
    let before = kernel.export();
    assert!(
        kernel
            .start_work(&[StartWork {
                id: 1,
                worker: 9,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert_eq!(kernel.export(), before);
    kernel
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let before = kernel.export();
    assert!(kernel.reassign_work(1, 9).is_err());
    assert_eq!(kernel.export(), before);
    kernel.cancel_work(1).unwrap();
    kernel.step().unwrap();
    let mature = kernel.export();
    assert!(mature.production.as_ref().unwrap().jobs.is_empty());
    assert_eq!(
        mature
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|person| person.id == 9)
            .unwrap()
            .age,
        12
    );
    kernel = Kernel::compile(World::from_json(&serde_json::to_string(&mature).unwrap()).unwrap())
        .unwrap();
    kernel.step().unwrap();
    assert_eq!(kernel.export().production.unwrap().jobs[0].worker, 9);
}
#[test]
fn workers_below_the_configured_age_do_not_supply_farm_labor() {
    let mut state = world();
    state.production = None;
    state.autowork = None;
    state.goals = None;
    state.market = None;
    state.goods_cargo = None;
    state.families.as_mut().unwrap().rules.min_work_age = 100;
    state.agriculture = World::from_toml(include_str!("../../../../content/parallel/farming.toml"))
        .unwrap()
        .agriculture;
    let mut kernel = Kernel::compile(state).unwrap();
    kernel.step().unwrap();
    assert!(
        kernel.export().agriculture.unwrap().farms[0]
            .fields
            .iter()
            .all(|field| field.phase == CropPhase::Fallow)
    );
}
#[test]
fn unsupported_hosts_reject_age_rules_and_imported_underage_jobs() {
    let mut state = world();
    let before = state.clone();
    assert!(state.step().is_err());
    assert_eq!(state, before);
    assert!(game_parallel_blocks::Engine::compile(state.clone()).is_err());
    let mut reference = state.clone();
    reference.families.as_mut().unwrap().rules.min_work_age = 0;
    reference
        .start_work(&[StartWork {
            id: 1,
            worker: 9,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    reference.families.as_mut().unwrap().rules.min_work_age = 12;
    assert!(reference.validate().is_err());
}
#[test]
fn editable_work_commands_respect_age_before_mutating() {
    let mut state = world();
    let before = state.clone();
    assert!(
        state
            .start_work(&[StartWork {
                id: 1,
                worker: 9,
                region: 1,
                recipe: 1,
                units: 1
            }])
            .is_err()
    );
    assert_eq!(state, before);
    state
        .start_work(&[StartWork {
            id: 1,
            worker: 1,
            region: 1,
            recipe: 1,
            units: 1,
        }])
        .unwrap();
    let before = state.clone();
    assert!(state.reassign_work(1, 9).is_err());
    assert_eq!(state, before);
}
