use crate::{CanonicalState, Execution, GameHost, Simulation};
use game_core::{SimDuration, SimTime};
use game_sim::ClockWorld;

#[test]
fn one_host_snapshot_and_metadata_only_runtime_survive_clone_and_restore() {
    let mut sim = Simulation::farming_fixture(77, Execution::default()).unwrap();
    let state = sim.canonical_state();
    let json = serde_json::to_value(&state).unwrap();
    assert!(
        json["components"]["columns"]["agriculture/field"]
            .as_array()
            .unwrap()
            .len()
            >= 2
    );
    let agriculture = json["state"]["agriculture"].as_object().unwrap();
    assert!(!agriculture.contains_key("components"));
    assert!(!agriculture.contains_key("fields"));
    assert!(!agriculture.contains_key("crops"));
    assert!(!agriculture.contains_key("soils"));
    let cloned = sim.canonical_snapshot().unwrap();
    let host =
        super::super::GameHost::restore(super::schema().unwrap(), cloned, |_| Ok(())).unwrap();
    assert!(sim.agriculture_queries.refresh(&host.storage).is_err());
    let mut rebound = Simulation::from_host(
        host,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    let mut restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    assert_eq!(rebound.canonical_state(), restored.canonical_state());
    rebound.advance(SimDuration(120_000)).unwrap();
    restored.advance(SimDuration(120_000)).unwrap();
    assert_eq!(rebound.canonical_state(), restored.canonical_state());
}

#[test]
fn body_activity_and_workshop_jobs_are_independent_canonical_columns() {
    let mut sim = Simulation::survival_fixture(13, Execution::default()).unwrap();
    let ruler = sim.host.state.ruler;
    let site = game_workshop::sites(&sim.host.storage)
        .map(|(id, _)| id)
        .next()
        .unwrap();
    sim.debug_actor_command(ruler, game_api::Command::Work { site })
        .unwrap();
    let json = serde_json::to_value(sim.canonical_state()).unwrap();
    assert!(
        !json["state"]["physical"]
            .as_object()
            .unwrap()
            .contains_key("actors")
    );
    assert_eq!(
        json["components"]["columns"]["physiology/body"]
            .as_array()
            .unwrap()
            .len(),
        sim.host.state.people.len()
    );
    assert_eq!(
        json["components"]["columns"]["coordination/activity"]
            .as_array()
            .unwrap()
            .len(),
        sim.host.state.people.len()
    );
    assert!(
        !json["components"]["columns"]["workshop/job"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    assert!(game_workshop::job(&restored.host.storage, ruler).is_some());
}

#[test]
fn obsolete_actor_payload_is_rejected_in_the_physical_metadata_header() {
    let sim = Simulation::survival_fixture(16, Execution::default()).unwrap();
    let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    saved["state"]["state"]["physical"]["actors"] = serde_json::json!({});
    assert!(Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()).is_err());
}

#[test]
fn stale_physical_trait_development_fails_before_publication() {
    let mut sim = Simulation::genetics_fixture(85, Execution::default()).unwrap();
    sim.physical_query.refresh(&sim.host.storage).unwrap();
    let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
    let actor = sim.host.state.ruler;
    game_physiology::traits::development_mut(&mut sim.host.storage, actor)
        .unwrap()
        .physical_training_milli += 1;
    let before = sim.canonical_snapshot().unwrap();
    assert!(sim.commit_prepared_step(prepared).is_err());
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn stale_expressed_alleles_fail_before_publication() {
    let mut sim = Simulation::genetics_fixture(86, Execution::default()).unwrap();
    let actor = sim.host.state.ruler;
    let locus = game_genetics::LocusId(
        sim.host
            .state
            .physical_traits
            .as_ref()
            .unwrap()
            .definitions
            .traits
            .iter()
            .find(|t| t.kind == game_physiology::traits::PhysicalTrait::Metabolism)
            .unwrap()
            .inheritance
            .locus
            .clone(),
    );
    let opening = sim
        .host
        .state
        .genetics
        .as_ref()
        .unwrap()
        .alleles(&sim.host.storage, actor, &locus)
        .unwrap();
    sim.physical_query.refresh(&sim.host.storage).unwrap();
    let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
    let new_value = if opening == [0; 2] { 1_000 } else { 0 };
    sim.host
        .state
        .genetics
        .as_mut()
        .unwrap()
        .set_alleles(&mut sim.host.storage, actor, &locus, [new_value; 2])
        .unwrap();
    let before = sim.canonical_snapshot().unwrap();
    assert!(sim.commit_prepared_step(prepared).is_err());
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn invalid_final_founder_traits_leave_admission_inert() {
    let mut sim = Simulation::skills_fixture(87, Execution::default()).unwrap();
    let last = sim.host.state.people.last().unwrap().id;
    game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, last)
        .unwrap()
        .personality
        .empathy = 1_001;
    let before = sim.canonical_snapshot().unwrap();
    let content =
        crate::genetics::Content::read(crate::genetics::Config::default().content_dir).unwrap();
    assert!(sim.initialize_trait_content(content).is_err());
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn genetic_and_trait_rows_are_authoritative_and_orphans_fail_restore() {
    let sim = Simulation::genetics_fixture(84, Execution::default()).unwrap();
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    for header in ["genetics", "physical_traits", "temperament"] {
        assert!(json["state"][header].get("people").is_none());
    }
    assert!(
        json["state"]["state"]["genetics"]
            .get("pregnancies")
            .is_none()
    );
    for column in [
        "genetics/genome",
        "physiology/development",
        "characters/temperament",
    ] {
        assert_eq!(
            json["state"]["components"]["columns"][column]
                .as_array()
                .unwrap()
                .len(),
            sim.host.state.people.len()
        );
        let mut malformed = json.clone();
        malformed["state"]["components"]["columns"][column]
            .as_array_mut()
            .unwrap()
            .remove(0);
        assert!(
            Simulation::load(
                &serde_json::to_vec(&malformed).unwrap(),
                Execution::default()
            )
            .is_err()
        );
    }
    let mut orphaned = json;
    for header in ["genetics", "physical_traits", "temperament", "heredity"] {
        orphaned["state"][header] = serde_json::Value::Null;
    }
    assert!(
        Simulation::load(
            &serde_json::to_vec(&orphaned).unwrap(),
            Execution::default()
        )
        .is_err()
    );
    assert_eq!(
        sim.canonical_state(),
        Simulation::load(&saved, Execution::default())
            .unwrap()
            .canonical_state()
    );
}

#[test]
fn pending_npc_death_refreshes_queries_after_occupancy_removal() {
    let mut sim = Simulation::combat_fixture(89, Execution::default()).unwrap();
    sim.initialize_health(game_health::Rules::fixture())
        .unwrap();
    let ruler = sim.host.state.ruler;
    let enemy = sim.host.state.people[3].id;
    let area = game_spatial::location(&sim.host.storage, game_ecs::StableId((ruler).0))
        .unwrap()
        .cell()
        .unwrap();
    let timber = sim
        .debug_add_inventory(
            game_api::Good::Timber,
            crate::InventoryLocation::Area(area),
            game_api::Quantity(10_000),
            game_api::Quantity(10_000),
            std::collections::BTreeSet::from([ruler]),
        )
        .unwrap();
    let shelter = sim
        .seed_shelter(
            area,
            ruler,
            std::collections::BTreeSet::from([ruler]),
            1,
            650,
            timber,
            game_api::Quantity(10_000),
            SimTime(0),
        )
        .unwrap();
    sim.set_initial_shelter_occupancy(ruler, shelter).unwrap();
    let encounter = sim
        .debug_start_encounter(vec![(ruler, 0), (enemy, 1)])
        .unwrap();
    for __stock_id in game_cargo::inventories(&sim.host.storage)
        .map(|(id, _)| id)
        .collect::<Vec<_>>()
    {
        let stock =
            game_cargo::inventory_mut(&mut sim.host.storage, __stock_id).expect("inventory exists");
        if stock.good == game_api::Good::Binding
            && stock.location == crate::InventoryLocation::Carried(enemy)
        {
            stock.quantity = game_api::Quantity(0);
        }
    }
    let damage = sim.body_rules(ruler).max_health - 1;
    sim.debug_injure(ruler, damage).unwrap();
    let combat = sim.host.state.combat.as_mut().unwrap();
    let map = combat.encounters[&encounter].map;
    combat.map_mut(map).unwrap().obstacles.clear();
    let battle = combat.encounters.get_mut(&encounter).unwrap();
    battle.fighters.get_mut(&ruler).unwrap().position = game_api::Point { x: 5_000, y: 5_000 };
    battle.fighters.get_mut(&enemy).unwrap().position = game_api::Point { x: 6_000, y: 5_000 };
    // An authored pending NPC turn runs through the normal advance path.
    battle.order = vec![enemy, ruler];
    battle.cursor = 0;
    assert!(game_health::occupancy(&sim.host.storage, ruler).is_some());
    let advanced = sim.advance(game_api::SimDuration(1)).unwrap();
    assert_eq!(advanced.reached, SimTime(1));
    assert!(game_health::occupancy(&sim.host.storage, ruler).is_none());
    assert!(sim.host.events().iter().any(|event| matches!(event.event,game_api::EventKind::Struck {actor,target,..} if actor == enemy && target == ruler)));
    assert!(
        sim.host
            .events()
            .iter()
            .any(|event| matches!(event.event,game_api::EventKind::Died {actor} if actor == ruler))
    );
}

#[test]
fn work_and_foraging_rows_are_saved_outside_runtime_headers() {
    let mut farm = Simulation::farming_fixture(95, Execution::default()).unwrap();
    let (field, area) = farm
        .host
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .fields(&farm.host.storage)
        .map(|(id, row)| (id, row.area))
        .last()
        .unwrap();
    let ruler = farm.host.state.ruler;
    farm.debug_move_actor(ruler, area).unwrap();
    if let game_api::Location::InTransit { arrives, .. } =
        *game_spatial::location(&farm.host.storage, game_ecs::StableId((ruler).0)).unwrap()
    {
        farm.advance(game_api::SimDuration(arrives.0 - farm.host.clock().time.0))
            .unwrap();
    }
    farm.debug_actor_command(
        ruler,
        game_api::Command::Cultivate {
            field,
            operation: game_api::FarmOperation::Prepare,
        },
    )
    .unwrap();
    let saved = farm.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert!(json["state"]["state"]["agriculture"].get("jobs").is_none());
    assert!(
        !json["state"]["components"]["columns"]["agriculture/cultivation"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        farm.canonical_state(),
        Simulation::load(&saved, Execution::default())
            .unwrap()
            .canonical_state()
    );
    let eco = Simulation::ecology_fixture(96, Execution::default()).unwrap();
    let saved = eco.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    for old in [
        "jobs",
        "known",
        "known_plots",
        "observed_areas",
        "next_request",
    ] {
        assert!(json["state"]["state"]["ecology"].get(old).is_none());
    }
    assert!(
        !json["state"]["components"]["columns"]["ecology/knowledge"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        eco.canonical_state(),
        Simulation::load(&saved, Execution::default())
            .unwrap()
            .canonical_state()
    );
}

#[test]
fn mind_column_is_authoritative_and_missing_person_rows_fail_restore() {
    let sim = Simulation::autonomy_fixture(93, Execution::default()).unwrap();
    let saved = sim.save().unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert!(json["state"]["state"]["decisions"].get("minds").is_none());
    assert_eq!(
        json["state"]["components"]["columns"]["characters/mind"]
            .as_array()
            .unwrap()
            .len(),
        sim.host.state.people.len()
    );
    assert_eq!(
        sim.canonical_state(),
        Simulation::load(&saved, Execution::default())
            .unwrap()
            .canonical_state()
    );
    json["state"]["components"]["columns"]["characters/mind"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(Simulation::load(&serde_json::to_vec(&json).unwrap(), Execution::default()).is_err());
}

#[test]
fn initial_mind_admission_rejects_a_late_bad_row_without_publishing() {
    let source = Simulation::autonomy_fixture(94, Execution::default()).unwrap();
    let mut sim = Simulation::survival_fixture(94, Execution::default()).unwrap();
    let mut rows: std::collections::BTreeMap<_, _> =
        game_characters::decisions::minds::<game_api::Command>(&source.host.storage)
            .map(|(id, mind)| (id, mind.clone()))
            .collect();
    rows.last_entry().unwrap().get_mut().personality.empathy = 1_001;
    let before = sim.canonical_snapshot().unwrap();
    assert!(sim.initialize_decision_people(rows).is_err());
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn lifecycle_rows_are_authoritative_and_missing_or_orphan_lives_fail_restore() {
    let sim = Simulation::life_fixture(90, Execution::default()).unwrap();
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert!(json["state"]["state"]["development"].get("lives").is_none());
    assert!(
        json["state"]["state"]["development"]
            .get("pregnancies")
            .is_none()
    );
    assert_eq!(
        json["state"]["components"]["columns"]["lifecycle/life"]
            .as_array()
            .unwrap()
            .len(),
        sim.host.state.people.len()
    );
    assert_eq!(
        sim.canonical_state(),
        Simulation::load(&saved, Execution::default())
            .unwrap()
            .canonical_state()
    );
    let mut missing = json.clone();
    missing["state"]["components"]["columns"]["lifecycle/life"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(
        Simulation::load(&serde_json::to_vec(&missing).unwrap(), Execution::default()).is_err()
    );
    let mut orphan = Simulation::survival_fixture(92, Execution::default()).unwrap();
    let mut life = game_lifecycle::life(&sim.host.storage, sim.host.state.ruler)
        .unwrap()
        .clone();
    life.id = orphan.host.state.ruler;
    game_lifecycle::insert_life(&mut orphan.host.storage, life).unwrap();
    assert!(
        matches!(Simulation::load(&orphan.save().unwrap(), Execution::default()), Err(game_api::Error::InvalidSave(message)) if message == "lifecycle components require lifecycle runtime")
    );
}

#[test]
fn stale_lifecycle_actions_leave_rows_clock_and_events_unchanged() {
    let mut sim = Simulation::life_fixture(91, Execution::default()).unwrap();
    let actor = sim.host.state.people[3].id;
    let partner = sim.host.state.people[4].id;
    sim.debug_actor_command(actor, game_api::Command::Conceive { partner })
        .unwrap();
    let pregnancy = game_lifecycle::all_pregnancies(&sim.host.storage)
        .next()
        .unwrap()
        .0;
    let age = sim
        .host
        .state
        .development
        .as_ref()
        .unwrap()
        .prepare_age(
            &sim.host.storage,
            actor,
            sim.host.clock().time,
            true,
            0,
            true,
        )
        .unwrap();
    game_lifecycle::life_mut(&mut sim.host.storage, actor)
        .unwrap()
        .preference = Some((game_api::ChildhoodInterest::Company, SimTime(1)));
    let before = sim.canonical_snapshot().unwrap();
    assert!(
        age.commit(
            sim.host.state.development.as_ref().unwrap(),
            &mut sim.host.storage
        )
        .is_err()
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
    let interruption = sim
        .host
        .state
        .development
        .as_ref()
        .unwrap()
        .prepare_interruption(&sim.host.storage, actor, false, [pregnancy]);
    game_lifecycle::pregnancy_mut(&mut sim.host.storage, pregnancy)
        .unwrap()
        .state = game_lifecycle::PregnancyState::Ended;
    let before = sim.canonical_snapshot().unwrap();
    assert!(
        interruption
            .commit(
                sim.host.state.development.as_ref().unwrap(),
                &mut sim.host.storage
            )
            .is_err()
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn health_knowledge_without_the_health_runtime_is_rejected() {
    let mut sim = Simulation::survival_fixture(88, Execution::default()).unwrap();
    assert!(sim.host.state.health.is_none());
    game_health::insert_knowledge(
        &mut sim.host.storage,
        sim.host.state.ruler,
        game_health::Knowledge::default(),
    )
    .unwrap();
    assert!(
        matches!(Simulation::load(&sim.save().unwrap(), Execution::default()),
        Err(game_api::Error::InvalidSave(message)) if message == "health components require health runtime")
    );
}

#[test]
fn health_columns_are_authoritative_and_orphans_fail_restore() {
    let sim = Simulation::health_fixture(82, Execution::default()).unwrap();
    let mut saved: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    let header = saved["state"]["state"]["health"].as_object().unwrap();
    for obsolete in [
        "clinical",
        "jobs",
        "water",
        "shelters",
        "occupancy",
        "known",
        "offers",
    ] {
        assert!(!header.contains_key(obsolete));
    }
    for column in [
        "health/clinical",
        "health/job",
        "health/water",
        "health/shelter",
        "health/occupancy",
        "health/knowledge",
        "health/offers",
    ] {
        assert!(saved["state"]["components"]["columns"][column].is_array());
    }
    assert_eq!(
        saved["state"]["components"]["columns"]["health/clinical"]
            .as_array()
            .unwrap()
            .len(),
        sim.host.state.people.len()
    );
    let restored = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    saved["state"]["state"]["health"] = serde_json::Value::Null;
    assert!(Simulation::load(&serde_json::to_vec(&saved).unwrap(), Execution::default()).is_err());
}

#[test]
fn stale_clinical_and_health_job_inputs_fail_before_publication() {
    for change_job in [false, true] {
        let mut sim = Simulation::health_fixture(83, Execution::default()).unwrap();
        let actor = sim.host.state.ruler;
        let area = game_spatial::location(&sim.host.storage, game_ecs::StableId((actor).0))
            .unwrap()
            .cell()
            .unwrap();
        game_health::insert_job(
            &mut sim.host.storage,
            game_health::Job {
                actor,
                area,
                completes: SimTime(60_000),
                work: game_health::Work::Treat {
                    patient: actor,
                    skill_ms: 0,
                    elapsed: 0,
                },
            },
        )
        .unwrap();
        sim.host
            .storage
            .set_activity(
                actor,
                game_api::Activity::HealthWork {
                    kind: game_api::HealthWorkKind::Treat,
                    completes: SimTime(60_000),
                },
            )
            .unwrap();
        sim.physical_query.refresh(&sim.host.storage).unwrap();
        let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
        if change_job {
            game_health::job_mut(&mut sim.host.storage, actor)
                .unwrap()
                .completes = SimTime(60_001);
        } else {
            game_health::clinical_mut(&mut sim.host.storage, actor)
                .unwrap()
                .thirst += 1;
        }
        let before = sim.canonical_snapshot().unwrap();
        assert!(sim.commit_prepared_step(prepared).is_err());
        assert_eq!(before, sim.canonical_snapshot().unwrap());
    }
}

#[test]
fn a_stale_body_batch_fails_before_clock_or_events_are_published() {
    let mut sim = Simulation::survival_fixture(14, Execution::default()).unwrap();
    sim.physical_query.refresh(&sim.host.storage).unwrap();
    let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
    game_physiology::body_mut(&mut sim.host.storage, sim.host.state.ruler)
        .unwrap()
        .vitals
        .hunger += 1;
    let before = sim.canonical_snapshot().unwrap();
    assert_eq!(
        sim.commit_prepared_step(prepared),
        Err(game_core::Error::InvalidSave(
            "stale physical component batch".into()
        ))
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn physical_batch_guards_opening_population_membership() {
    let mut sim = Simulation::survival_fixture(15, Execution::default()).unwrap();
    sim.physical_query.refresh(&sim.host.storage).unwrap();
    let prepared = sim.prepare_world_step(SimTime(1)).unwrap();
    let mut newcomer = sim.host.state.people[0].clone();
    newcomer.id = game_api::PersonId(sim.host.allocate_id().unwrap().0);
    newcomer.role = game_api::Role::Citizen;
    let body = game_physiology::body(&sim.host.storage, sim.host.state.ruler)
        .unwrap()
        .clone();
    game_physiology::insert_body(&mut sim.host.storage, newcomer.id, body).unwrap();
    let location = game_spatial::location(
        &sim.host.storage,
        game_ecs::StableId((sim.host.state.ruler).0),
    )
    .unwrap()
    .clone();
    game_spatial::set_location(
        &mut sim.host.storage,
        game_ecs::StableId((newcomer.id).0),
        location,
    )
    .unwrap();
    sim.host
        .storage
        .set_activity(newcomer.id, game_api::Activity::Idle)
        .unwrap();
    let id = newcomer.id;
    let index = sim.host.state.people.len();
    sim.host.state.people.push(newcomer);
    sim.person_index.insert(id, index);
    let inventory = sim
        .debug_add_inventory(
            game_api::Good::Food,
            crate::InventoryLocation::Carried(id),
            game_api::Quantity(0),
            game_api::Quantity(10_000),
            std::collections::BTreeSet::from([id]),
        )
        .unwrap();
    sim.host.state.people[index].inventory = inventory;
    Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
    let before = sim.canonical_snapshot().unwrap();
    assert_eq!(
        sim.commit_prepared_step(prepared),
        Err(game_core::Error::InvalidSave(
            "stale physical component batch".into()
        ))
    );
    assert_eq!(before, sim.canonical_snapshot().unwrap());
}

#[test]
fn spatial_column_is_the_only_person_position_and_bad_graphs_fail_restore() {
    let sim = Simulation::kernel_fixture(17, 5, Execution::default()).unwrap();
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert!(
        json["state"]["state"]["people"]
            .as_array()
            .unwrap()
            .iter()
            .all(|person| person.get("location").is_none())
    );
    assert_eq!(
        json["state"]["components"]["columns"]["spatial/location"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    let first = sim.host.state.people[0].id;
    let second = sim.host.state.people[1].id;
    let mut state = GameHost::restore(
        super::schema().unwrap(),
        sim.canonical_snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    game_spatial::set_location(
        &mut state.storage,
        game_ecs::StableId((first).0),
        game_api::Location::Carried { carrier: second },
    )
    .unwrap();
    game_spatial::set_location(
        &mut state.storage,
        game_ecs::StableId((second).0),
        game_api::Location::Carried { carrier: first },
    )
    .unwrap();
    let mut corrupt = json.clone();
    corrupt["state"] = serde_json::to_value(state.snapshot().unwrap()).unwrap();
    assert!(
        Simulation::load(&serde_json::to_vec(&corrupt).unwrap(), Execution::default()).is_err()
    );
    let mut missing = GameHost::restore(
        super::schema().unwrap(),
        sim.canonical_snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    game_spatial::remove_location(&mut missing.storage, game_ecs::StableId(first.0)).unwrap();
    corrupt["state"] = serde_json::to_value(missing.snapshot().unwrap()).unwrap();
    assert!(
        Simulation::load(&serde_json::to_vec(&corrupt).unwrap(), Execution::default()).is_err()
    );
    assert_eq!(sim.save().unwrap(), saved);
}

#[test]
fn workshop_sites_roundtrip_only_as_shared_rows_and_require_physical_metadata() {
    let sim = Simulation::survival_fixture(18, Execution::default()).unwrap();
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    assert!(
        !json["state"]["state"]["physical"]
            .as_object()
            .unwrap()
            .contains_key("sites")
    );
    assert_eq!(
        json["state"]["components"]["columns"]["workshop/site"]
            .as_array()
            .unwrap()
            .len(),
        game_workshop::sites(&sim.host.storage).count()
    );
    let restored = Simulation::load(
        &saved,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());

    let mut kernel = Simulation::kernel_fixture(19, 5, Execution::default()).unwrap();
    let person = &kernel.host.state.people[0];
    let id = game_api::SiteId(person.id.0);
    let stock = person.inventory;
    let chosen_recipe = kernel
        .fixture_recipe(
            stock,
            stock,
            game_api::Quantity(1),
            game_api::Quantity(1),
            SimDuration(1),
        )
        .unwrap();
    game_workshop::insert_site(
        &mut kernel.host.storage,
        game_workshop::Site {
            recipe: chosen_recipe,
            id,
            area: kernel.host.state.areas[0].id,
            input: stock,

            tool: stock,
            output: stock,

            access: std::collections::BTreeSet::new(),
            last_granted: None,
        },
    )
    .unwrap();
    assert!(Simulation::load(&kernel.save().unwrap(), Execution::default()).is_err());
}

#[test]
fn personal_skills_are_saved_as_components_and_require_their_runtime() {
    let sim = Simulation::skills_fixture(20, Execution::default()).unwrap();
    let saved = sim.save().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&saved).unwrap();
    let metadata = json["state"]["state"]["skills"].as_object().unwrap();
    assert!(!metadata.contains_key("records"));
    assert!(!metadata.contains_key("semantic_ticks"));
    assert_eq!(
        json["state"]["components"]["columns"]["skills/personal"]
            .as_array()
            .unwrap()
            .len(),
        sim.host.state.people.len()
    );
    let restored = Simulation::load(
        &saved,
        Execution {
            workers: 2,
            reverse_jobs: true,
        },
    )
    .unwrap();
    assert_eq!(sim.canonical_state(), restored.canonical_state());
    let mut missing_runtime = GameHost::restore(
        super::schema().unwrap(),
        sim.canonical_snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    missing_runtime.state.skills = None;
    assert!(crate::skills::validate(&CanonicalState::from_host(&missing_runtime)).is_err());
}

#[test]
fn land_soil_keys_match_one_land_use_and_reject_missing_or_orphan_rows() {
    let mut sim = Simulation::ecology_fixture(21, Execution::default()).unwrap();
    let plot = sim
        .debug_add_plot(
            sim.host.state.areas[1].id,
            None,
            sim.host
                .state
                .people
                .iter()
                .map(|person| person.id)
                .collect(),
            900,
        )
        .unwrap();
    sim.host.storage.validate_land().unwrap();
    let json = serde_json::to_value(sim.canonical_state()).unwrap();
    assert!(
        json["components"]["columns"]
            .get("agriculture/soil")
            .is_none()
    );
    assert!(json["components"]["columns"]["land/soil"].is_array());

    let mut missing = GameHost::restore(
        super::schema().unwrap(),
        sim.canonical_snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    game_land::remove_soil(&mut missing.storage, plot).unwrap();
    assert!(missing.storage.validate_land().is_err());

    let mut orphan = GameHost::restore(
        super::schema().unwrap(),
        sim.canonical_snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    let person = game_core::SiteId(orphan.state.people[0].id.0);
    game_land::insert_soil(
        &mut orphan.storage,
        person,
        game_land::Soil {
            fertility: 500,
            water: 1,
            capacity: 10,
        },
    )
    .unwrap();
    assert!(orphan.storage.validate_land().is_err());

    let mut ambiguous = GameHost::restore(
        super::schema().unwrap(),
        sim.canonical_snapshot().unwrap(),
        |_| Ok(()),
    )
    .unwrap();
    let field = ambiguous
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .fields(&ambiguous.storage)
        .next()
        .unwrap()
        .1
        .clone();
    let mut duplicate = field;
    duplicate.id = plot;
    ambiguous
        .state
        .agriculture
        .as_ref()
        .unwrap()
        .insert_field(&mut ambiguous.storage, duplicate)
        .unwrap();
    assert!(ambiguous.storage.validate_land().is_err());
    let mut envelope: serde_json::Value = serde_json::from_slice(&sim.save().unwrap()).unwrap();
    for invalid in [missing, orphan, ambiguous] {
        envelope["state"] = serde_json::to_value(invalid.snapshot().unwrap()).unwrap();
        assert!(
            Simulation::load(
                &serde_json::to_vec(&envelope).unwrap(),
                Execution::default()
            )
            .is_err()
        );
    }
}
#[test]
fn generic_host_fields_exist_once_and_metadata_shadows_fail_restore() {
    let mut sim = Simulation::survival_fixture(37, Execution::default()).unwrap();
    sim.submit(game_api::Submission {
        caller_sequence: sim.next_caller_sequence().unwrap(),
        command: game_api::Command::Move {
            destination: game_api::AreaId(2),
        },
    })
    .unwrap();
    let before = sim.save().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let host = &saved["state"];
    let metadata = host["state"].as_object().unwrap();
    for field in [
        "components",
        "seed",
        "time",
        "next_id",
        "next_event",
        "events",
    ] {
        assert!(host.get(field).is_some());
        assert!(!metadata.contains_key(field));
        let mut forged = saved.clone();
        forged["state"]["state"][field] = host[field].clone();
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "metadata must reject the host field {field}"
        );
        assert_eq!(sim.save().unwrap(), before);
    }
    assert!(host["components"]["columns"]["physiology/body"].is_array());
    for record in host["events"].as_array().unwrap() {
        assert!(record.get("sequence").is_some());
        assert!(record.get("at").is_some());
        assert!(record.get("event").is_some());
        assert!(record.get("id").is_none());
        assert!(record.get("time").is_none());
        assert!(record.get("kind").is_none());
    }
}
#[test]
fn native_inventory_columns_are_canonical_and_reject_identity_and_reference_forgery() {
    let sim = Simulation::survival_fixture(31, Execution::default()).unwrap();
    let before = sim.save().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    assert!(saved["state"]["state"].get("inventories").is_none());
    let rows = saved["state"]["components"]["columns"]["cargo/inventory"]
        .as_array()
        .unwrap();
    assert!(!rows.is_empty());
    let ids: Vec<_> = rows
        .iter()
        .map(|row| row["entity"].as_u64().unwrap())
        .collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(rows.iter().all(|row| row["entity"] == row["value"]["id"]));
    let restored = Simulation::load(&before, Execution::default()).unwrap();
    assert_eq!(restored.save().unwrap(), before);
    for attack in 0..5 {
        let mut forged = saved.clone();
        let row = &mut forged["state"]["components"]["columns"]["cargo/inventory"][0]["value"];
        match attack {
            0 => row["id"] = serde_json::json!(999999),
            1 => row["location"] = serde_json::json!({"Carried": 999999}),
            2 => row["access"] = serde_json::json!([999999]),
            3 => row["spoils_at"] = serde_json::json!(999999),
            _ => {
                forged["state"]["state"]["inventories"] = serde_json::json!({});
            }
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "stock attack {attack}"
        );
        assert_eq!(sim.save().unwrap(), before);
    }
}

#[test]
fn native_property_columns_restore_and_reject_metadata_shadows_and_orphan_rows() {
    let sim = Simulation::property_fixture(71, Execution::default()).unwrap();
    let before = sim.save().unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&before).unwrap();
    let metadata = &saved["state"]["state"]["property"];
    assert_eq!(metadata.as_object().unwrap().len(), 1);
    assert!(metadata.get("last_event").is_some());
    let columns = &saved["state"]["components"]["columns"];
    for name in [
        "claim",
        "incident",
        "provenance",
        "attitude",
        "knowledge",
        "activity",
        "requests",
    ] {
        assert!(columns[format!("property/{name}")].is_array());
    }
    assert_eq!(
        columns["property/provenance"].as_array().unwrap().len(),
        columns["cargo/inventory"].as_array().unwrap().len()
    );
    let restored = Simulation::load(&before, Execution::default()).unwrap();
    assert_eq!(restored.canonical_state(), sim.canonical_state());
    assert_eq!(restored.save().unwrap(), before);
    for field in [
        "claims",
        "incidents",
        "portions",
        "attitudes",
        "knowledge",
        "incident_knowledge",
        "pending",
        "reports",
        "next_search",
    ] {
        assert!(metadata.get(field).is_none());
        let mut forged = saved.clone();
        forged["state"]["state"]["property"][field] = serde_json::json!({});
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "runtime shadow {field}"
        );
    }
    let area = sim.host.state.areas[0].id.0;
    for attack in 0..8 {
        let mut forged = saved.clone();
        let columns = &mut forged["state"]["components"]["columns"];
        match attack {
            0 => forged["state"]["state"]["property"] = serde_json::Value::Null,
            1 => columns["property/claim"][0]["value"]["id"] = serde_json::json!(999999),
            2 => columns["property/claim"][0]["value"]["claimant"] = serde_json::json!(area),
            3 => {
                columns["property/provenance"][0]["entity"] =
                    serde_json::json!(sim.host.state.ruler.0)
            }
            4 => columns["property/attitude"][0]["entity"] = serde_json::json!(area),
            5 => columns["property/knowledge"][0]["entity"] = serde_json::json!(area),
            6 => columns["property/activity"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity":area,"value":{"pending":null,"report":null}})),
            _ => columns["property/requests"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"entity":area,"value":{"next_search":0}})),
        }
        assert!(
            Simulation::load(&serde_json::to_vec(&forged).unwrap(), Execution::default()).is_err(),
            "property reference attack {attack}"
        );
        assert_eq!(sim.save().unwrap(), before);
    }
}
