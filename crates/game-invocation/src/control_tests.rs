use crate::*;
fn control_store() -> game_ecs::Store {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_ecs::Store::with_schema(schema)
}
use game_core::*;
use std::collections::BTreeSet;
const ACTOR: PersonId = PersonId(10);
const OLD: PersonId = PersonId(11);
fn facts() -> AdmissionFacts {
    AdmissionFacts {
        actor: ACTOR,
        now: SimTime(0),
        current: Some(ACTOR),
        alive: true,
        health: 100_000,
        independent: true,
        learning: 100,
        required_learning: 100,
        fatigue: 0,
        fatigue_limit: 100_000,
        archived: BTreeSet::from([OLD, PersonId(12)]),
    }
}
fn execute(
    store: &mut game_ecs::Store,
    state: &ControlState,
    f: &mut AdmissionFacts,
    action: ControlAction,
    id: u64,
) -> Vec<ControlEffect> {
    let plan = state
        .prepare_action(store, ACTOR, &action, f, Some(WarningId(id)))
        .unwrap();
    let effects = plan.effects.clone();
    plan.commit(state, store).unwrap();
    for effect in &effects {
        if let ControlEffect::FatigueCharge { quantity, .. } = effect {
            f.fatigue += quantity;
        }
    }
    effects
}
fn start(
    store: &mut game_ecs::Store,
    state: &ControlState,
    f: &mut AdmissionFacts,
    lives: BTreeSet<PersonId>,
) -> u64 {
    execute(store, state, f, ControlAction::Activate { lives }, 1);
    let mut next = 2;
    while let Some(w) = state.pending_consent(store).cloned() {
        execute(
            store,
            state,
            f,
            ControlAction::AcceptWarning { warning: w.id },
            next,
        );
        next += 1;
    }
    next
}
#[test]
fn learned_person_accepts_the_exact_activation_charge_before_any_borrowed_access() {
    let state = ControlState::new(Rules::fixture()).unwrap();
    let mut store = control_store();
    let mut f = facts();
    assert!(
        state
            .prepare_action(
                &store,
                ACTOR,
                &ControlAction::Activate {
                    lives: BTreeSet::from([OLD])
                },
                &AdmissionFacts {
                    learning: 99,
                    ..f.clone()
                },
                Some(WarningId(1))
            )
            .is_err()
    );
    assert!(
        state
            .prepare_action(
                &store,
                ACTOR,
                &ControlAction::Activate {
                    lives: BTreeSet::from([OLD])
                },
                &AdmissionFacts {
                    fatigue: 100_000,
                    ..f.clone()
                },
                Some(WarningId(1))
            )
            .is_err()
    );
    execute(
        &mut store,
        &state,
        &mut f,
        ControlAction::Activate {
            lives: BTreeSet::from([OLD]),
        },
        1,
    );
    assert!(state.active(&store).is_none());
    assert_eq!(state.pending_warning(&store).unwrap().instant_fatigue, 1000);
    assert_eq!(f.fatigue, 0);
    let effects = execute(
        &mut store,
        &state,
        &mut f,
        ControlAction::AcceptWarning {
            warning: WarningId(1),
        },
        2,
    );
    assert_eq!(f.fatigue, 1000);
    assert_eq!(burden(&store, ACTOR).unwrap().burden, 6000);
    assert!(state.active(&store).is_some());
    assert!(
        effects
            .iter()
            .any(|e| matches!(e,ControlEffect::Started {actor,..}if *actor==ACTOR))
    );
}
#[test]
fn warning_before_new_cost_and_guarded_acceptance_never_partially_changes_state() {
    let state = ControlState::new(Rules::fixture()).unwrap();
    let mut store = control_store();
    let mut f = facts();
    start(&mut store, &state, &mut f, BTreeSet::from([OLD]));
    let boundary = state.boundary(
        &store,
        SimTime(0),
        SimTime(200_000),
        &BTreeSet::from([ACTOR]),
    );
    assert_eq!(boundary, SimTime(114_000));
    let a = state.active(&store).unwrap();
    let advanced = advance_burden(AdvanceInput {
        rules: &state.rules,
        before: burden(&store, ACTOR).unwrap(),
        actor: ACTOR,
        now: SimTime(0),
        next: boundary,
        alive: true,
        active: Some(ActiveInput {
            life_count: a.lives.len(),
            accepted: &a.accepted,
            episode: None,
            next_loss_burden: a.next_loss_burden,
        }),
    })
    .unwrap();
    insert_burden(&mut store, ACTOR, advanced.burden).unwrap();
    f.now = boundary;
    state
        .prepare_review(&store, &f, Some(WarningId(3)))
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert_eq!(
        state.pending_warning(&store).unwrap().cost,
        InvocationCost::Strain
    );
    assert_eq!(state.pending_warning(&store).unwrap().severity, 1);
    assert_eq!(
        body_rates(
            &state.rules,
            burden(&store, ACTOR).unwrap(),
            Some(ActiveInput {
                life_count: 1,
                accepted: &state.active(&store).unwrap().accepted,
                episode: None,
                next_loss_burden: state.rules.loss_threshold
            }),
            ACTOR,
            boundary,
            true
        )
        .unwrap()
        .fatigue_per_mille,
        0
    );
    let before = state.clone();
    assert!(
        state
            .prepare_action(
                &store,
                ACTOR,
                &ControlAction::AcceptWarning {
                    warning: WarningId(999)
                },
                &f,
                Some(WarningId(4))
            )
            .is_err()
    );
    assert_eq!(state, before);
    let prepared = state
        .prepare_action(
            &store,
            ACTOR,
            &ControlAction::AcceptWarning {
                warning: WarningId(3),
            },
            &f,
            Some(WarningId(4)),
        )
        .unwrap();
    execute(&mut store, &state, &mut f, ControlAction::Dismiss, 4);
    let after = state.clone();
    assert_eq!(prepared.commit(&state, &mut store), Err(Error::Unavailable));
    assert_eq!(state, after);
}
#[test]
fn accepted_episode_has_no_dismissal_reconsent_or_protective_cost_cap_and_selects_competing_lives()
{
    let state = ControlState::new(Rules::fixture()).unwrap();
    let mut store = control_store();
    let mut f = facts();
    start(
        &mut store,
        &state,
        &mut f,
        BTreeSet::from([OLD, PersonId(12)]),
    );
    burden_mut(&mut store, ACTOR).unwrap().burden = state.rules.loss_threshold;
    let mut id = 10;
    state
        .prepare_review(&store, &f, Some(WarningId(id)))
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    id += 1;
    while let Some(w) = state.pending_consent(&store).cloned() {
        execute(
            &mut store,
            &state,
            &mut f,
            ControlAction::AcceptWarning { warning: w.id },
            id,
        );
        id += 1;
    }
    let episode = state.active(&store).unwrap().episode.clone().unwrap();
    assert_eq!(episode.ends, SimTime(state.rules.loss_duration.0));
    assert!(
        state
            .prepare_action(
                &store,
                ACTOR,
                &ControlAction::Dismiss,
                &f,
                Some(WarningId(id))
            )
            .is_err()
    );
    let tick = state
        .prepare_controller(&store, f.now, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        tick.effects,
        vec![ControlEffect::ControllerSelected {
            actor: ACTOR,
            life: OLD,
            changed: false
        }]
    );
    tick.commit(&state, &mut store).unwrap();
    f.now = SimTime(state.rules.controller_quantum.0);
    let tick = state
        .prepare_controller(&store, f.now, true)
        .unwrap()
        .unwrap();
    assert_eq!(
        tick.effects,
        vec![ControlEffect::ControllerSelected {
            actor: ACTOR,
            life: PersonId(12),
            changed: true
        }]
    );
    tick.commit(&state, &mut store).unwrap();
    let b = burden_mut(&mut store, ACTOR).unwrap();
    b.burden = state.rules.permanent_burden;
    b.exposure = state.rules.permanent_exposure.0;
    state
        .prepare_review(&store, &f, Some(WarningId(id)))
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert!(state.pending_consent(&store).is_none());
    assert!(burden(&store, ACTOR).unwrap().permanent_identity > 0);
    assert!(burden(&store, ACTOR).unwrap().lifespan_loss > 0);
}
#[test]
fn dismissal_and_reactivation_preserve_burden_without_creating_protracted_exposure() {
    let state = ControlState::new(Rules::fixture()).unwrap();
    let mut store = control_store();
    let mut f = facts();
    start(&mut store, &state, &mut f, BTreeSet::from([OLD]));
    burden_mut(&mut store, ACTOR).unwrap().uninterrupted = 10;
    execute(&mut store, &state, &mut f, ControlAction::Dismiss, 3);
    assert_eq!(burden(&store, ACTOR).unwrap().burden, 6000);
    assert_eq!(burden(&store, ACTOR).unwrap().uninterrupted, 0);
    execute(
        &mut store,
        &state,
        &mut f,
        ControlAction::Activate {
            lives: BTreeSet::from([OLD]),
        },
        4,
    );
    execute(
        &mut store,
        &state,
        &mut f,
        ControlAction::AcceptWarning {
            warning: WarningId(4),
        },
        5,
    );
    assert_eq!(burden(&store, ACTOR).unwrap().burden, 12000);
    assert_eq!(f.fatigue, 2000);
    assert_eq!(burden(&store, ACTOR).unwrap().exposure, 0);
    assert_eq!(burden(&store, ACTOR).unwrap().permanent_identity, 0);
}
#[test]
fn actual_death_clears_control_while_successor_does_not_inherit_direct_penalties() {
    let state = ControlState::new(Rules::fixture()).unwrap();
    let mut store = control_store();
    let mut f = facts();
    start(&mut store, &state, &mut f, BTreeSet::from([OLD]));
    let b = burden_mut(&mut store, ACTOR).unwrap();
    b.permanent_identity = 10;
    b.lifespan_loss = 60_000;
    b.uninterrupted = 10;
    state
        .prepare_deaths(&store, &BTreeSet::from([ACTOR]), f.now)
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert!(state.active(&store).is_none());
    assert_eq!(burden(&store, ACTOR).unwrap().uninterrupted, 0);
    assert_eq!(burden(&store, ACTOR).unwrap().lifespan_loss, 60_000);
    let successor = AdmissionFacts {
        actor: PersonId(20),
        current: Some(PersonId(20)),
        fatigue: 0,
        ..facts()
    };
    state
        .prepare_action(
            &store,
            PersonId(20),
            &ControlAction::Activate {
                lives: BTreeSet::from([OLD]),
            },
            &successor,
            Some(WarningId(3)),
        )
        .unwrap()
        .commit(&state, &mut store)
        .unwrap();
    assert_eq!(burden(&store, PersonId(20)).unwrap().permanent_identity, 0);
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<ControlState>(&json).unwrap(), state);
}
