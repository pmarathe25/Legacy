#[path = "support/activity_epochs.rs"]
mod activity_epochs;
#[path = "support/continuation.rs"]
mod continuation;
#[path = "support/failure_learning.rs"]
mod failure_learning;
#[path = "support/option_memory.rs"]
mod option_memory;
#[path = "support/review_grid.rs"]
mod review_grid;
#[path = "support/review_tiers.rs"]
mod review_tiers;
mod support;
#[path = "support/utility.rs"]
mod utility;
use game_characters::*;
use game_core::*;
use support::*;

#[test]
fn stale_stock_causes_failure_learning_and_backoff() {
    let mut f = setup();
    f.host
        .dispatch(game_lineage::Begin { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.store,
            allow: true,
        })
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.other,
            inventory: f.store,
        })
        .unwrap();
    f.host
        .dispatch(Register {
            person: f.other,
            mind: Mind::default(),
        })
        .unwrap();
    f.host
        .dispatch(game_cargo::Consume {
            inventory: f.store,
            good: game_cargo::FOOD,
            quantity: 1000,
        })
        .unwrap();
    f.host.dispatch(game_physiology::Start).unwrap();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 75));
    let m = &f.host.get::<Minds>().unwrap().people[&f.other];
    assert_eq!(m.failures, 1);
    assert_eq!(m.retry_after.len(), 1);
    assert!(
        m.retry_after
            .values()
            .all(|at| *at > f.host.now()
                && *at <= f.host.now().after(Rules::default().retry).unwrap())
    );
    run(&mut f.host, SimTime(MINUTE.0 * 100));
    assert_eq!(f.host.get::<Minds>().unwrap().people[&f.other].failures, 1);
}

#[test]
fn fixture_state_roundtrips_with_only_required_plugins() {
    let f = setup();
    let save = f.host.save().unwrap();
    let loaded = builder().load(&save, 2).unwrap();
    assert_eq!(save, loaded.save().unwrap());
}
#[test]
fn learned_values_change_choices_with_physical_options_held_constant() {
    let mut f = setup();
    f.host
        .dispatch(game_lineage::Begin { person: f.ruler })
        .unwrap();
    f.host
        .dispatch(game_property::Permit {
            issuer: f.ruler,
            person: f.other,
            inventory: f.store,
            allow: true,
        })
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.other,
            inventory: f.store,
        })
        .unwrap();
    let pack = f
        .host
        .dispatch(game_cargo::Create {
            custody: game_cargo::Custody::Carried(f.other),
            capacity: 100,
        })
        .unwrap();
    let patch = f
        .host
        .dispatch(game_ecology::Create(game_ecology::Patch {
            area: f.home,
            good: game_cargo::FOOD,
            stock: 100,
            capacity: 100,
            renew: 0,
        }))
        .unwrap();
    f.host
        .dispatch(game_property::ClaimInventory {
            claimant: f.other,
            inventory: pack,
            recognized_by: std::collections::BTreeSet::from([f.other]),
        })
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObserveStore {
            person: f.other,
            inventory: pack,
        })
        .unwrap();
    f.host
        .dispatch(game_knowledge::ObservePatch {
            person: f.other,
            patch,
        })
        .unwrap();
    f.host
        .dispatch(Register {
            person: f.other,
            mind: Mind::default(),
        })
        .unwrap();
    f.host.dispatch(game_physiology::Start).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 31));
    let mut learned = builder().load(&f.host.save().unwrap(), 2).unwrap();
    for _ in 0..8 {
        learned
            .dispatch(game_relationships::LearnValue {
                person: f.other,
                concept: "independence".into(),
                outcome: 5,
            })
            .unwrap();
    }
    f.host.dispatch(Start).unwrap();
    learned.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(MINUTE.0 * 32));
    run(&mut learned, SimTime(MINUTE.0 * 32));
    assert_eq!(food(&f.host, f.store), 990);
    assert_eq!(food(&learned, f.store), 1000);
    assert_eq!(
        learned.get::<game_sim::Actions>().unwrap().0[&f.other].kind,
        "gather"
    );
    assert!(
        learned.get::<Minds>().unwrap().people[&f.other]
            .last
            .as_ref()
            .unwrap()
            .reasons[0]
            .starts_with("food:")
    );
}

#[test]
fn an_early_action_cancellation_prompts_a_parked_mind_with_unmet_needs() {
    let mut f = setup();
    f.host
        .dispatch(Register {
            person: f.guardian,
            mind: Mind::default(),
        })
        .unwrap();
    f.host.dispatch(game_physiology::Start).unwrap();
    f.host.dispatch(Start).unwrap();
    run(&mut f.host, SimTime(11 * MINUTE.0));
    assert_eq!(
        f.host.get::<game_sim::Actions>().unwrap().0[&f.guardian].until,
        Some(SimTime(71 * MINUTE.0))
    );
    f.host
        .dispatch(game_physiology::Damage {
            person: f.guardian,
            amount: 100,
        })
        .unwrap();
    f.host
        .dispatch(game_physiology::Heal {
            person: f.guardian,
            amount: 100,
        })
        .unwrap();
    let mut loaded = builder().load(&f.host.save().unwrap(), 4).unwrap();
    for host in [&mut f.host, &mut loaded] {
        run(host, SimTime(12 * MINUTE.0));
    }
    assert_eq!(
        f.host.get::<game_sim::Actions>().unwrap().0[&f.guardian].until,
        Some(SimTime(71 * MINUTE.0))
    );
    assert_eq!(f.host.save().unwrap(), loaded.save().unwrap());
}
