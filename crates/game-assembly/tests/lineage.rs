mod support;
use game_api::*;
use game_assembly::{
    decisions::{Aspiration, Personality, Relationship},
    lineage::GuardianGoal,
    *,
};

fn fixture(workers: usize) -> Simulation {
    Simulation::lineage_fixture(
        1,
        Execution {
            workers,
            reverse_jobs: true,
        },
    )
    .unwrap()
}

fn kill_ruler(sim: &mut Simulation) -> PersonId {
    let ruler = sim.canonical_state().state.ruler;
    let health = sim
        .canonical_state()
        .state
        .physical
        .as_ref()
        .unwrap()
        .rules
        .max_health;
    sim.debug_injure(ruler, health).unwrap();
    ruler
}

fn guardians(sim: &Simulation) -> Vec<PersonId> {
    sim.canonical_state()
        .state
        .lineage
        .as_ref()
        .unwrap()
        .guardians
        .keys()
        .copied()
        .collect()
}

fn quiet_other_caregivers(sim: &mut Simulation, child: PersonId) {
    let citizens: Vec<_> = sim
        .canonical_state()
        .state
        .people
        .iter()
        .filter(|p| p.role == Role::Citizen)
        .map(|p| p.id)
        .collect();
    for citizen in citizens {
        let mut personality = Personality::restful();
        personality.empathy = 0;
        sim.debug_set_personality(citizen, personality, Aspiration::Comfort)
            .unwrap();
        sim.debug_set_relationship(
            citizen,
            child,
            Relationship {
                trust: 0,
                affection: 0,
            },
        )
        .unwrap();
    }
}

fn child_for_mentorship(workers: usize) -> Simulation {
    let mut sim = fixture(workers);
    kill_ruler(&mut sim);
    sim.advance(SimDuration(900_000)).unwrap();
    let child = sim.canonical_state().state.ruler;
    quiet_other_caregivers(&mut sim, child);
    send(
        &mut sim,
        Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    );
    sim.advance(SimDuration(300_000)).unwrap();
    assert_eq!(sim.observe().life_stage, Some(LifeStage::Child));
    sim
}

fn send(sim: &mut Simulation, command: Command) {
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command,
    })
    .unwrap();
}

#[test]
fn guided_childhood_learning_takes_two_real_primary_actions_and_does_not_grant_readiness() {
    let mut sim = child_for_mentorship(1);
    let child = sim.canonical_state().state.ruler;
    let teacher = guardians(&sim)[0];
    assert!(
        sim.observe()
            .lineage
            .unwrap()
            .guardians
            .iter()
            .all(|g| !g.ready)
    );
    assert!(!sim.observe().lineage.unwrap().invocation_ready);
    assert!(sim.observe().available_commands.contains(&Command::Learn {
        teacher,
        topic: LessonTopic::Invocation
    }));
    send(
        &mut sim,
        Command::Learn {
            teacher,
            topic: LessonTopic::Invocation,
        },
    );
    for actor in [child, teacher] {
        assert!(matches!(
            *sim.canonical_state()
                .components
                .activity(actor)
                .expect("activity exists"),
            Activity::Lesson { .. }
        ));
    }
    let pending = sim.canonical_snapshot().unwrap();
    let other_teacher = guardians(&sim)[1];
    assert!(
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::RequestGuardian {
                guardian: other_teacher,
                request: GuardianRequest::Rest
            },
        })
        .is_err()
    );
    assert_eq!(sim.canonical_snapshot().unwrap(), pending);
    sim.advance(SimDuration(59_999)).unwrap();
    assert_eq!(sim.observe().lineage.unwrap().invocation_learning, 0);
    sim.advance(SimDuration(1)).unwrap();
    assert_eq!(sim.observe().lineage.unwrap().invocation_learning, 100);
    assert!(!sim.observe().lineage.unwrap().invocation_ready);
    send(
        &mut sim,
        Command::Learn {
            teacher,
            topic: LessonTopic::Invocation,
        },
    );
    sim.advance(MINUTE).unwrap();
    let view = sim.observe().lineage.unwrap();
    assert_eq!(view.invocation_learning, 200);
    assert!(view.invocation_ready);
    assert!(view.guardians.iter().all(|g| !g.ready));
    assert_eq!(sim.canonical_state().events.iter().filter(|event| matches!(event.event,
        EventKind::LessonCompleted { student, topic: LessonTopic::Invocation, gain: 100, .. } if student == child)).count(), 2);
}

