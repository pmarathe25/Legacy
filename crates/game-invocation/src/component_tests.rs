use crate::*;
use game_core::*;
use game_ecs::{Batch, Schema, StableId, Store};
use std::collections::BTreeSet;

fn store() -> Store {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<u64>("test/admission", |number| {
            if *number > 10 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    Store::with_schema(schema)
}
fn facts() -> AdmissionFacts {
    AdmissionFacts {
        actor: PersonId(10),
        now: SimTime(0),
        current: Some(PersonId(10)),
        alive: true,
        health: 100_000,
        independent: true,
        learning: 100,
        required_learning: 100,
        fatigue: 0,
        fatigue_limit: 100_000,
        archived: BTreeSet::from([PersonId(11)]),
    }
}
#[test]
fn staged_control_and_foreign_rejection_preserve_components_and_metadata() {
    let mut store = store();
    let mut control = ControlState::new(Rules::fixture()).unwrap();
    let facts = facts();
    let prepared = control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Activate {
                lives: facts.archived.clone(),
            },
            &facts,
            Some(WarningId(1)),
        )
        .unwrap();
    let before = store.clone();
    let before_control = control.clone();
    let mut batch = Batch::new();
    let finish = prepared.stage_into(&control, &store, &mut batch).unwrap();
    batch.insert(StableId(20), 11_u64);
    assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
    assert_eq!(store, before);
    assert_eq!(control, before_control);
    drop(finish);
    control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Activate {
                lives: facts.archived.clone(),
            },
            &facts,
            Some(WarningId(1)),
        )
        .unwrap()
        .commit(&mut control, &mut store)
        .unwrap();
    assert_eq!(control.warning.as_ref().unwrap().id, WarningId(1));
    assert_eq!(warnings(&store).count(), 1);
    assert_eq!(burden(&store, facts.actor), Some(&Burden::default()));
}
#[test]
fn warning_and_burden_rows_guard_prepared_actions_and_restore_without_shadow_maps() {
    let mut store = store();
    let mut control = ControlState::new(Rules::fixture()).unwrap();
    let facts = facts();
    control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Activate {
                lives: facts.archived.clone(),
            },
            &facts,
            Some(WarningId(1)),
        )
        .unwrap()
        .commit(&mut control, &mut store)
        .unwrap();
    let action = ControlAction::AcceptWarning {
        warning: WarningId(1),
    };
    let prepared = control
        .prepare_action(&store, facts.actor, &action, &facts, Some(WarningId(2)))
        .unwrap();
    store
        .get_mut::<InvocationWarning>(StableId(1))
        .unwrap()
        .instant_fatigue += 1;
    let before = store.clone();
    assert_eq!(
        prepared.commit(&mut control, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    control
        .prepare_action(&store, facts.actor, &action, &facts, Some(WarningId(2)))
        .unwrap()
        .commit(&mut control, &mut store)
        .unwrap();
    let prepared = control
        .prepare_advance(&store, SimTime(0), SimTime(10_000), |_| true)
        .unwrap();
    burden_mut(&mut store, facts.actor).unwrap().burden += 1;
    let before = store.clone();
    assert_eq!(
        prepared.commit(&control, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    let json = serde_json::to_value(&control).unwrap();
    assert!(json.get("people").is_none());
    assert!(json.get("warnings").is_none());
    let restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    assert_eq!(store, restored);
    control
        .validate(
            &restored,
            ControlValidation {
                now: SimTime(0),
                next_id: 100,
                current: facts.current,
                incarnations: &BTreeSet::from([facts.actor, PersonId(11)]),
                archived: &facts.archived,
            },
        )
        .unwrap();
}
#[test]
fn context_validation_rejects_orphans_wrong_warning_keys_and_rule_specific_costs() {
    let mut store = store();
    let control = ControlState::new(Rules::fixture()).unwrap();
    insert_burden(&mut store, PersonId(10), Burden::default()).unwrap();
    assert!(validate_absent(&store).is_err());
    let incarnations = BTreeSet::from([PersonId(10), PersonId(11)]);
    let archived = BTreeSet::from([PersonId(11)]);
    let check = |store: &Store| {
        control.validate(
            store,
            ControlValidation {
                now: SimTime(0),
                next_id: 100,
                current: Some(PersonId(10)),
                incarnations: &incarnations,
                archived: &archived,
            },
        )
    };
    assert!(check(&store).is_ok());
    burden_mut(&mut store, PersonId(10)).unwrap().lifespan_loss =
        control.rules.lifespan_loss_per_band.0 * 253;
    assert!(check(&store).is_err());
    burden_mut(&mut store, PersonId(10)).unwrap().lifespan_loss = 0;
    let mut value = warning_spec(
        &control.rules,
        PersonId(10),
        &archived,
        CostRequirement {
            cost: InvocationCost::Strain,
            severity: 0,
            permanent: false,
        },
    )
    .unwrap();
    value.id = WarningId(2);
    store.insert_or_spawn(StableId(3), value).unwrap();
    assert!(check(&store).is_err());
}
#[test]
fn native_cost_progression_is_slice_invariant_and_does_not_touch_dead_people() {
    let mut whole = store();
    insert_burden(
        &mut whole,
        PersonId(10),
        Burden {
            burden: 60_000,
            ..Burden::default()
        },
    )
    .unwrap();
    insert_burden(
        &mut whole,
        PersonId(11),
        Burden {
            burden: 60_000,
            ..Burden::default()
        },
    )
    .unwrap();
    let control = ControlState::new(Rules::fixture()).unwrap();
    let mut split = whole.clone();
    control
        .prepare_advance(&whole, SimTime(0), SimTime(30_000), |id| id == PersonId(10))
        .unwrap()
        .commit(&control, &mut whole)
        .unwrap();
    for (now, next) in [(0, 10001), (10001, 30000)] {
        control
            .prepare_advance(&split, SimTime(now), SimTime(next), |id| id == PersonId(10))
            .unwrap()
            .commit(&control, &mut split)
            .unwrap();
    }
    assert_eq!(whole, split);
    assert_eq!(burden(&whole, PersonId(11)).unwrap().burden, 60_000);
}

#[test]
fn activation_cost_composes_with_physiology_in_one_admitted_batch() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_physiology::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let facts = facts();
    game_physiology::insert_body(
        &mut store,
        facts.actor,
        game_physiology::Body {
            vitals: Vitals {
                hunger: 0,
                fatigue: 0,
                health: facts.health,
                alive: true,
            },
            fatigue_remainder: 0,
            health_remainder: 0,
        },
    )
    .unwrap();
    let mut control = ControlState::new(Rules::fixture()).unwrap();
    control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Activate {
                lives: facts.archived.clone(),
            },
            &facts,
            Some(WarningId(1)),
        )
        .unwrap()
        .commit(&mut control, &mut store)
        .unwrap();
    let prepare = |store: &Store| {
        control
            .prepare_action(
                store,
                facts.actor,
                &ControlAction::AcceptWarning {
                    warning: WarningId(1),
                },
                &facts,
                Some(WarningId(2)),
            )
            .unwrap()
    };
    let plan = prepare(&store);
    let quantity = plan
        .effects
        .iter()
        .find_map(|effect| match effect {
            ControlEffect::FatigueCharge { quantity, .. } => Some(*quantity),
            _ => None,
        })
        .unwrap();
    assert_eq!(quantity, 1_000);
    let before = store.clone();
    let before_control = control.clone();
    assert!(
        game_physiology::prepare_fatigue_charge(&store, facts.actor, quantity, quantity - 1)
            .is_err()
    );
    assert_eq!(control, before_control);
    assert_eq!(store, before);
    let charge =
        game_physiology::prepare_fatigue_charge(&store, facts.actor, quantity, facts.fatigue_limit)
            .unwrap();
    let mut batch = Batch::new();
    let finish = plan.stage_into(&control, &store, &mut batch).unwrap();
    charge.stage_into(&store, &mut batch).unwrap();
    store.apply_batch(batch).unwrap();
    finish.finish(&mut control);
    assert_eq!(
        game_physiology::body(&store, facts.actor)
            .unwrap()
            .vitals
            .fatigue,
        quantity
    );
    assert_eq!(burden(&store, facts.actor).unwrap().burden, 6_000);
    assert!(control.active.is_some());
    assert!(control.warning.is_none());
}
