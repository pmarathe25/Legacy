use super::*;

#[derive(Clone, Default, Serialize, Deserialize)]
struct PhaseState {
    ordinary: Vec<PersonId>,
    victim: Option<PersonId>,
    urgent: Option<PersonId>,
    interrupted: bool,
    reject_commit: bool,
    withdraw_on_fed: bool,
    withdrawn: bool,
    reject_withdrawn: bool,
    accepted_generation: Option<bool>,
    retained_backup: bool,
    urgent_on_rejection: bool,
    start_for: Option<PersonId>,
    end_for: Option<PersonId>,
    cascade: Option<(PersonId, bool)>,
    cascade_active: bool,
    cascade_control: bool,
    cascade_after: usize,
    cascade_attempts: Vec<u8>,
    rechecks: u64,
    trace: Vec<String>,
    batches: Vec<usize>,
}
impl Resource for PhaseState {
    const KEY: &'static str = "test.interaction-phases";
    fn validate(&self) -> Result<()> {
        require(
            !self.reject_commit || self.trace.is_empty(),
            "injected phase validation failure",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct ConfigurePhase(PhaseState);
impl Request for ConfigurePhase {
    const KEY: &'static str = "test.configure-interaction-phases";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct PreparePhase(Vec<PersonId>);
impl Request for PreparePhase {
    const KEY: &'static str = "test.prepare-interaction-phase";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Ordinary {
    person: PersonId,
    urgent: bool,
    generation: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Cascade(PersonId, u8, bool);
impl Request for Cascade {
    const KEY: &'static str = "test.interaction-retry-cascade";
    type Response = game_choice::Outcome;
}
impl Request for Ordinary {
    const KEY: &'static str = "test.interaction-ordinary";
    type Response = game_choice::Outcome;
}
fn phase_builder() -> Builder {
    let mut b = wave_builder();
    b.plugin("interaction-phase-test", 1).unwrap();
    b.resource::<PhaseState>().unwrap();
    b.request::<Cascade>(|tx, r| {
        tx.edit::<PhaseState>()?.cascade_attempts.push(r.1);
        let state = tx.get::<PhaseState>()?;
        if state.cascade_attempts.len() < state.cascade_after {
            return Ok(game_choice::Outcome::Reconsider);
        }
        if state.cascade_attempts.len() == 4 && state.cascade_active {
            let controlled = state.cascade_control;
            tx.call(game_choice::Undertake {
                person: r.0,
                kind: "interrupt-active".into(),
                duration: HOUR,
                request: Pending::immediate(game_choice::Wait),
            })?;
            if controlled {
                tx.call(game_lineage::Begin { person: r.0 })?;
            }
        }
        if r.2 {
            tx.call(game_physiology::Exert {
                person: r.0,
                energy: 1,
                fatigue: 1,
            })?;
        } else {
            tx.notify(game_characters::Alert { person: r.0 })?;
        }
        Ok(game_choice::Outcome::Reconsider)
    })
    .unwrap();
    b.request::<PreparePhase>(|tx, r| {
        tx.listener(
            &game_choice::PlanCohort { people: r.0 },
            "characters.prepare-decisions",
        )
    })
    .unwrap();
    b.request::<ConfigurePhase>(|tx, r| {
        *tx.edit::<PhaseState>()? = r.0;
        Ok(())
    })
    .unwrap();
    b.request::<Ordinary>(|tx, r| {
        let state = tx.get::<PhaseState>()?;
        if state.reject_withdrawn && state.withdrawn != r.generation && state.urgent_on_rejection {
            tx.edit::<PhaseState>()?.interrupted = true;
            tx.notify(game_characters::Alert { person: r.person })?;
            return Ok(game_choice::Outcome::Reconsider);
        }
        require(
            !state.reject_withdrawn || state.withdrawn == r.generation,
            "owner rejected obsolete intent",
        )?;
        tx.edit::<PhaseState>()?.accepted_generation = Some(r.generation);
        tx.edit::<PhaseState>()?
            .trace
            .push(format!("ordinary:{}:{}", r.person.0, r.urgent));
        if let Some(person) = tx.get::<PhaseState>()?.victim {
            tx.call(game_physiology::Exert {
                person,
                energy: 1,
                fatigue: 0,
            })?;
        }
        if let Some(person) = tx.get::<PhaseState>()?.start_for.filter(|p| *p != r.person) {
            tx.call(game_choice::Undertake {
                person,
                kind: "joint".into(),
                duration: HOUR,
                request: Pending::immediate(game_choice::Wait),
            })?;
        }
        if let Some(person) = tx.get::<PhaseState>()?.end_for.filter(|p| *p != r.person) {
            tx.cancel_action(person)?;
        }
        Ok(game_choice::Outcome::Performed)
    })
    .unwrap();
    game_choice::provider(&mut b, "test.phase-ordinary", |tx, e| {
        let state = tx.get::<PhaseState>()?;
        if let Some((person, body)) = state.cascade.filter(|(person, _)| *person == e.person) {
            for id in 0..32 {
                game_choice::offer(
                    tx,
                    person,
                    format!("test:cascade:{id}"),
                    game_choice::motive(4_000_000_000 - i64::from(id), 0, 0, 0, 0),
                    Cascade(person, id, body),
                )?;
            }
            return Ok(());
        }
        if state.ordinary.contains(&e.person) {
            let urgent = state.interrupted && state.urgent == Some(e.person);
            let generation = state.withdrawn;
            let backup = state.retained_backup;
            let continuation = (state.end_for == Some(e.person))
                .then(|| tx.get::<Actions>().unwrap().0.get(&e.person).map(|a| a.id))
                .flatten();
            let mut proposal = game_choice::Proposal::new(
                if urgent {
                    "test:urgent"
                } else if generation {
                    "test:replacement"
                } else {
                    "test:old"
                },
                game_choice::motive(3_000_000_000, 0, 0, 0, 0),
                Ordinary {
                    person: e.person,
                    urgent,
                    generation,
                },
            )?;
            proposal.continuation = continuation;
            game_choice::submit(tx, e.person, proposal)?;
            if backup {
                game_choice::offer(
                    tx,
                    e.person,
                    "test:backup",
                    game_choice::motive(2_000_000_000, 0, 0, 0, 0),
                    Ordinary {
                        person: e.person,
                        urgent: false,
                        generation: true,
                    },
                )?;
            }
        }
        Ok(())
    })
    .unwrap();
    b.before::<game_physiology::ServeLocalMeals>("test.capture-phase-batch", |tx, r| {
        tx.edit::<PhaseState>()?.batches.push(r.meals.len());
        Ok(())
    })
    .unwrap();
    b.subscribe::<game_physiology::Fed>("test.phase-fed", |tx, e| {
        if tx.get::<PhaseState>()?.withdraw_on_fed {
            tx.edit::<PhaseState>()?.withdrawn = true;
        }
        tx.edit::<PhaseState>()?
            .trace
            .push(format!("fed:{}", e.patient.0));
        if !tx.get::<PhaseState>()?.urgent_on_rejection
            && !tx.get::<PhaseState>()?.interrupted
            && let Some(person) = tx.get::<PhaseState>()?.urgent
        {
            tx.edit::<PhaseState>()?.interrupted = true;
            tx.notify(game_characters::Alert { person })?;
        }
        Ok(())
    })
    .unwrap();
    b
}
fn prepared_phase(urgent: bool) -> (Host, Vec<PersonId>) {
    let (mut host, people, store) = prepared_many(phase_builder(), true, 128, None, 8, 0, 1);
    host.dispatch(game_cargo::Deposit {
        inventory: store,
        good: game_cargo::FOOD,
        quantity: 70,
    })
    .unwrap();
    for &person in &people {
        host.dispatch(game_physiology::Exert {
            person,
            energy: 40_000,
            fatigue: 0,
        })
        .unwrap();
    }
    host.dispatch(ConfigurePhase(PhaseState {
        ordinary: if urgent {
            vec![people[2], people[5]]
        } else {
            vec![people[3]]
        },
        victim: (!urgent).then_some(people[0]),
        urgent: urgent.then_some(people[5]),
        ..Default::default()
    }))
    .unwrap();
    local_tuning(&mut host);
    (host, people)
}
#[test]
fn local_interactions_batch_mixed_cohort_before_interfering_ordinary_action() {
    let (initial, people) = prepared_phase(false);
    let saved = initial.save().unwrap();
    let mut worlds = Vec::new();
    for workers in [1, 4, 12] {
        let mut host = phase_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        let state = host.get::<PhaseState>().unwrap();
        assert_eq!(state.batches, vec![7]);
        assert!(
            state.trace[..7]
                .iter()
                .all(|event| event.starts_with("fed:"))
        );
        assert_eq!(state.trace[7], format!("ordinary:{}:false", people[3].0));
        assert!(
            host.get::<game_characters::Minds>()
                .unwrap()
                .paused_reviews
                .is_empty()
        );
        worlds.push(host.save().unwrap());
    }
    assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
}
#[test]
fn local_interactions_urgent_deferred_actor_preempts_and_replans_stale_choice() {
    let (initial, people) = prepared_phase(true);
    let saved = initial.save().unwrap();
    let mut worlds = Vec::new();
    for workers in [1, 4, 12] {
        let mut host = phase_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        let state = host.get::<PhaseState>().unwrap();
        assert_eq!(state.batches, vec![6]);
        assert_eq!(state.trace[6], format!("ordinary:{}:true", people[5].0));
        assert_eq!(state.trace[7], format!("ordinary:{}:false", people[2].0));
        assert!(
            !state
                .trace
                .contains(&format!("ordinary:{}:false", people[5].0))
        );
        assert!(
            host.get::<game_characters::Minds>()
                .unwrap()
                .paused_reviews
                .is_empty()
        );
        worlds.push(host.save().unwrap());
    }
    assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn local_interactions_commit_validation_rolls_back_the_whole_frame() {
    let (mut host, _) = prepared_phase(false);
    let mut state = host.get::<PhaseState>().unwrap().clone();
    state.reject_commit = true;
    host.dispatch(ConfigurePhase(state)).unwrap();
    let before = host.save().unwrap();
    assert!(host.advance(SimTime(30_000), 1).is_err());
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn local_interactions_owner_rejection_uses_remaining_prepared_alternatives() {
    for reject_withdrawn in [false, true] {
        let (mut initial, people) = prepared_phase(false);
        let mut state = initial.get::<PhaseState>().unwrap().clone();
        state.withdraw_on_fed = true;
        state.reject_withdrawn = reject_withdrawn;
        state.retained_backup = true;
        initial.dispatch(ConfigurePhase(state)).unwrap();
        initial.dispatch(PreparePhase(people.clone())).unwrap();
        let frame = initial
            .get::<game_choice::Offers>()
            .unwrap()
            .frame
            .as_ref()
            .unwrap();
        let prepared = game_choice::inspect_frame_request::<game_decision_intents::Prepared>(
            &frame.decisions[&people[3]],
        )
        .unwrap();
        assert_eq!(
            prepared.sources.get("test:old").map(String::as_str),
            Some("test.phase-ordinary")
        );
        let saved = initial.save().unwrap();
        let mut worlds = Vec::new();
        for workers in [1, 4, 12] {
            let mut host = phase_builder().load(&saved, workers).unwrap();
            support::run(&mut host, SimTime(30_000));
            assert_eq!(
                host.get::<PhaseState>().unwrap().accepted_generation,
                Some(reject_withdrawn)
            );
            assert_eq!(
                host.get::<game_characters::Minds>().unwrap().people[&people[3]].failures,
                u64::from(reject_withdrawn)
            );
            assert_eq!(
                host.get::<game_characters::Minds>().unwrap().people[&people[3]]
                    .chosen
                    .as_deref(),
                Some(if reject_withdrawn {
                    "test:backup"
                } else {
                    "test:old"
                })
            );
            worlds.push(host.save().unwrap());
        }
        assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

#[test]
fn local_interactions_urgent_owner_rejection_refreshes_before_retained_alternatives() {
    let (mut initial, people) = prepared_phase(false);
    let mut state = initial.get::<PhaseState>().unwrap().clone();
    state.withdraw_on_fed = true;
    state.reject_withdrawn = true;
    state.urgent_on_rejection = true;
    state.urgent = Some(people[3]);
    state.retained_backup = true;
    initial.dispatch(ConfigurePhase(state)).unwrap();
    initial.dispatch(PreparePhase(people.clone())).unwrap();
    let saved = initial.save().unwrap();
    let mut worlds = Vec::new();
    for workers in [1, 4, 12] {
        let mut host = phase_builder().load(&saved, workers).unwrap();
        support::run(&mut host, SimTime(30_000));
        let mind = &host.get::<game_characters::Minds>().unwrap().people[&people[3]];
        assert_eq!(mind.chosen.as_deref(), Some("test:urgent"));
        assert_eq!(mind.failures, 1);
        assert!(
            host.get::<PhaseState>()
                .unwrap()
                .trace
                .contains(&format!("ordinary:{}:true", people[3].0))
        );
        worlds.push(host.save().unwrap());
    }
    assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn local_interactions_refreshes_share_one_budget_across_many_owner_interruptions() {
    for body_change in [false, true] {
        let (mut initial, people) = prepared_phase(false);
        let mut state = initial.get::<PhaseState>().unwrap().clone();
        state.victim = None;
        state.cascade = Some((people[3], body_change));
        initial.dispatch(ConfigurePhase(state)).unwrap();
        initial.dispatch(PreparePhase(people.clone())).unwrap();
        let saved = initial.save().unwrap();
        let mut worlds = Vec::new();
        for workers in [1, 4, 12] {
            let mut host = phase_builder().load(&saved, workers).unwrap();
            support::run(&mut host, SimTime(30_000));
            let attempts = &host.get::<PhaseState>().unwrap().cascade_attempts;
            // Each attempt and each intervening fresh review consumes one step.
            assert_eq!(attempts.len(), 4);
            assert_eq!(
                host.get::<game_characters::Minds>()
                    .unwrap()
                    .schedule
                    .waking[&people[3]],
                SimTime(60_000)
            );
            assert_eq!(
                attempts
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                4
            );
            assert!(
                host.get::<game_characters::Minds>()
                    .unwrap()
                    .paused_reviews
                    .is_empty()
            );
            assert!(
                host.get::<game_characters::Minds>()
                    .unwrap()
                    .cohort_urgency
                    .is_empty()
            );
            worlds.push(host.save().unwrap());
        }
        assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

#[test]
fn local_interactions_eighth_owner_attempt_still_preserves_its_interrupt() {
    let (mut host, people) = prepared_phase(false);
    let person = people[3];
    let mut state = host.get::<PhaseState>().unwrap().clone();
    state.victim = None;
    state.cascade = Some((person, false));
    state.cascade_after = 8;
    host.dispatch(ConfigurePhase(state)).unwrap();
    host.dispatch(PreparePhase(people)).unwrap();
    support::run(&mut host, SimTime(30_000));
    assert_eq!(host.get::<PhaseState>().unwrap().cascade_attempts.len(), 8);
    assert_eq!(
        host.get::<game_characters::Minds>()
            .unwrap()
            .schedule
            .waking[&person],
        SimTime(60_000)
    );
}

#[test]
fn local_interactions_exhausted_interrupt_wake_rolls_back_with_its_frame() {
    let (mut host, people) = prepared_phase(false);
    let mut state = host.get::<PhaseState>().unwrap().clone();
    state.victim = None;
    state.cascade = Some((people[3], false));
    state.reject_commit = true;
    host.dispatch(ConfigurePhase(state)).unwrap();
    host.dispatch(PreparePhase(people)).unwrap();
    let before = host.save().unwrap();
    assert!(host.advance(SimTime(30_000), 1).is_err());
    assert_eq!(host.save().unwrap(), before);
}

#[test]
fn local_interactions_exhausted_interrupt_wakes_active_actor_without_overriding_control() {
    for controlled in [false, true] {
        let (mut initial, people) = prepared_phase(false);
        let person = people[3];
        let mut state = initial.get::<PhaseState>().unwrap().clone();
        state.victim = None;
        state.cascade = Some((person, false));
        state.cascade_active = true;
        state.cascade_control = controlled;
        initial.dispatch(ConfigurePhase(state)).unwrap();
        initial.dispatch(PreparePhase(people.clone())).unwrap();
        let saved = initial.save().unwrap();
        let mut worlds = Vec::new();
        for workers in [1, 4, 12] {
            let mut host = phase_builder().load(&saved, workers).unwrap();
            support::run(&mut host, SimTime(30_000));
            assert_eq!(host.get::<PhaseState>().unwrap().cascade_attempts.len(), 4);
            let action = host.get::<Actions>().unwrap().0[&person].id;
            assert_eq!(
                host.get::<game_characters::Minds>()
                    .unwrap()
                    .schedule
                    .waking[&person],
                SimTime(60_000)
            );
            // Save/load must retain the deferred evidence even though the action ends an hour later.
            let mut host = phase_builder()
                .load(&host.save().unwrap(), workers)
                .unwrap();
            support::run(&mut host, SimTime(60_000));
            let attempts = host.get::<PhaseState>().unwrap().cascade_attempts.len();
            if controlled {
                assert_eq!(attempts, 4);
                assert_eq!(host.get::<Actions>().unwrap().0[&person].id, action);
            } else {
                assert!(
                    attempts > 4,
                    "the exhausted interrupt must get another review"
                );
            }
            worlds.push(host.save().unwrap());
        }
        assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

#[test]
fn local_interactions_preserve_new_actions_and_discard_ended_continuations() {
    for ended in [false, true] {
        let (mut initial, people) = prepared_phase(false);
        let mut state = initial.get::<PhaseState>().unwrap().clone();
        state.ordinary = vec![people[2], people[5]];
        state.victim = None;
        state.retained_backup = true;
        if ended {
            state.end_for = Some(people[5]);
            initial
                .dispatch(game_choice::Undertake {
                    person: people[5],
                    kind: "prior".into(),
                    duration: HOUR,
                    request: Pending::immediate(game_choice::Wait),
                })
                .unwrap();
            // Replace the fixture's idle frame snapshot with preparation of this action.
            initial
                .dispatch(game_choice::InvalidateFramePerson(people[5]))
                .unwrap();
        } else {
            state.start_for = Some(people[5]);
        }
        initial.dispatch(ConfigurePhase(state)).unwrap();
        initial.dispatch(PreparePhase(people.clone())).unwrap();
        let saved = initial.save().unwrap();
        let mut worlds = Vec::new();
        for workers in [1, 4, 12] {
            let mut host = phase_builder().load(&saved, workers).unwrap();
            support::run(&mut host, SimTime(30_000));
            let mind = &host.get::<game_characters::Minds>().unwrap().people[&people[5]];
            assert_eq!(mind.failures, 0);
            if ended {
                assert_eq!(mind.chosen.as_deref(), Some("test:backup"));
                assert!(host.get::<Actions>().unwrap().0.get(&people[5]).is_none());
            } else {
                assert!(mind.chosen.is_none());
                assert_eq!(host.get::<Actions>().unwrap().0[&people[5]].kind, "joint");
                assert!(
                    !host
                        .get::<PhaseState>()
                        .unwrap()
                        .trace
                        .contains(&format!("ordinary:{}:false", people[5].0))
                );
            }
            assert!(
                host.get::<game_characters::Minds>()
                    .unwrap()
                    .paused_reviews
                    .is_empty()
            );
            worlds.push(host.save().unwrap());
        }
        assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
    }
}

#[test]
fn local_interactions_preserve_explicit_provider_recheck_hooks() {
    let (mut initial, people) = prepared_phase(false);
    initial.dispatch(PreparePhase(people)).unwrap();
    let mut b = phase_builder();
    b.before::<game_choice::FrameRecheck>("test.explicit-recheck", |tx, _| {
        tx.edit::<PhaseState>()?.rechecks += 1;
        Ok(())
    })
    .unwrap();
    let mut host = b.load(&initial.save().unwrap(), 4).unwrap();
    support::run(&mut host, SimTime(30_000));
    assert!(host.get::<PhaseState>().unwrap().rechecks > 0);
}

fn companionship_builder() -> Builder {
    let mut b = phase_builder();
    game_vitality::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_companionship::plugin(&mut b).unwrap();
    b
}
#[test]
fn local_interactions_companionship_owner_rejects_newly_controlled_recipient() {
    let (mut initial, people, _) = prepared_many(companionship_builder(), true, 128, None, 2, 0, 1);
    for &person in &people {
        initial
            .dispatch(game_social_life::Register {
                person,
                name: format!("person {}", person.0),
            })
            .unwrap();
    }
    let proposal = initial
        .dispatch(game_choice::Collect { person: people[0] })
        .unwrap()
        .into_iter()
        .find(|p| p.key == format!("company:{}", people[1].0))
        .unwrap();
    // The offered request was valid when selected; ownership changes afterward.
    initial
        .dispatch(game_lineage::Begin { person: people[1] })
        .unwrap();
    let saved = initial.save().unwrap();
    for workers in [1, 4, 12] {
        let mut host = companionship_builder().load(&saved, workers).unwrap();
        let before = host.save().unwrap();
        assert!(
            host.dispatch(game_choice::Run(proposal.request.clone()))
                .is_err()
        );
        assert_eq!(host.save().unwrap(), before);
        assert!(host.get::<Actions>().unwrap().0.get(&people[1]).is_none());
    }
}

#[test]
fn companionship_search_skips_current_heir_even_when_caller_protects_someone_else() {
    let mut initial = companionship_builder().build(3, 1).unwrap();
    initial
        .dispatch(game_social_life::Configure(game_social_life::Rules {
            contact_interval: MINUTE,
            ..Default::default()
        }))
        .unwrap();
    let area = initial
        .dispatch(game_land::CreateArea {
            name: "company".into(),
        })
        .unwrap();
    let mut people = Vec::new();
    for _ in 0..3 {
        let person = initial
            .dispatch(game_physiology::Create {
                kind: game_physiology::Kind::Mortal,
                born: SimTime(0),
            })
            .unwrap();
        initial
            .dispatch(game_transport::Place { person, area })
            .unwrap();
        initial
            .dispatch(game_social_life::Register {
                person,
                name: format!("person {}", person.0),
            })
            .unwrap();
        people.push(person);
    }
    initial
        .dispatch(game_lineage::Begin { person: people[1] })
        .unwrap();
    // Without the lineage filter this target wins both affinity and ID ordering.
    for _ in 0..20 {
        initial
            .dispatch(game_relationships::Converse {
                observer: people[0],
                other: people[1],
                warmth: 5,
            })
            .unwrap();
    }
    initial
        .dispatch(game_relationships::Converse {
            observer: people[0],
            other: people[2],
            warmth: 0,
        })
        .unwrap();
    initial
        .dispatch(game_relationships::LearnValue {
            person: people[0],
            concept: "fellowship".into(),
            outcome: 5,
        })
        .unwrap();
    support::run(&mut initial, SimTime(MINUTE.0));
    let saved = initial.save().unwrap();
    for protected in [None, Some(people[0])] {
        let mut worlds = Vec::new();
        for workers in [1, 4, 12] {
            let mut host = companionship_builder().load(&saved, workers).unwrap();
            assert!(
                host.dispatch(game_companionship::SeekCompany {
                    person: people[0],
                    protected
                })
                .unwrap()
            );
            assert_eq!(
                host.get::<game_social_life::SocialLife>().unwrap().meetings[&people[0]].other,
                people[2]
            );
            assert!(host.get::<Actions>().unwrap().0.get(&people[1]).is_none());
            worlds.push(host.save().unwrap());
        }
        assert!(worlds.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