#[test]
fn interrupting_either_lesson_participant_discards_unfinished_learning() {
    for interrupt_teacher in [false, true] {
        let mut sim = child_for_mentorship(1);
        let child = sim.canonical_state().state.ruler;
        let teacher = guardians(&sim)[0];
        send(
            &mut sim,
            Command::Learn {
                teacher,
                topic: LessonTopic::Invocation,
            },
        );
        sim.advance(SimDuration(30_000)).unwrap();
        if interrupt_teacher {
            sim.debug_actor_command(teacher, Command::Cancel).unwrap();
        } else {
            send(
                &mut sim,
                Command::Move {
                    destination: AreaId(2),
                },
            );
        }
        assert!(!matches!(
            *sim.canonical_state()
                .components
                .activity(teacher)
                .expect("activity exists"),
            Activity::Lesson { .. }
        ));
        assert!(!matches!(
            *sim.canonical_state()
                .components
                .activity(child)
                .expect("activity exists"),
            Activity::Lesson { .. }
        ));
        sim.advance(SimDuration(90_000)).unwrap();
        assert_eq!(
            sim.canonical_state()
                .state
                .lineage
                .as_ref()
                .unwrap()
                .invocation_learning
                .get(&child)
                .copied()
                .unwrap_or(0),
            0
        );
        assert!(
            !sim.canonical_state()
                .events
                .iter()
                .any(|event| matches!(event.event,
            EventKind::LessonCompleted { student, .. } if student == child))
        );
        Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    }
}

#[test]
fn guardians_disagree_from_observed_conduct_and_do_not_credit_unseen_practice() {
    let mut sim = child_for_mentorship(1);
    let child = sim.canonical_state().state.ruler;
    let observers = guardians(&sim);
    sim.debug_set_personality(
        observers[0],
        Personality::restful(),
        Aspiration::SupportOthers,
    )
    .unwrap();
    sim.debug_set_personality(
        observers[1],
        Personality::industrious(),
        Aspiration::SupportOthers,
    )
    .unwrap();
    send(
        &mut sim,
        Command::Play {
            interest: ChildhoodInterest::Craft,
        },
    );
    sim.advance(MINUTE).unwrap();
    send(
        &mut sim,
        Command::Move {
            destination: AreaId(2),
        },
    );
    let world = sim.canonical_state().state.lineage.as_ref().unwrap();
    let flexible = &world.guardians[&observers[0]].assessments[&child];
    let careful = &world.guardians[&observers[1]].assessments[&child];
    assert!(flexible.ready);
    assert!(!careful.ready);
    assert_eq!(flexible.independence, careful.independence);
    assert_eq!(flexible.competence, careful.competence);
    assert_ne!(flexible.explanation, careful.explanation);

    let mut unseen = child_for_mentorship(1);
    let child = unseen.canonical_state().state.ruler;
    for observer in guardians(&unseen) {
        unseen.debug_move_actor(observer, AreaId(3)).unwrap();
    }
    send(
        &mut unseen,
        Command::Play {
            interest: ChildhoodInterest::Craft,
        },
    );
    unseen.advance(SimDuration(360_000)).unwrap();
    assert_eq!(
        unseen.observe().interests,
        vec![(ChildhoodInterest::Craft, 100)]
    );
    assert!(
        unseen
            .canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .values()
            .all(|g| g.assessments[&child].competence == 0 && !g.assessments[&child].ready)
    );
}

