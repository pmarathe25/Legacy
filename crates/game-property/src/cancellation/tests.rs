use super::*;
use game_core::*;
use std::collections::BTreeMap;
fn schema() -> game_ecs::Schema {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<String>("test/foreign", |row| {
            if row == "reject" {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    schema
}
fn add_report(store: &mut Store, sender: u64) {
    insert_activity(
        store,
        PersonId(sender),
        Activity {
            pending: None,
            report: Some(Report {
                sender: PersonId(sender),
                recipient: PersonId(3),
                area: AreaId(10),
                started_at: SimTime(10),
                completes: SimTime(20),
                snapshot: IncidentKnowledge {
                    learned_at: SimTime(10),
                    source: IncidentSource::Observed,
                    incident: Incident {
                        id: IncidentId(9),
                        actor: PersonId(8),
                        source: InventoryId(11),
                        destination: InventoryId(12),
                        area: AreaId(10),
                        quantity: Quantity(100),
                        started_at: SimTime(0),
                        completes: SimTime(1),
                        changed_at: SimTime(1),
                        outcome: IncidentOutcome::Taken,
                        portions: BTreeMap::from([(ClaimId(0), 100)]),
                    },
                },
            }),
        },
    )
    .unwrap();
}
#[test]
fn shared_report_participants_cancel_once_in_first_initiator_order_atomically() {
    let mut store = Store::with_schema(schema());
    add_report(&mut store, 2);
    add_report(&mut store, 4);
    store
        .insert_or_spawn(StableId(3), "standing".to_owned())
        .unwrap();
    let mut runtime = Runtime::default();
    let actors = [PersonId(3), PersonId(2), PersonId(3), PersonId(4)];
    let prepared = runtime.prepare_cancel_many(&store, &actors, SimTime(11));
    let counts = prepared
        .effects()
        .map(|(actor, effects)| (actor, effects.len()))
        .collect::<Vec<_>>();
    assert_eq!(
        counts,
        vec![(PersonId(3), 2), (PersonId(2), 0), (PersonId(4), 0)]
    );
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = prepared.stage_into(&runtime, &store, &mut batch).unwrap();
    batch.insert(StableId(3), "reject".to_owned());
    assert!(store.apply_batch(batch).is_err());
    drop(finish);
    assert_eq!(store.snapshot().unwrap(), before);
    let effects = runtime
        .prepare_cancel_many(&store, &actors, SimTime(11))
        .commit(&mut runtime, &mut store)
        .unwrap();
    assert_eq!(
        effects
            .iter()
            .map(|(actor, effects)| (*actor, effects.len()))
            .collect::<Vec<_>>(),
        counts
    );
    assert!(matches!(
        &effects[0].1[..],
        [
            Effect::ClearReport {
                sender: PersonId(2),
                interrupted: true,
                ..
            },
            Effect::ClearReport {
                sender: PersonId(4),
                interrupted: true,
                ..
            }
        ]
    ));
    assert!(reports(&store).next().is_none());
    assert_eq!(
        store.get::<String>(StableId(3)).map(String::as_str),
        Some("standing")
    );
    assert_eq!(runtime.last_event, 0);
}
#[test]
fn new_receiver_report_membership_rejects_the_prepared_bulk_cancel() {
    let mut store = Store::with_schema(schema());
    add_report(&mut store, 2);
    let mut runtime = Runtime::default();
    let prepared = runtime.prepare_cancel_many(&store, &[PersonId(3), PersonId(2)], SimTime(11));
    add_report(&mut store, 4);
    let before = store.snapshot().unwrap();
    assert_eq!(prepared.validate(&runtime, &store), Err(Error::Unavailable));
    assert!(prepared.commit(&mut runtime, &mut store).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}

fn add_taking(store: &mut Store) {
    insert_incident(
        store,
        IncidentId(9),
        Incident {
            id: IncidentId(9),
            actor: PersonId(2),
            source: InventoryId(11),
            destination: InventoryId(12),
            area: AreaId(10),
            quantity: Quantity(100),
            started_at: SimTime(0),
            completes: SimTime(20),
            changed_at: SimTime(0),
            outcome: IncidentOutcome::Attempt,
            portions: BTreeMap::from([(ClaimId(0), 100)]),
        },
    )
    .unwrap();
    insert_activity(
        store,
        PersonId(2),
        Activity {
            pending: Some(IncidentId(9)),
            report: None,
        },
    )
    .unwrap();
}
#[test]
fn final_cancel_guard_rejects_late_changed_taking_and_report_rows() {
    for report in [false, true] {
        let mut store = Store::with_schema(schema());
        if report {
            add_report(&mut store, 2);
        } else {
            add_taking(&mut store);
        }
        let runtime = Runtime::default();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let finish = runtime
            .prepare_cancel_many(&store, &[PersonId(2)], SimTime(11))
            .stage_into(&runtime, &store, &mut batch)
            .unwrap();
        finish.validate_batch(&store, &batch).unwrap();
        batch.insert(StableId(2), activity(&store, PersonId(2)).unwrap().clone());
        assert_eq!(
            finish.validate_batch(&store, &batch),
            Err(Error::Unavailable)
        );
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(runtime.last_event, 0);
    }
}
#[test]
fn final_cancel_guard_rejects_late_new_report_membership_and_opening_read_changes() {
    let mut store = Store::with_schema(schema());
    add_report(&mut store, 2);
    let runtime = Runtime::default();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_cancel_many(&store, &[PersonId(3)], SimTime(11))
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    let mut incoming = activity(&store, PersonId(2)).unwrap().clone();
    incoming.report.as_mut().unwrap().sender = PersonId(4);
    batch.insert(StableId(4), incoming);
    assert_eq!(
        finish.validate_batch(&store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);

    let mut batch = Batch::new();
    let finish = runtime
        .prepare_cancel_many(&store, &[PersonId(3)], SimTime(11))
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    activity_mut(&mut store, PersonId(2))
        .unwrap()
        .report
        .as_mut()
        .unwrap()
        .completes = SimTime(21);
    let changed = store.snapshot().unwrap();
    assert_eq!(
        finish.validate_batch(&store, &batch),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), changed);
}
#[test]
fn cancellation_retains_runtime_cursor_until_final_admission() {
    let mut store = Store::with_schema(schema());
    add_report(&mut store, 2);
    let mut runtime = Runtime::default();
    let before = store.snapshot().unwrap();
    let mut batch = Batch::new();
    let finish = runtime
        .prepare_cancel_many(&store, &[PersonId(3)], SimTime(11))
        .stage_into(&runtime, &store, &mut batch)
        .unwrap();
    runtime.last_event = 7;
    assert_eq!(finish.validate_runtime(&runtime), Err(Error::Unavailable));
    assert_eq!(store.snapshot().unwrap(), before);
    assert_eq!(runtime.last_event, 7);
}
