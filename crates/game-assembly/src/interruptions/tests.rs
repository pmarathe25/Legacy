use super::*;

fn staged_interrupt(sim: &mut Simulation, actors: &[PersonId]) {
    let prepared = sim
        .prepare_interruptions(actors, ActivityOwner::Projects)
        .unwrap();
    sim.ensure_event_capacity(usize::try_from(prepared.event_count().unwrap()).unwrap())
        .unwrap();
    let mut batch = Batch::new();
    let mut finish = prepared
        .stage_into(sim, &mut batch, &BTreeMap::new())
        .unwrap();
    sim.host.storage.apply_batch(batch).unwrap();
    for actor in actors {
        finish.finish_actor(sim, *actor);
    }
}
#[test]
fn real_pregnancy_cancellation_matches_existing_dispatch_and_restores() {
    let mut old = Simulation::life_fixture(61, Execution::default()).unwrap();
    let parents = [old.host.state.people[3].id, old.host.state.people[4].id];
    old.debug_actor_command(
        parents[0],
        Command::Conceive {
            partner: parents[1],
        },
    )
    .unwrap();
    assert!(matches!(
        old.host.storage.activity(parents[0]),
        Some(Activity::Conceive { .. })
    ));
    let mut staged = Simulation::load(
        &old.save().unwrap(),
        Execution {
            workers: 3,
            reverse_jobs: true,
        },
    )
    .unwrap();
    old.interrupt_actor_activity_except_projects(parents[0]);
    old.interrupt_actor_activity_except_projects(parents[1]);
    staged_interrupt(&mut staged, &[parents[0], parents[1], parents[0]]);
    assert_eq!(old.canonical_state(), staged.canonical_state());
    assert_eq!(
        Simulation::load(&staged.save().unwrap(), Execution::default())
            .unwrap()
            .canonical_state(),
        staged.canonical_state()
    );
}
#[test]
fn invalid_foreign_write_and_stale_activity_publish_no_cancellation() {
    let mut sim = Simulation::survival_fixture(62, Execution::default()).unwrap();
    let actor = sim.host.state.ruler;
    let prepared = sim
        .prepare_interruptions(&[actor], ActivityOwner::Projects)
        .unwrap();
    let mut batch = Batch::new();
    prepared
        .stage_into(&sim, &mut batch, &BTreeMap::new())
        .unwrap();
    batch.remove::<Activity>(StableId(0));
    let before = sim.canonical_snapshot().unwrap();
    assert!(sim.host.storage.apply_batch(batch).is_err());
    assert_eq!(before, sim.canonical_snapshot().unwrap());
    let prepared = sim
        .prepare_interruptions(&[actor], ActivityOwner::Projects)
        .unwrap();
    *sim.host.storage.activity_mut(actor).unwrap() = Activity::Rest;
    let before = sim.canonical_snapshot().unwrap();
    assert!(
        prepared
            .stage_into(&sim, &mut Batch::new(), &BTreeMap::new())
            .is_err()
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}
#[test]
fn realm_recipient_fan_in_counts_every_event_without_fixed_budget() {
    let mut old = Simulation::kernel_fixture(63, 132, Execution::default()).unwrap();
    let rules = Simulation::survival_fixture(63, Execution::default())
        .unwrap()
        .host
        .state
        .physical
        .unwrap()
        .rules;
    old.initialize_physical_people(rules).unwrap();
    let recipient = old.host.state.ruler;
    let now = old.host.clock().time;
    let completes = SimTime(now.0.checked_add(MINUTE.0).unwrap());
    let area = old.resolved_location(recipient).cell().unwrap();
    let speakers: Vec<_> = old
        .host
        .state
        .people
        .iter()
        .map(|p| p.id)
        .filter(|id| *id != recipient)
        .collect();
    let mut realm = realm::World::default();
    for speaker in &speakers {
        let commitment = RealmCommitmentId(1);
        realm.talks.insert(
            *speaker,
            realm::Talk {
                recipient,
                area,
                completes,
                report: game_realm::Report {
                    commitment,
                    speaker: *speaker,
                    sent_at: now,
                    snapshot: game_realm::Knowledge::default(),
                },
                institutions: Vec::new(),
                stocks: Vec::new(),
            },
        );
        *old.host.storage.activity_mut(*speaker).unwrap() = Activity::RealmTalk {
            commitment,
            recipient,
            completes,
        };
        old.host
            .state
            .due
            .entry(completes)
            .or_default()
            .push(Due::Physical(*speaker));
    }
    old.host.state.realm = Some(realm);
    let mut staged = Simulation::kernel_fixture(63, 132, Execution::default()).unwrap();
    staged
        .initialize_physical_people(old.host.state.physical.as_ref().unwrap().rules.clone())
        .unwrap();
    staged.host.state.realm = old.host.state.realm.clone();
    for speaker in &speakers {
        *staged.host.storage.activity_mut(*speaker).unwrap() =
            old.host.storage.activity(*speaker).unwrap().clone();
    }
    staged.host.state.due = old.host.state.due.clone();
    let prepared = staged
        .prepare_interruptions(&[recipient], ActivityOwner::Projects)
        .unwrap();
    assert_eq!(prepared.event_count().unwrap(), 131);
    let mut batch = Batch::new();
    let mut finish = prepared
        .stage_into(&staged, &mut batch, &BTreeMap::new())
        .unwrap();
    staged.host.storage.apply_batch(batch).unwrap();
    finish.finish_actor(&mut staged, recipient);
    old.interrupt_actor_activity_except_projects(recipient);
    assert_eq!(old.canonical_state(), staged.canonical_state());
    assert_eq!(staged.host.state.realm.as_ref().unwrap().talks.len(), 0);
}
#[test]
fn caller_replacement_and_paused_workshop_job_are_not_overwritten() {
    let mut sim = Simulation::survival_fixture(64, Execution::default()).unwrap();
    let actor = sim.host.state.ruler;
    let site = game_workshop::sites(&sim.host.storage)
        .map(|(id, _)| id)
        .next()
        .unwrap();
    sim.debug_actor_command(actor, Command::Work { site })
        .unwrap();
    let before = game_workshop::job(&sim.host.storage, actor)
        .unwrap()
        .clone();
    let prepared = sim
        .prepare_interruptions(&[actor], ActivityOwner::Projects)
        .unwrap();
    let mut batch = Batch::new();
    let mut finish = prepared
        .stage_into(&sim, &mut batch, &BTreeMap::from([(actor, Activity::Rest)]))
        .unwrap();
    sim.host.storage.apply_batch(batch).unwrap();
    finish.finish_actor(&mut sim, actor);
    assert_eq!(sim.host.storage.activity(actor), Some(&Activity::Rest));
    assert_eq!(game_workshop::job(&sim.host.storage, actor), Some(&before));
}