#[test]
fn a_ready_guardian_performs_real_work_but_refuses_resources_they_cannot_use() {
    let mut sim = child_for_mentorship(1);
    let child = sim.canonical_state().state.ruler;
    let guardian = guardians(&sim)[0];
    sim.debug_set_personality(guardian, Personality::restful(), Aspiration::SupportOthers)
        .unwrap();
    let food = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Carried(child),
            Quantity(1_000),
            Quantity(1_000),
            std::collections::BTreeSet::from([child]),
        )
        .unwrap();
    send(
        &mut sim,
        Command::Play {
            interest: ChildhoodInterest::Craft,
        },
    );
    sim.advance(MINUTE).unwrap();
    send(&mut sim, Command::Eat { source: food });
    sim.advance(MINUTE).unwrap();
    assert!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians[&guardian]
            .assessments[&child]
            .ready
    );
    let site = game_workshop::sites(sim.canonical_state().components)
        .map(|(_, site)| site)
        .find(|site| site.area == AreaId(1))
        .unwrap()
        .id;
    send(
        &mut sim,
        Command::RequestGuardian {
            guardian,
            request: GuardianRequest::Work { site },
        },
    );
    assert!(matches!(
        *sim.canonical_state()
            .components
            .activity(guardian)
            .expect("activity exists"),
        Activity::Work { .. }
    ));
    sim.advance(MINUTE).unwrap();
    assert!(sim.canonical_state().events.iter().any(|event| matches!(event.event, EventKind::Produced { actor, site: source, .. } if actor == guardian && source == site)));
    assert!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians[&guardian]
            .assignment
            .is_none()
    );
    let mut save: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    support::component_id(&mut save, "workshop/site", site.0)["access"]
        .as_array_mut()
        .unwrap()
        .retain(|id| id != &serde_json::json!(guardian.0));
    support::component(&mut save, "characters/mind", guardian)["sites"][site.0.to_string()]["access"] =
        serde_json::json!(false);
    let mut restricted =
        Simulation::load(&serde_json::to_vec(&save).unwrap(), Execution::default()).unwrap();
    let stocks = restricted.canonical_state().state.inventories.clone();
    send(
        &mut restricted,
        Command::RequestGuardian {
            guardian,
            request: GuardianRequest::Work { site },
        },
    );
    assert_eq!(restricted.canonical_state().state.inventories, stocks);
    assert!(
        matches!(restricted.canonical_state().events.last().unwrap().event, EventKind::GuardianRefused { guardian: person, .. } if person == guardian)
    );
    assert_eq!(
        restricted
            .canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians[&guardian]
            .target,
        Some(child)
    );
    assert!(
        restricted
            .observe()
            .lineage
            .unwrap()
            .guardians
            .iter()
            .find(|g| g.guardian == guardian)
            .unwrap()
            .explanation
            .contains("permissions")
    );
}

#[test]
fn travel_instruction_copies_personal_route_knowledge_and_cannot_teach_an_unknown_craft() {
    let sim = child_for_mentorship(1);
    let child = sim.canonical_state().state.ruler;
    let teacher = guardians(&sim)[0];
    let mut save: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    support::component(&mut save, "characters/mind", teacher)["routes"]
        .as_object_mut()
        .unwrap()
        .remove("3");
    let mut sim =
        Simulation::load(&serde_json::to_vec(&save).unwrap(), Execution::default()).unwrap();
    assert!(
        !game_characters::decisions::mind::<game_api::Command>(
            sim.canonical_state().components,
            child
        )
        .expect("mind exists")
        .routes
        .contains_key(&AreaId(2))
    );
    send(
        &mut sim,
        Command::Learn {
            teacher,
            topic: LessonTopic::Travel,
        },
    );
    sim.advance(MINUTE).unwrap();
    let routes = &game_characters::decisions::mind::<game_api::Command>(
        sim.canonical_state().components,
        child,
    )
    .expect("mind exists")
    .routes;
    assert!(routes.contains_key(&AreaId(2)));
    assert!(!routes.contains_key(&AreaId(3)));
    assert_eq!(sim.observe().lineage.unwrap().invocation_learning, 0);
    send(
        &mut sim,
        Command::Learn {
            teacher,
            topic: LessonTopic::Craft,
        },
    );
    let view = sim.observe().lineage.unwrap();
    assert!(
        view.guardians
            .iter()
            .find(|g| g.guardian == teacher)
            .unwrap()
            .explanation
            .contains("cannot teach")
    );
    assert!(sim.observe().interests.is_empty());
}

