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
    let control = ControlState::new(Rules::fixture()).unwrap();
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
    prepared.stage_into(&control, &store, &mut batch).unwrap();
    batch.insert(StableId(20), 11_u64);
    assert_eq!(store.apply_batch(batch), Err(Error::Unavailable));
    assert_eq!(store, before);
    assert_eq!(control, before_control);
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
        .commit(&control, &mut store)
        .unwrap();
    assert_eq!(control.pending_consent(&store).unwrap().id, WarningId(1));
    assert_eq!(warnings(&store).count(), 1);
    assert_eq!(burden(&store, facts.actor), Some(&Burden::default()));
}
#[test]
fn warning_and_burden_rows_guard_prepared_actions_and_restore_without_shadow_maps() {
    let mut store = store();
    let control = ControlState::new(Rules::fixture()).unwrap();
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
        .commit(&control, &mut store)
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
        prepared.commit(&control, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    control
        .prepare_action(&store, facts.actor, &action, &facts, Some(WarningId(2)))
        .unwrap()
        .commit(&control, &mut store)
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
    let control = ControlState::new(Rules::fixture()).unwrap();
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
        .commit(&control, &mut store)
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
    plan.stage_into(&control, &store, &mut batch).unwrap();
    charge.stage_into(&store, &mut batch).unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(
        game_physiology::body(&store, facts.actor)
            .unwrap()
            .vitals
            .fatigue,
        quantity
    );
    assert_eq!(burden(&store, facts.actor).unwrap().burden, 6_000);
    assert!(control.active(&store).is_some());
    assert!(control.pending_consent(&store).is_none());
}

#[test]
fn session_is_native_unique_and_guards_its_opening_row() {
    let mut store = store();
    let control = ControlState::new(Rules::fixture()).unwrap();
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
        .commit(&control, &mut store)
        .unwrap();
    assert_eq!(sessions(&store).count(), 1);
    let row = session(&store, facts.actor).unwrap().clone();
    let prepared = control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Dismiss,
            &facts,
            Some(WarningId(2)),
        )
        .unwrap();
    session_mut(&mut store, facts.actor)
        .unwrap()
        .warning
        .as_mut()
        .unwrap()
        .activation = None;
    let before = store.clone();
    assert!(prepared.commit(&control, &mut store).is_err());
    assert_eq!(store, before);
    *session_mut(&mut store, facts.actor).unwrap() = row.clone();
    let second = PersonId(12);
    let mut other = row.clone();
    other.actor = second;
    other.activation.as_mut().unwrap().actor = second;
    store.insert_or_spawn(StableId(second.0), other).unwrap();
    assert!(
        control
            .prepare_action(
                &store,
                facts.actor,
                &ControlAction::Dismiss,
                &facts,
                Some(WarningId(2))
            )
            .is_err()
    );
    store.remove::<ControlSession>(StableId(second.0)).unwrap();
    let prepared = control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Dismiss,
            &facts,
            Some(WarningId(2)),
        )
        .unwrap();
    let before = store.clone();
    let mut batch = Batch::new();
    prepared.stage_into(&control, &store, &mut batch).unwrap();
    batch.insert(StableId(facts.actor.0), row);
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store, before);
    control
        .prepare_action(
            &store,
            facts.actor,
            &ControlAction::Dismiss,
            &facts,
            Some(WarningId(2)),
        )
        .unwrap()
        .commit(&control, &mut store)
        .unwrap();
    assert!(session(&store, facts.actor).is_none());
    assert!(burden(&store, facts.actor).is_some());
    assert!(warning(&store, WarningId(1)).is_some());
    assert_eq!(
        serde_json::to_value(&control)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn combined_session_cost_and_lifespan_response_reject_foreign_body_or_age_edits() {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    game_physiology::register_components(&mut schema).unwrap();
    game_lifecycle::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    let control = ControlState::new(Rules::fixture()).unwrap();
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
    game_lifecycle::insert_life(
        &mut store,
        game_lifecycle::Life {
            id: facts.actor,
            born: facts.now,
            initial_age: SimDuration(0),
            stage: LifeStage::Infant,
            reproduction: game_lifecycle::Reproduction::Neither,
            parents: vec![],
            caregivers: BTreeSet::new(),
            preference: None,
        },
    )
    .unwrap();
    let lifecycle = game_lifecycle::Runtime {
        rules: game_lifecycle::Rules::human(),
    };
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
        .commit(&control, &mut store)
        .unwrap();
    for age_edit in [false, true] {
        let plan = control
            .prepare_action(
                &store,
                facts.actor,
                &ControlAction::AcceptWarning {
                    warning: WarningId(1),
                },
                &facts,
                Some(WarningId(2)),
            )
            .unwrap();
        let before = store.clone();
        let effects = plan.effects.clone();
        let mut next_id = 2_u64;
        let mut published = Vec::<ControlEffect>::new();
        let life_facts = game_lifecycle::LifespanFacts {
            actor: facts.actor,
            now: facts.now,
            alive: true,
            lifespan_loss: plan.lifespan_loss_after(&store, facts.actor),
        };
        let lifespan = lifecycle
            .prepare_lifespan_change(&store, life_facts)
            .unwrap();
        let survival = game_physiology::prepare_survival(&store, facts.actor).unwrap();
        let mut batch = Batch::new();
        let receipt = plan.stage_into(&control, &store, &mut batch).unwrap();
        let fatigue = game_physiology::prepare_fatigue_charge(
            &store,
            facts.actor,
            1_000,
            facts.fatigue_limit,
        )
        .unwrap()
        .stage_into_with_receipt(&store, &mut batch)
        .unwrap();
        if age_edit {
            batch.remove::<game_lifecycle::Life>(StableId(facts.actor.0));
            assert!(
                lifespan
                    .check(&lifecycle, &store, life_facts, &batch)
                    .is_err()
            );
        } else {
            batch.remove::<game_physiology::Body>(StableId(facts.actor.0));
        }
        let publication = (|| -> Result<(), Error> {
            receipt.validate_batch(&control, &store, &batch)?;
            survival.check(&store, &batch, Some(&fatigue))?;
            lifespan.check(&lifecycle, &store, life_facts, &batch)?;
            store.apply_batch(batch)?;
            next_id += 1;
            published.extend(effects);
            Ok(())
        })();
        assert!(publication.is_err());
        assert_eq!(next_id, 2);
        assert!(published.is_empty());
        assert_eq!(store, before);
        assert_eq!(control.pending_consent(&store).unwrap().id, WarningId(1));
        assert!(control.active(&store).is_none());
        assert_eq!(warnings(&store).count(), 1);
        assert_eq!(burden(&store, facts.actor).unwrap().burden, 0);
    }
}

#[test]
fn final_control_receipt_rejects_pending_warning_burden_and_second_session_collisions() {
    let mut store = store();
    let control = ControlState::new(Rules::fixture()).unwrap();
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
        .commit(&control, &mut store)
        .unwrap();
    for attack in 0..3 {
        let prepared = control
            .prepare_review(&store, &facts, Some(WarningId(2)))
            .unwrap();
        let before = store.clone();
        let mut batch = Batch::new();
        let receipt = prepared.stage_into(&control, &store, &mut batch).unwrap();
        match attack {
            0 => {
                batch.remove::<InvocationWarning>(StableId(1));
            }
            1 => {
                batch.remove::<Burden>(StableId(facts.actor.0));
            }
            _ => {
                let mut other = session(&store, facts.actor).unwrap().clone();
                other.actor = PersonId(12);
                other.activation.as_mut().unwrap().actor = other.actor;
                batch.insert(StableId(other.actor.0), other);
            }
        }
        assert!(receipt.validate_batch(&control, &store, &batch).is_err());
        assert_eq!(store, before);
        assert_eq!(warnings(&store).count(), 1);
        assert_eq!(sessions(&store).count(), 1);
    }
    let prepared = control
        .prepare_review(&store, &facts, Some(WarningId(2)))
        .unwrap();
    let mut batch = Batch::new();
    let receipt = prepared.stage_into(&control, &store, &mut batch).unwrap();
    batch.insert(StableId(13), Burden::default());
    receipt.validate_batch(&control, &store, &batch).unwrap();
    store.apply_batch(batch).unwrap();
}

#[test]
fn new_warning_identity_rejects_canonical_occupancy_and_late_foreign_component_shape() {
    let mut occupied = store();
    let control = ControlState::new(Rules::fixture()).unwrap();
    let facts = facts();
    let prepare = |store: &Store| {
        control
            .prepare_action(
                store,
                facts.actor,
                &ControlAction::Activate {
                    lives: facts.archived.clone(),
                },
                &facts,
                Some(WarningId(1)),
            )
            .unwrap()
    };
    let pending = prepare(&occupied);
    occupied.insert_or_spawn(StableId(1), 5_u64).unwrap();
    let before = occupied.clone();
    assert!(pending.commit(&control, &mut occupied).is_err());
    assert_eq!(occupied, before);
    let clean = store();
    let mut batch = Batch::new();
    let receipt = prepare(&clean)
        .stage_into(&control, &clean, &mut batch)
        .unwrap();
    batch.insert(StableId(1), 5_u64);
    let before = clean.clone();
    assert!(receipt.validate_batch(&control, &clean, &batch).is_err());
    assert_eq!(clean, before);
    assert!(control.pending_consent(&clean).is_none());
}
