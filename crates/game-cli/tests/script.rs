use game_api::*;
use game_assembly::*;

#[test]
fn text_adapter_and_direct_api_produce_identical_logical_results() {
    let execution = Execution::default();
    let mut direct = Simulation::new("kernel-fixture", 1, execution.clone()).unwrap();
    let mut text = Simulation::new("kernel-fixture", 1, execution.clone()).unwrap();
    direct
        .submit(Submission {
            caller_sequence: 1,
            command: Command::Move {
                destination: AreaId(2),
            },
        })
        .unwrap();
    direct.advance(SimDuration(180_003)).unwrap();
    for line in [
        "status",
        "move 2",
        "run 119999ms",
        "debug",
        "run 1ms",
        "status",
        "run 60003ms",
    ] {
        game_cli::execute(&mut text, line, &execution).unwrap();
    }
    assert_eq!(direct.canonical_state(), text.canonical_state());
    let before = text.canonical_state().clone();
    assert!(game_cli::execute(&mut text, "move 999", &execution).is_err());
    assert!(game_cli::execute(&mut text, "run 18446744073709551615d", &execution).is_err());
    assert_eq!(&before, text.canonical_state());
}

#[test]
fn cli_process_accepts_a_noninteractive_script() {
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_game-cli"))
        .args([
            "--scenario",
            "kernel-fixture",
            "--seed",
            "1",
            "--workers",
            "2",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"move 2\nrun 119999ms\nstatus\nrun 1ms\nstatus\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    let views: Vec<PlayerView> = output
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    assert_eq!(views.len(), 2);
    assert!(matches!(
        views[0].location,
        Location::InTransit { to: AreaId(2), .. }
    ));
    assert_eq!(views[1].time, SimTime(120_000));
    assert_eq!(views[1].location, Location::AtArea(AreaId(2)));
}

#[test]
fn newborn_care_is_reachable_through_normal_observations_and_the_text_adapter() {
    let execution = Execution::default();
    let mut sim = Simulation::life_fixture(1, execution.clone()).unwrap();
    game_cli::execute(&mut sim, "run 5m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    let child = view
        .nearby_life_stages
        .iter()
        .find(|(_, stage)| *stage == LifeStage::Infant)
        .unwrap()
        .0;
    let source = view
        .available_commands
        .iter()
        .find_map(|c| {
            if let Command::Care {
                child: target,
                source,
            } = c
            {
                (*target == child).then_some(*source)
            } else {
                None
            }
        })
        .unwrap();
    game_cli::execute(
        &mut sim,
        &format!("care {} {}", child.0, source.0),
        &execution,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    let ruler = sim.canonical_state().ruler;
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.kind, EventKind::Cared { caregiver, child: target, .. } if caregiver == ruler && target == child)));
}

#[test]
fn save_is_atomic_even_when_the_target_has_the_old_temporary_extension() {
    let execution = Execution::default();
    let mut sim = Simulation::new("kernel-fixture", 1, execution.clone()).unwrap();
    let root = std::env::temp_dir().join(format!("legacy-cli-save-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("world.save-tmp");
    std::fs::write(&path, b"old save").unwrap();
    game_cli::execute(&mut sim, &format!("save {}", path.display()), &execution).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        sim.canonical_state(),
        Simulation::load(&bytes, execution.clone())
            .unwrap()
            .canonical_state()
    );
    let directory = root.join("not-a-file");
    std::fs::create_dir_all(&directory).unwrap();
    assert!(
        game_cli::execute(
            &mut sim,
            &format!("save {}", directory.display()),
            &execution
        )
        .is_err()
    );
    assert!(directory.is_dir());
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn an_encounter_is_playable_through_the_text_client_and_ordinary_views() {
    let execution = Execution::default();
    let mut sim = Simulation::combat_fixture(1, execution.clone()).unwrap();
    let target = sim
        .observe()
        .nearby_people
        .iter()
        .find(|(_, role)| *role == Role::Citizen)
        .unwrap()
        .0;
    game_cli::execute(&mut sim, &format!("engage {}", target.0), &execution).unwrap();
    let view: Option<TacticalView> =
        serde_json::from_str(&game_cli::execute(&mut sim, "combat", &execution).unwrap()).unwrap();
    let encounter = view.unwrap().encounter;
    for _ in 0..3 {
        game_cli::execute(
            &mut sim,
            &format!("turn {} attack {}", encounter.0, target.0),
            &execution,
        )
        .unwrap();
        game_cli::execute(&mut sim, "run 6s", &execution).unwrap();
    }
    game_cli::execute(
        &mut sim,
        &format!("turn {} capture {}", encounter.0, target.0),
        &execution,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 6s", &execution).unwrap();
    assert_eq!(sim.time(), SimTime(24_000));
    assert!(
        sim.canonical_state()
            .combat
            .as_ref()
            .unwrap()
            .captivity
            .contains_key(&target)
    );
}

#[test]
fn hosted_text_commands_pause_long_runs_and_restore_committed_saves() {
    use game_cli::SessionOutput;
    let execution = Execution {
        workers: 4,
        reverse_jobs: true,
    };
    let sim = Simulation::kernel_fixture(1, 10_000, execution.clone()).unwrap();
    let session = game_session::Session::spawn(sim).unwrap();
    let SessionOutput::Advancing(run) = game_cli::execute_session(&session, "run 3650d").unwrap()
    else {
        panic!("expected an advance handle")
    };
    game_cli::execute_session(&session, "move 2").unwrap();
    let SessionOutput::Text(status) = game_cli::execute_session(&session, "status").unwrap() else {
        panic!("expected a view")
    };
    let view: PlayerView = serde_json::from_str(&status).unwrap();
    assert!(view.time.0 < 3650 * 86_400_000);
    let SessionOutput::Text(paused) = game_cli::execute_session(&session, "pause").unwrap() else {
        panic!("expected a view")
    };
    let paused: PlayerView = serde_json::from_str(&paused).unwrap();
    let result = run.wait().unwrap();
    assert_eq!(result.reason, StopReason::Paused);
    assert_eq!(result.reached, paused.time);
    let bytes = session.save().unwrap();
    let saved = Simulation::load(&bytes, execution).unwrap();
    assert_eq!(saved.time(), paused.time);
    assert!(session.load(b"invalid".to_vec()).is_err());
    assert_eq!(session.observe().unwrap().time, paused.time);
    session
        .submit_command(Command::Move {
            destination: AreaId(3),
        })
        .unwrap();
    session.load(bytes).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&session.debug_json().unwrap()).unwrap(),
        serde_json::to_value(saved.canonical_state()).unwrap()
    );
}

#[test]
fn player_childhood_choices_are_playable_through_normal_views_and_text() {
    let execution = Execution::default();
    let mut sim = Simulation::new("childhood-fixture", 1, execution.clone()).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert_eq!(view.life_stage, Some(LifeStage::Infant));
    assert!(view.available_commands.contains(&Command::Signal {
        preference: ChildhoodInterest::Food
    }));
    game_cli::execute(&mut sim, "signal food", &execution).unwrap();
    game_cli::execute(&mut sim, "run 5m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert_eq!(view.life_stage, Some(LifeStage::Child));
    game_cli::execute(&mut sim, "play craft", &execution).unwrap();
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert_eq!(view.interests, vec![(ChildhoodInterest::Craft, 100)]);
    assert!(
        view.known_areas
            .iter()
            .all(|a| [AreaId(1), AreaId(2), AreaId(3)].contains(&a.id))
    );
}

#[test]
fn seasonal_farming_commands_use_real_time_seed_and_local_field_views() {
    let execution = Execution::default();
    let mut sim = Simulation::new("farming-fixture", 1, execution.clone()).unwrap();
    assert!(sim.observe().agriculture.unwrap().fields.is_empty());
    game_cli::execute(&mut sim, "move 2", &execution).unwrap();
    game_cli::execute(&mut sim, "run 2m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    let field = view
        .available_commands
        .iter()
        .find_map(|command| {
            if let Command::Cultivate {
                field,
                operation: FarmOperation::Prepare,
            } = command
            {
                Some(*field)
            } else {
                None
            }
        })
        .unwrap();
    assert!(view.available_commands.contains(&Command::Cultivate {
        field,
        operation: FarmOperation::Prepare
    }));
    game_cli::execute(&mut sim, &format!("farm {} prepare", field.0), &execution).unwrap();
    game_cli::execute(&mut sim, "run 4h", &execution).unwrap();
    let seeds_before: u64 = sim
        .observe()
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .filter(|s| s.good == Good::Seed)
        .map(|s| s.quantity.0)
        .sum();
    game_cli::execute(&mut sim, &format!("farm {} plant", field.0), &execution).unwrap();
    let seeds_after: u64 = sim
        .observe()
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .filter(|s| s.good == Good::Seed)
        .map(|s| s.quantity.0)
        .sum();
    assert_eq!(seeds_before - seeds_after, 1_000);
    game_cli::execute(&mut sim, "run 2h", &execution).unwrap();
    assert_eq!(
        sim.observe()
            .agriculture
            .unwrap()
            .fields
            .iter()
            .find(|f| f.id == field)
            .unwrap()
            .phase,
        "Growing"
    );
    assert!(sim.observe().time.0 >= 6 * 3_600_000);
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn agreement_text_commands_record_promises_and_deliver_actual_cargo() {
    use std::collections::BTreeSet;
    let execution = Execution::default();
    let mut sim = Simulation::new("agreement-fixture", 1, execution.clone()).unwrap();
    let ruler = sim.observe().ruler;
    let recipient = sim
        .observe()
        .nearby_people
        .iter()
        .find(|(_, role)| *role == Role::Citizen)
        .unwrap()
        .0;
    let source = sim
        .observe()
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .find(|s| s.good == Good::Food && s.can_access && s.quantity.0 >= 1_000)
        .unwrap()
        .id;
    let cargo = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == ruler)
        .unwrap()
        .inventory;
    let destination = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == recipient)
        .unwrap()
        .inventory;
    let proposal = AgreementAction::Propose {
        parties: BTreeSet::from([ruler, recipient]),
        terms: vec![AgreementTerm::Delivery {
            debtor: ruler,
            creditor: recipient,
            source,
            destination,
            quantity: Quantity(1_000),
            deadline: SimTime(180_000),
        }],
        procedure: None,
    };
    let stocks = sim.canonical_state().inventories.clone();
    game_cli::execute(
        &mut sim,
        &format!("agreement {}", serde_json::to_string(&proposal).unwrap()),
        &execution,
    )
    .unwrap();
    assert_eq!(&stocks, &sim.canonical_state().inventories);
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert_eq!(view.agreements[0].status, AgreementStatus::Active);
    game_cli::execute(
        &mut sim,
        &format!("load {} {} 1000", source.0, cargo.0),
        &execution,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    game_cli::execute(
        &mut sim,
        &format!("load {} {} 1000", cargo.0, destination.0),
        &execution,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    assert_eq!(
        sim.observe().agreements[0].status,
        AgreementStatus::Fulfilled
    );
    assert_eq!(sim.observe().agreements[0].obligations[0].performed, 1_000);
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn session_stops_at_successor_birth_and_switches_to_the_newborns_real_agency() {
    let mut sim = Simulation::new("lineage-fixture", 1, Execution::default()).unwrap();
    let old = sim.observe().ruler;
    sim.debug_injure(
        old,
        sim.canonical_state()
            .physical
            .as_ref()
            .unwrap()
            .rules
            .max_health,
    )
    .unwrap();
    let session = game_session::Session::spawn(sim).unwrap();
    let waiting = game_cli::execute_session(&session, "lineage").unwrap();
    let game_cli::SessionOutput::Text(waiting) = waiting else {
        panic!("view expected")
    };
    let waiting: Option<LineageView> = serde_json::from_str(&waiting).unwrap();
    assert_eq!(waiting.unwrap().current, None);
    let game_cli::SessionOutput::Advancing(handle) =
        game_cli::execute_session(&session, "run 3650d").unwrap()
    else {
        panic!("run expected")
    };
    let result = handle.wait().unwrap();
    let StopReason::SuccessorBorn { ruler } = result.reason else {
        panic!("birth must stop acceleration")
    };
    assert_ne!(ruler, old);
    assert_eq!(result.reached, SimTime(300_000));
    let view = session.observe().unwrap();
    assert_eq!(view.life_stage, Some(LifeStage::Infant));
    assert_eq!(view.lineage.unwrap().current, Some(ruler));
    assert!(view.available_commands.contains(&Command::Signal {
        preference: ChildhoodInterest::Food
    }));
    game_cli::execute_session(&session, "signal food").unwrap();
    let game_cli::SessionOutput::Advancing(handle) =
        game_cli::execute_session(&session, "run 6s").unwrap()
    else {
        panic!("run expected")
    };
    handle.wait().unwrap();
    assert_eq!(
        session.observe().unwrap().preference,
        Some(ChildhoodInterest::Food)
    );
}

#[test]
fn text_client_exposes_refusal_and_resistance_without_granting_goods() {
    let mut sim = Simulation::new("property-fixture", 1, Execution::default()).unwrap();
    let execution = Execution::default();
    let view = sim.observe();
    let source = view
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .find(|s| s.good == Good::Food && !s.can_access && s.quantity.0 >= 1_000)
        .unwrap()
        .id;
    let cargo = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == view.ruler)
        .unwrap()
        .inventory;
    game_cli::execute(
        &mut sim,
        &format!("request-access {}", source.0),
        &execution,
    )
    .unwrap();
    assert!(
        !sim.observe()
            .physical
            .unwrap()
            .local_stocks
            .iter()
            .find(|s| s.id == source)
            .unwrap()
            .can_access
    );
    let stock = sim.canonical_state().inventories[&source].quantity;
    game_cli::execute(
        &mut sim,
        &format!("take {} {} 1000", source.0, cargo.0),
        &execution,
    )
    .unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert_eq!(
        view.property.incidents[0].outcome,
        IncidentOutcome::Interrupted
    );
    assert!(view.combat.is_some());
    assert_eq!(sim.canonical_state().inventories[&source].quantity, stock);
    assert_eq!(
        sim.canonical_state().inventories[&cargo].quantity,
        Quantity(0)
    );
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn childhood_guidance_requires_completed_lessons_in_the_text_client() {
    let execution = Execution::default();
    let mut sim = Simulation::new("lineage-fixture", 1, execution.clone()).unwrap();
    sim.debug_injure(
        sim.observe().ruler,
        sim.canonical_state()
            .physical
            .as_ref()
            .unwrap()
            .rules
            .max_health,
    )
    .unwrap();
    game_cli::execute(&mut sim, "run 5m", &execution).unwrap();
    game_cli::execute(&mut sim, "signal food", &execution).unwrap();
    game_cli::execute(&mut sim, "run 5m", &execution).unwrap();
    let teacher = sim
        .observe()
        .available_commands
        .iter()
        .find_map(|c| match c {
            Command::Learn {
                teacher,
                topic: LessonTopic::Invocation,
            } => Some(*teacher),
            _ => None,
        })
        .unwrap();
    for expected in [100, 200] {
        game_cli::execute(
            &mut sim,
            &format!("learn {} invocation", teacher.0),
            &execution,
        )
        .unwrap();
        let before = sim.observe().lineage.unwrap().invocation_learning;
        game_cli::execute(&mut sim, "run 59999ms", &execution).unwrap();
        assert_eq!(sim.observe().lineage.unwrap().invocation_learning, before);
        game_cli::execute(&mut sim, "run 1ms", &execution).unwrap();
        assert_eq!(sim.observe().lineage.unwrap().invocation_learning, expected);
    }
    assert!(sim.observe().lineage.unwrap().invocation_ready);
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn institution_charters_and_supported_offices_have_real_access_effects_through_cli() {
    use std::collections::BTreeSet;
    let execution = Execution::default();
    let mut sim = Simulation::new("institution-fixture", 1, execution.clone()).unwrap();
    let ruler = sim.observe().ruler;
    let owner = sim
        .observe()
        .nearby_people
        .iter()
        .find(|(_, role)| *role == Role::Citizen)
        .unwrap()
        .0;
    let stock = sim
        .observe()
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .find(|s| s.good == Good::Food && !s.can_access)
        .unwrap()
        .id;
    let charter = InstitutionCharter {
        purpose: "Food access by our agreement".into(),
        jurisdiction: AreaId(1),
        founders: BTreeSet::from([ruler, owner]),
        open_membership: false,
        procedure: DecisionProcedure {
            proposers: Subjects::members(),
            voters: Subjects::members(),
            minimum_support: 2,
        },
        offices: vec![OfficeMandate {
            slot: 0,
            holder: ruler,
            inventories: BTreeSet::from([stock]),
            sites: BTreeSet::new(),
            powers: BTreeSet::from([OfficePower::Access]),
            minimum_support: 2,
        }],
        rules: vec![InstitutionalRule {
            subjects: Subjects::named(ruler),
            when: RuleExpression::All(vec![
                RuleExpression::Fact(SocialCondition::Local),
                RuleExpression::Fact(SocialCondition::Member),
            ]),
            effect: SocialEffect::Access {
                inventory: stock,
                allowed: true,
            },
            authority: 0,
        }],
        contributions: Vec::new(),
        evidence: BTreeSet::new(),
    };
    let action = InstitutionAction::Found {
        charter,
        origin: None,
    };
    let before = sim.canonical_state().inventories.clone();
    game_cli::execute(
        &mut sim,
        &format!("institution {}", serde_json::to_string(&action).unwrap()),
        &execution,
    )
    .unwrap();
    assert_eq!(sim.canonical_state().inventories, before);
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert_eq!(view.institutions[0].status, InstitutionStatus::Active);
    assert!(
        view.physical
            .unwrap()
            .local_stocks
            .iter()
            .find(|s| s.id == stock)
            .unwrap()
            .can_access
    );
    let action = InstitutionAction::SupportOffice {
        institution: view.institutions[0].id,
        office: 0,
        support: false,
    };
    game_cli::execute(
        &mut sim,
        &format!("institution {}", serde_json::to_string(&action).unwrap()),
        &execution,
    )
    .unwrap();
    assert!(
        !sim.observe()
            .physical
            .unwrap()
            .local_stocks
            .iter()
            .find(|s| s.id == stock)
            .unwrap()
            .can_access
    );
    assert_eq!(sim.observe().ruler, ruler);
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn carrying_a_new_good_creates_only_an_empty_compartment_then_loads_real_stock() {
    let execution = Execution::default();
    let mut sim = Simulation::new("survival-fixture", 1, execution.clone()).unwrap();
    let view = sim.observe();
    let tool = view
        .physical
        .unwrap()
        .local_stocks
        .iter()
        .find(|s| s.good == Good::Tool && s.can_access)
        .unwrap()
        .id;
    game_cli::execute(&mut sim, &format!("carry {} 1000", tool.0), &execution).unwrap();
    assert_eq!(
        sim.observe()
            .physical
            .unwrap()
            .local_stocks
            .iter()
            .find(|s| s.good == Good::Tool && s.position == StockPosition::Carried(view.ruler))
            .unwrap()
            .quantity,
        Quantity(0)
    );
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&tool].quantity,
        Quantity(0)
    );
    assert_eq!(
        sim.observe()
            .physical
            .unwrap()
            .local_stocks
            .iter()
            .find(|s| s.good == Good::Tool && s.position == StockPosition::Carried(view.ruler))
            .unwrap()
            .quantity,
        Quantity(1_000)
    );
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn the_ordinary_text_view_observes_a_completed_autonomous_workshop_without_private_accounts() {
    let execution = Execution::default();
    let mut sim = Simulation::new("project-fixture", 1, execution.clone()).unwrap();
    game_cli::execute(&mut sim, "run 25m", &execution).unwrap();
    let view: PlayerView =
        serde_json::from_str(&game_cli::execute(&mut sim, "status", &execution).unwrap()).unwrap();
    assert!(
        view.projects
            .projects
            .iter()
            .any(|project| project.status == ProjectStatus::Completed)
    );
    assert!(!view.projects.enterprises.is_empty());
    assert!(
        view.projects
            .enterprises
            .iter()
            .all(|enterprise| enterprise.accounts.is_none())
    );
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn a_captive_can_attempt_escape_through_ordinary_text_commands() {
    let execution = Execution::default();
    let mut sim = Simulation::new("force-fixture", 88, execution.clone()).unwrap();
    let ruler = sim.observe().ruler;
    let captor = sim.canonical_state().people[3].id;
    let people: Vec<_> = sim.canonical_state().people.iter().map(|p| p.id).collect();
    for actor in people {
        sim.debug_actor_command(actor, Command::Cancel).unwrap();
        sim.debug_set_personality(
            actor,
            game_assembly::decisions::Personality::restful(),
            game_assembly::decisions::Aspiration::Comfort,
        )
        .unwrap();
    }
    let tool = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| {
            i.good == Good::Tool && i.location == game_assembly::InventoryLocation::Area(AreaId(1))
        })
        .unwrap()
        .id;
    game_cli::execute(&mut sim, &format!("carry {} 1000", tool.0), &execution).unwrap();
    game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
    game_cli::execute(&mut sim, &format!("yield {}", captor.0), &execution).unwrap();
    sim.debug_actor_command(captor, Command::Restrain { target: ruler })
        .unwrap();
    game_cli::execute(&mut sim, "run 6s", &execution).unwrap();
    assert!(
        sim.observe()
            .force
            .confinements
            .iter()
            .any(|c| c.target == ruler && c.can_escape)
    );
    game_cli::execute(&mut sim, "escape", &execution).unwrap();
    game_cli::execute(&mut sim, "run 6s", &execution).unwrap();
    assert!(sim.observe().force.confinements.is_empty());
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn childhood_guidance_activation_and_cost_choices_are_playable_through_text() {
    let execution = Execution::default();
    let mut sim = Simulation::new("invocation-fixture", 9, execution.clone()).unwrap();
    let actor = sim.observe().ruler;
    let world = sim.canonical_state().development.as_ref().unwrap();
    let childhood =
        sim.canonical_state().components.life(actor).unwrap().born.0 + world.rules.infancy.0;
    let delta = childhood - sim.time().0;
    game_cli::execute(&mut sim, &format!("run {delta}ms"), &execution).unwrap();
    let teacher = *sim
        .canonical_state()
        .lineage
        .as_ref()
        .unwrap()
        .guardians
        .keys()
        .next()
        .unwrap();
    for _ in 0..2 {
        let mut completed = false;
        for _ in 0..12 {
            if matches!(
                sim.canonical_state()
                    .components
                    .location(teacher)
                    .expect("location exists"),
                Location::AtArea(_)
            ) {
                sim.debug_actor_command(teacher, Command::Rest).unwrap();
            }
            if game_cli::execute(
                &mut sim,
                &format!("learn {} invocation", teacher.0),
                &execution,
            )
            .is_ok()
                && matches!(
                    sim.observe().physical.unwrap().activity,
                    Activity::Lesson { .. }
                )
            {
                game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
                completed = true;
                break;
            }
            game_cli::execute(&mut sim, "run 1m", &execution).unwrap();
        }
        assert!(completed);
    }
    let invocation = sim.observe().invocation.unwrap();
    assert!(invocation.ready);
    let life = invocation.available_lives[0];
    let action = serde_json::to_string(&InvocationAction::Activate {
        lives: std::collections::BTreeSet::from([life]),
    })
    .unwrap();
    game_cli::execute(&mut sim, &format!("invocation {action}"), &execution).unwrap();
    let warning = sim.observe().invocation.unwrap().warning.unwrap();
    let stopped: AdvanceResult =
        serde_json::from_str(&game_cli::execute(&mut sim, "run 1m", &execution).unwrap()).unwrap();
    assert_eq!(
        stopped.reason,
        StopReason::InvocationWarning {
            warning: warning.id
        }
    );
    let action = serde_json::to_string(&InvocationAction::AcceptWarning {
        warning: warning.id,
    })
    .unwrap();
    game_cli::execute(&mut sim, &format!("invocation {action}"), &execution).unwrap();
    assert!(
        sim.observe()
            .invocation
            .unwrap()
            .active_lives
            .contains(&life)
    );
    game_cli::execute(&mut sim, r#"invocation "Dismiss""#, &execution).unwrap();
    assert!(sim.observe().invocation.unwrap().active_lives.is_empty());
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn a_guardian_food_request_delivers_existing_food_without_graduation_or_free_stock() {
    let execution = Execution::default();
    let mut sim = Simulation::new("force-fixture", 88, execution.clone()).unwrap();
    let guardian = *sim
        .canonical_state()
        .lineage
        .as_ref()
        .unwrap()
        .guardians
        .keys()
        .next()
        .unwrap();
    let ruler = sim.observe().ruler;
    for person in sim
        .canonical_state()
        .people
        .iter()
        .map(|p| p.id)
        .collect::<Vec<_>>()
    {
        sim.debug_actor_command(person, Command::Rest).unwrap();
        sim.debug_set_personality(
            person,
            game_assembly::decisions::Personality::restful(),
            game_assembly::decisions::Aspiration::Comfort,
        )
        .unwrap();
    }
    let source = sim
        .canonical_state()
        .inventories
        .values()
        .find(|i| {
            i.good == Good::Food && i.location == game_assembly::InventoryLocation::Area(AreaId(1))
        })
        .unwrap()
        .id;
    let stock = sim.canonical_state().inventories[&source].quantity.0;
    let cargo = sim
        .canonical_state()
        .people
        .iter()
        .find(|p| p.id == ruler)
        .unwrap()
        .inventory;
    let held = sim.canonical_state().inventories[&cargo].quantity.0;
    game_cli::execute(
        &mut sim,
        &format!("guardian {} supply food {} 1000", guardian.0, source.0),
        &execution,
    )
    .unwrap();
    assert_eq!(sim.canonical_state().inventories[&source].quantity.0, stock);
    assert_eq!(sim.canonical_state().inventories[&cargo].quantity.0, held);
    game_cli::execute(&mut sim, "run 2m", &execution).unwrap();
    assert!(sim.canonical_state().inventories[&source].quantity.0 <= stock - 1000);
    let loaded: u64 = sim
        .canonical_state()
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Transferred {
                actor,
                source: from,
                quantity,
                ..
            } if actor == guardian && from == source => Some(quantity.0),
            _ => None,
        })
        .sum();
    assert_eq!(loaded, 1000);
    assert_eq!(
        sim.canonical_state().inventories[&cargo].quantity.0,
        held + 1000
    );
    assert!(sim.canonical_state().events.iter().any(|e| matches!(e.kind, EventKind::GuardianAccepted { guardian: actor, requester } if actor == guardian && requester == ruler)));
    Simulation::load(&sim.save().unwrap(), execution).unwrap();
}

#[test]
fn normal_openings_and_seeded_random_selection_launch_through_the_cli() {
    use std::io::Write;
    for scenario in [
        "settlement-founder",
        "established-ruler",
        "child-successor",
        "random",
    ] {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_game-cli"))
            .args(["--scenario", scenario, "--seed", "41"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"status\nquit\n")
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success(), "{scenario}");
        let text = String::from_utf8(result.stdout).unwrap();
        let actual = if scenario == "random" {
            game_assembly::scenarios::choose(41)
        } else {
            scenario
        };
        assert!(text.starts_with(&format!("opening: {actual}\n")));
        let view: PlayerView = text
            .lines()
            .find_map(|line| serde_json::from_str(line).ok())
            .unwrap();
        assert!(view.lineage.is_some() && view.environment.is_some());
        if actual == "child-successor" {
            assert_eq!(view.life_stage, Some(LifeStage::Infant));
        }
    }
}

#[test]
fn roadside_transport_is_playable_with_carried_food_through_the_text_client() {
    let execution = Execution::default();
    let mut sim = Simulation::new("transport-fixture", 1, execution.clone()).unwrap();
    let ruler = sim.canonical_state().ruler;
    let food = sim
        .debug_add_inventory(
            Good::Food,
            InventoryLocation::Carried(ruler),
            Quantity(2_000),
            Quantity(2_000),
            std::collections::BTreeSet::from([ruler]),
        )
        .unwrap();
    game_cli::execute(&mut sim, "move 2", &execution).unwrap();
    game_cli::execute(&mut sim, "run 30000ms", &execution).unwrap();
    game_cli::execute(&mut sim, "transport \"Stop\"", &execution).unwrap();
    assert!(matches!(
        sim.observe().location,
        Location::StoppedOnRoad {
            progress: SimDuration(30_000),
            ..
        }
    ));
    assert!(
        sim.observe()
            .available_commands
            .contains(&Command::Eat { source: food })
    );
    game_cli::execute(&mut sim, &format!("eat {}", food.0), &execution).unwrap();
    game_cli::execute(&mut sim, "run 60000ms", &execution).unwrap();
    assert_eq!(
        sim.canonical_state().inventories[&food].quantity,
        Quantity(1_000)
    );
    let mut restored = Simulation::load(&sim.save().unwrap(), execution.clone()).unwrap();
    game_cli::execute(&mut restored, "transport \"Resume\"", &execution).unwrap();
    game_cli::execute(&mut restored, "run 90000ms", &execution).unwrap();
    assert_eq!(restored.observe().location, Location::AtArea(AreaId(2)));
}