#[test]
fn pending_coupled_lessons_preserve_exactly_once_learning_across_workers_saves_and_queries() {
    let mut a = child_for_mentorship(1);
    let teacher = guardians(&a)[0];
    let child = a.canonical_state().state.ruler;
    send(
        &mut a,
        Command::Learn {
            teacher,
            topic: LessonTopic::Invocation,
        },
    );
    a.advance(SimDuration(30_000)).unwrap();
    let saved = a.save().unwrap();
    let mut invalid: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    support::component(&mut invalid, "coordination/activity", child)["Lesson"]["topic"] =
        serde_json::json!("Travel");
    assert!(
        Simulation::load(&serde_json::to_vec(&invalid).unwrap(), Execution::default()).is_err()
    );
    let mut b = Simulation::load(
        &saved,
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let before = a.canonical_snapshot().unwrap();
    for _ in 0..100 {
        a.observe();
    }
    assert_eq!(a.canonical_snapshot().unwrap(), before);
    a.advance(SimDuration(90_000)).unwrap();
    for dt in [1, 29_999, 60_000] {
        b.advance(SimDuration(dt)).unwrap();
    }
    assert_eq!(a.canonical_state(), b.canonical_state());
    assert_eq!(a.observe().lineage.unwrap().invocation_learning, 100);
    assert_eq!(
        a.canonical_state()
            .events
            .iter()
            .filter(|event| matches!(event.event,
        EventKind::LessonCompleted { student, .. } if student == child))
            .count(),
        1
    );
}

#[test]
fn guardian_spectating_exists_only_in_the_gap_and_never_reveals_a_future_child() {
    let mut sim = fixture(1);
    let guardians = guardians(&sim);
    for guardian in &guardians {
        sim.debug_move_actor(*guardian, AreaId(3)).unwrap();
    }
    sim.advance(SimDuration(180_000)).unwrap();
    assert!(sim.observe().lineage.unwrap().guardians.is_empty());
    kill_ruler(&mut sim);
    let before = sim.canonical_snapshot().unwrap();
    for _ in 0..100 {
        let view = sim.observe().lineage.unwrap();
        assert_eq!(view.current, None);
        assert_eq!(view.waiting_since, Some(SimTime(180_000)));
        assert_eq!(view.guardians.len(), guardians.len());
        assert!(view.guardians.iter().all(|g| g.target.is_none()));
        for guardian in view.guardians {
            let person = sim
                .canonical_state()
                .state
                .people
                .iter()
                .find(|p| p.id == guardian.guardian)
                .unwrap();
            assert_eq!(
                guardian.location,
                *game_spatial::location(
                    sim.canonical_state().components,
                    game_ecs::StableId((person.id).0)
                )
                .expect("location exists")
            );
        }
    }
    assert_eq!(sim.canonical_snapshot().unwrap(), before);
    sim.advance(SimDuration(120_000)).unwrap();
    let view = sim.observe().lineage.unwrap();
    assert_eq!(view.current, Some(sim.canonical_state().state.ruler));
    assert_eq!(view.waiting_since, None);
    assert!(view.guardians.is_empty());
}

#[test]
fn simultaneous_births_use_persistent_order_without_rebinding_to_the_second_child() {
    let mut sim = fixture(4);
    let pairs: Vec<_> = sim.canonical_state().state.people[3..7]
        .chunks(2)
        .map(|pair| (pair[0].id, pair[1].id))
        .collect();
    for (first, second) in pairs {
        for (person, partner) in [(first, second), (second, first)] {
            sim.debug_set_personality(person, Personality::restful(), Aspiration::Parenthood)
                .unwrap();
            sim.debug_set_relationship(
                person,
                partner,
                Relationship {
                    trust: 800,
                    affection: 800,
                },
            )
            .unwrap();
        }
        sim.debug_actor_command(first, Command::Conceive { partner: second })
            .unwrap();
    }
    kill_ruler(&mut sim);
    let result = sim.advance(SimDuration(900_000)).unwrap();
    assert_eq!(result.reached, SimTime(240_000));
    let newborns: Vec<_> = game_lifecycle::all_lives(sim.canonical_state().components)
        .map(|(_, row)| row)
        .filter(|life| life.born == SimTime(240_000))
        .map(|life| life.id)
        .collect();
    assert_eq!(newborns.len(), 2);
    assert_eq!(sim.canonical_state().state.ruler, newborns[0]);
    assert_eq!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .incarnations
            .len(),
        2
    );
}

#[test]
fn a_birth_at_the_death_timestamp_waits_for_a_strictly_later_birth() {
    let original = fixture(1);
    let ruler = original.canonical_state().state.ruler;
    let mut save: serde_json::Value = serde_json::from_slice(&original.save().unwrap()).unwrap();
    let life = support::component(&mut save, "lifecycle/life", ruler);
    life["initial_age"] = serde_json::json!(1_200_000);
    life["stage"] = serde_json::json!("Elder");
    let state = save["state"]["state"].as_object_mut().unwrap();
    let due = state.get_mut("due").unwrap().as_object_mut().unwrap();
    let age = serde_json::json!({"Age": ruler.0});
    for events in due.values_mut() {
        events.as_array_mut().unwrap().retain(|event| event != &age);
    }
    due.retain(|_, events| !events.as_array().unwrap().is_empty());
    due.entry("300000")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .unwrap()
        .push(age);
    let mut sim =
        Simulation::load(&serde_json::to_vec(&save).unwrap(), Execution::default()).unwrap();
    let result = sim.advance(SimDuration(300_000)).unwrap();
    assert_eq!(result.reason, StopReason::TargetReached);
    let world = sim.canonical_state().state.lineage.as_ref().unwrap();
    assert_eq!(world.current(), None);
    assert_eq!(world.waiting.as_ref().unwrap().died_at, SimTime(300_000));
    assert!(
        game_lifecycle::all_lives(sim.canonical_state().components)
            .map(|(_, row)| row)
            .any(|life| life.born == SimTime(300_000) && !life.parents.is_empty())
    );
    let later = sim.advance(SimDuration(600_000)).unwrap();
    assert!(matches!(later.reason, StopReason::SuccessorBorn { .. }));
    assert!(later.reached > SimTime(300_000));
}

#[test]
fn a_confined_guardian_senses_the_newborn_but_regeneration_does_not_free_them() {
    let mut sim = fixture(1);
    let guardians = guardians(&sim);
    let guardian = guardians[0];
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::Engage { target: guardian },
    })
    .unwrap();
    let encounter = sim.observe().combat.unwrap().encounter;
    for _ in 0..4 {
        sim.submit(Submission {
            caller_sequence: sim.next_caller_sequence().unwrap(),
            command: Command::CombatTurn {
                encounter,
                movement: None,
                action: CombatAction::Attack { target: guardian },
            },
        })
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
    }
    sim.submit(Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: Command::CombatTurn {
            encounter,
            movement: None,
            action: CombatAction::Capture { target: guardian },
        },
    })
    .unwrap();
    sim.advance(SimDuration(6_000)).unwrap();
    let custody = sim
        .canonical_state()
        .state
        .combat
        .as_ref()
        .unwrap()
        .captivity[&guardian]
        .clone();
    kill_ruler(&mut sim);
    let result = sim.advance(SimDuration(900_000)).unwrap();
    let child = sim.canonical_state().state.ruler;
    assert_eq!(result.reason, StopReason::SuccessorBorn { ruler: child });
    assert_eq!(
        sim.canonical_state()
            .state
            .combat
            .as_ref()
            .unwrap()
            .captivity[&guardian],
        custody
    );
    assert_eq!(
        game_physiology::body(sim.canonical_state().components, guardian)
            .expect("body exists")
            .vitals
            .health,
        180_000
    );
    let search = &sim
        .canonical_state()
        .state
        .lineage
        .as_ref()
        .unwrap()
        .guardians[&guardian];
    assert_eq!(search.target, Some(child));
    assert_eq!(search.goal, GuardianGoal::Confined);
    assert_eq!(
        sim.debug_move_actor(guardian, AreaId(2)),
        Err(Error::Unavailable)
    );
    Simulation::load(
        &sim.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: true,
        },
    )
    .unwrap();
}

#[test]
fn actual_first_post_death_birth_changes_control_without_inheriting_assets() {
    let mut sim = fixture(1);
    let predecessor = kill_ruler(&mut sim);
    assert_eq!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .current(),
        None
    );
    let possessions: Vec<_> = sim
        .canonical_state()
        .state
        .inventories
        .values()
        .filter(|i| i.location == InventoryLocation::Carried(predecessor))
        .cloned()
        .collect();
    let result = sim.advance(SimDuration(900_000)).unwrap();
    let child = sim.canonical_state().state.ruler;
    assert_ne!(child, predecessor);
    assert_eq!(result.reached, SimTime(300_000));
    assert_eq!(result.reason, StopReason::SuccessorBorn { ruler: child });
    assert_eq!(sim.observe().life_stage, Some(LifeStage::Infant));
    let world = sim.canonical_state().state.lineage.as_ref().unwrap();
    assert_eq!(world.current(), Some(child));
    assert_eq!(
        world
            .incarnations
            .iter()
            .map(|l| l.person)
            .collect::<Vec<_>>(),
        vec![predecessor, child]
    );
    assert_eq!(world.incarnations[0].died_at, Some(SimTime(0)));
    assert_eq!(world.incarnations[1].bound_at, SimTime(300_000));
    for original in possessions {
        let now = &sim.canonical_state().state.inventories[&original.id];
        assert_eq!(now, &original);
        assert!(!now.access.contains(&child));
    }
    let newborn = sim
        .canonical_state()
        .state
        .people
        .iter()
        .find(|p| p.id == child)
        .unwrap();
    assert_eq!(
        sim.canonical_state().state.inventories[&newborn.inventory].quantity,
        Quantity(0)
    );
    assert!(
        game_characters::decisions::mind::<game_api::Command>(
            sim.canonical_state().components,
            child
        )
        .expect("mind exists")
        .values
        .is_empty()
    );
    assert!(
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Move {
                destination: AreaId(2)
            }
        })
        .is_err()
    );
}

#[test]
fn births_before_death_are_ineligible_and_an_unbounded_gap_manufactures_nothing() {
    let mut sim = fixture(1);
    sim.advance(SimDuration(300_000)).unwrap();
    let previous_child = game_lifecycle::all_lives(sim.canonical_state().components)
        .map(|(_, row)| row)
        .find(|l| !l.parents.is_empty())
        .unwrap()
        .id;
    let predecessor = kill_ruler(&mut sim);
    assert_ne!(sim.canonical_state().state.ruler, previous_child);
    let citizens: Vec<_> = sim
        .canonical_state()
        .state
        .people
        .iter()
        .filter(|p| p.role != Role::Guardian && p.id != predecessor)
        .map(|p| p.id)
        .collect();
    for citizen in citizens {
        // Stop any newly coupled conception using the physical cancellation path.
        sim.debug_actor_command(citizen, Command::Cancel).ok();
        sim.debug_set_personality(citizen, Personality::restful(), Aspiration::Comfort)
            .unwrap();
    }
    let count = sim.canonical_state().state.people.len();
    let result = sim.advance(SimDuration(86_400_000)).unwrap();
    assert_eq!(result.reason, StopReason::TargetReached);
    assert_eq!(sim.canonical_state().state.people.len(), count);
    assert_eq!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .current(),
        None
    );
    assert_eq!(sim.canonical_state().state.ruler, predecessor);
    assert!(
        sim.canonical_state()
            .events
            .iter()
            .any(|e| matches!(e.event, EventKind::Died { actor } if actor == previous_child))
    );
    assert!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .values()
            .all(|g| g.target.is_none() && g.sensed_location.is_none())
    );
}

#[test]
fn guardians_follow_the_connection_using_real_travel_and_real_food() {
    let mut sim = fixture(1);
    let guardians = guardians(&sim);
    for guardian in &guardians {
        sim.debug_move_actor(*guardian, AreaId(3)).unwrap();
    }
    kill_ruler(&mut sim);
    sim.advance(SimDuration(900_000)).unwrap();
    let child = sim.canonical_state().state.ruler;
    for guardian in &guardians {
        let person = sim
            .canonical_state()
            .state
            .people
            .iter()
            .find(|p| p.id == *guardian)
            .unwrap();
        assert_eq!(
            *game_spatial::location(
                sim.canonical_state().components,
                game_ecs::StableId((person.id).0)
            )
            .expect("location exists"),
            Location::InTransit {
                from: AreaId(3),
                to: AreaId(1),
                departed: SimTime(300_000),
                arrives: SimTime(480_000),
                road_start: None,
            }
        );
        let knowledge = &sim
            .canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians[guardian];
        assert_eq!(knowledge.target, Some(child));
        assert_eq!(knowledge.sensed_location, Some(Location::AtArea(AreaId(1))));
        assert_eq!(knowledge.goal, GuardianGoal::Searching);
    }
    quiet_other_caregivers(&mut sim, child);
    sim.submit(Submission {
        caller_sequence: 1,
        command: Command::Signal {
            preference: ChildhoodInterest::Food,
        },
    })
    .unwrap();
    sim.advance(SimDuration(179_999)).unwrap();
    assert!(guardians.iter().all(|id| {
        matches!(
            *game_spatial::location(sim.canonical_state().components, game_ecs::StableId(id.0))
                .expect("location exists"),
            Location::InTransit { .. }
        )
    }));
    sim.advance(SimDuration(60_001)).unwrap();
    let care = sim
        .canonical_state()
        .events
        .iter()
        .find(|event| {
            matches!(event.event,
                EventKind::Cared { caregiver, child: recipient, quantity: Quantity(1_000), .. }
                    if recipient == child && guardians.contains(&caregiver)
            )
        })
        .unwrap();
    assert_eq!(care.at, SimTime(540_000));
    assert!(
        game_lifecycle::life(sim.canonical_state().components, child)
            .expect("life exists")
            .caregivers
            .iter()
            .any(|id| guardians.contains(id))
    );
}

#[test]
fn destruction_recovers_in_place_without_changing_the_search_target() {
    let mut sim = fixture(1);
    let guardian = guardians(&sim)[0];
    sim.debug_move_actor(guardian, AreaId(3)).unwrap();
    sim.advance(SimDuration(180_000)).unwrap();
    // The living ruler may cause a return journey at this review. Cancel that
    // route at its current physical endpoint before arranging the next gap.
    sim.advance(SimDuration(180_000)).unwrap();
    kill_ruler(&mut sim);
    let location = game_spatial::location(
        sim.canonical_state().components,
        game_ecs::StableId((guardian).0),
    )
    .expect("location exists")
    .clone();
    let health = sim
        .canonical_state()
        .state
        .physical
        .as_ref()
        .unwrap()
        .rules
        .max_health;
    sim.debug_injure(guardian, health).unwrap();
    assert!(
        game_physiology::body(sim.canonical_state().components, guardian)
            .expect("body exists")
            .vitals
            .alive
    );
    sim.advance(SimDuration(60_000)).unwrap();
    let body =
        &game_physiology::body(sim.canonical_state().components, guardian).expect("body exists");
    assert_eq!(body.vitals.health, 60_000);
    assert_eq!(
        *game_spatial::location(
            sim.canonical_state().components,
            game_ecs::StableId((guardian).0)
        )
        .expect("location exists"),
        location
    );
    assert_eq!(
        sim.canonical_state()
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians[&guardian]
            .target,
        None
    );
    sim.advance(SimDuration(120_000)).unwrap();
    assert_eq!(
        game_physiology::body(sim.canonical_state().components, guardian)
            .expect("body exists")
            .vitals
            .health,
        health
    );
}

#[test]
fn waiting_and_pending_guardian_journeys_survive_saves_workers_queries_and_slices() {
    let mut a = fixture(1);
    for guardian in guardians(&a) {
        a.debug_move_actor(guardian, AreaId(3)).unwrap();
    }
    kill_ruler(&mut a);
    a.advance(SimDuration(130_000)).unwrap();
    let mut b = Simulation::load(
        &a.save().unwrap(),
        Execution {
            workers: 4,
            reverse_jobs: false,
        },
    )
    .unwrap();
    let before = a.canonical_snapshot().unwrap();
    for _ in 0..100 {
        a.observe();
    }
    assert_eq!(a.canonical_snapshot().unwrap(), before);
    let one = a.advance(SimDuration(170_000)).unwrap();
    for dt in [1, 29_999, 60_000, 79_999] {
        b.advance(SimDuration(dt)).unwrap();
    }
    let two = b.advance(SimDuration(1)).unwrap();
    assert_eq!(one.reason, two.reason);
    assert_eq!(a.canonical_state(), b.canonical_state());
    let mut restored = Simulation::load(
        &a.save().unwrap(),
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    a.advance(SimDuration(240_000)).unwrap();
    for dt in [30_000, 60_000, 1, 149_999] {
        restored.advance(SimDuration(dt)).unwrap();
    }
    assert_eq!(a.canonical_state(), restored.canonical_state());
}
