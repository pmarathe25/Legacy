use game_api::{
    Activity, ChildhoodInterest, Command, EcologyAction, EventKind, Good, HealthAction,
    InvocationAction, LifeStage, PlayerView, Quantity, ResourceKind, SimDuration, SocialAction,
    StockPosition, StopReason, Submission,
};
use game_assembly::{Execution, Simulation, scenarios::Config};
use game_openings::PopulationPlan as Plan;
use std::collections::BTreeMap;
use std::time::Instant;

// A benchmark actor uses the same observations and public commands as a client.
// This policy never supplies goods, accepts invocation costs, or directs NPCs.
fn player_input(
    view: &PlayerView,
    config: &Config,
    inspected: &BTreeMap<game_api::AreaId, game_api::SimTime>,
) -> Option<Command> {
    let body = view.physical.as_ref()?;
    if !body.vitals.alive {
        return None;
    }
    let offered = &view.available_commands;
    if view.life_stage == Some(LifeStage::Infant) {
        let preference = if body.vitals.hunger >= 6 * 3_600_000 {
            ChildhoodInterest::Food
        } else {
            ChildhoodInterest::Company
        };
        return offered
            .iter()
            .find(|c| matches!(c, Command::Signal { preference: p } if *p == preference))
            .cloned();
    }
    if !matches!(body.activity, Activity::Idle | Activity::Rest) {
        return None;
    }
    if !matches!(view.location, game_api::Location::AtArea(_)) {
        return None;
    }
    let mut unmet_water = false;
    if let Some(health) = &view.health {
        if (health.symptoms.fever || health.symptoms.wounded || health.symptoms.shivering)
            && let Some(command) = offered.iter().find(|c| {
                matches!(
                    c,
                    Command::Health {
                        action: HealthAction::AcceptTreatment { .. }
                    }
                )
            })
        {
            return Some(command.clone());
        }
        if health.thirst >= config.health.drink_relief / 2
            && let Some(command) = offered.iter().find(|c| {
                matches!(c,
                Command::Health { action: HealthAction::Drink { source } }
                    if health.water.iter().any(|w| w.inventory == *source && !w.suspected_unsafe))
            })
        {
            return Some(command.clone());
        }
        if health.thirst >= config.health.drink_relief / 2 {
            unmet_water = true;
            if let Some(command) = offered.iter().find(|command| {
                matches!(
                    command,
                    Command::Health {
                        action: HealthAction::Boil { .. }
                    }
                )
            }) {
                return Some(command.clone());
            }
            let supplies_here = [
                (Good::Timber, config.health.boiling_fuel),
                (Good::Tool, Quantity(1_000)),
            ]
            .into_iter()
            .all(|(good, quantity)| {
                body.local_stocks.iter().any(|stock| {
                    stock.good == good && stock.can_access && stock.quantity >= quantity
                })
            });
            let held_water = body
                .local_stocks
                .iter()
                .filter(|stock| {
                    stock.good == Good::Water
                        && stock.position == StockPosition::Carried(view.ruler)
                })
                .map(|stock| u128::from(stock.quantity.0))
                .sum::<u128>();
            if supplies_here
                && held_water < u128::from(config.health.drink_quantity.0)
                && let Some(command) = offered.iter().find(|command| {
                    matches!(command,
                    Command::Load { source, destination, .. }
                        if body.local_stocks.iter().any(|stock| stock.id == *source
                            && stock.good == Good::Water
                            && stock.position != StockPosition::Carried(view.ruler))
                        && body.local_stocks.iter().any(|stock| stock.id == *destination
                            && stock.good == Good::Water
                            && stock.position == StockPosition::Carried(view.ruler)))
                })
            {
                return Some(command.clone());
            }
            if let Some(command) = offered.iter().find(|command| matches!(command,
                Command::Ecology { action: EcologyAction::Gather { resource, unauthorized: false, .. } }
                    if view.ecology.resources.iter().any(|source| source.id == *resource
                        && source.kind == ResourceKind::Spring && source.accessible && source.contamination == 0))) {
                return Some(command.clone());
            }
            if supplies_here && held_water < u128::from(config.health.drink_quantity.0)
                && let Some(command) = offered.iter().find(|command| matches!(command,
                    Command::Ecology { action: EcologyAction::Gather { resource, unauthorized: false, .. } }
                        if view.ecology.resources.iter().any(|source| source.id == *resource
                            && source.kind == ResourceKind::Spring && source.accessible
                            && source.available >= config.health.drink_quantity)))
            {
                return Some(command.clone());
            }
            // If preparation is unavailable, the actor may choose the real
            // illness risk of drinking over imminent dehydration.
            if health.thirst >= config.health.thirst_harm / 2
                && let Some(command) = offered.iter().find(|command| {
                    matches!(
                        command,
                        Command::Health {
                            action: HealthAction::Drink { .. }
                        }
                    )
                })
            {
                return Some(command.clone());
            }
            let local_supply = view.ecology.resources.iter().any(|source|
                source.kind == ResourceKind::Spring && source.accessible
                    && source.available >= config.health.drink_quantity
                    && matches!(view.location, game_api::Location::AtArea(area) if area == source.area));
            if !local_supply {
                if let Some(command) = offered.iter().find(|command| matches!(command,
                Command::Move { destination } if view.known_stocks.iter().any(|stock|
                    stock.good == Good::Water && stock.accessible && stock.quantity >= config.health.drink_quantity
                        && stock.area == *destination)
                    || view.ecology.resources.iter().any(|source| source.kind == ResourceKind::Spring
                        && source.accessible && source.contamination == 0 && source.available >= config.health.drink_quantity
                        && source.area == *destination))) {
                return Some(command.clone());
            }
                if let Some(command) = offered
                    .iter()
                    .filter_map(|command| match command {
                        Command::Move { destination }
                            if inspected.get(destination).is_none_or(|at| {
                                view.time.0.saturating_sub(at.0) >= config.ecology.search_retry.0
                            }) =>
                        {
                            Some((inspected.get(destination).copied(), *destination, command))
                        }
                        _ => None,
                    })
                    .min_by_key(|(seen, area, _)| (*seen, *area))
                    .map(|(_, _, command)| command)
                {
                    return Some(command.clone());
                }
            }
        }
        if health.sheltered.is_none()
            && let Some(command) = offered.iter().find(|c| {
                matches!(
                    c,
                    Command::Health {
                        action: HealthAction::EnterShelter { .. }
                    }
                )
            })
        {
            return Some(command.clone());
        }
    }
    if body.vitals.hunger >= 86_400_000 {
        if let Some(command) = offered.iter().find(|c| matches!(c, Command::Eat { .. })) {
            return Some(command.clone());
        }
        for command in offered {
            if let Command::Load {
                source,
                destination,
                quantity,
            } = command
                && body
                    .local_stocks
                    .iter()
                    .any(|stock| stock.id == *source && stock.good == Good::Food)
                && body.local_stocks.iter().any(|stock| {
                    stock.id == *destination && stock.position == StockPosition::Carried(view.ruler)
                })
            {
                return Some(Command::Load {
                    source: *source,
                    destination: *destination,
                    quantity: Quantity(quantity.0.min(2_000)),
                });
            }
        }
    }
    if body.vitals.fatigue >= 8 * 3_600_000 || body.vitals.health == 0 {
        return offered.iter().find(|c| matches!(c, Command::Rest)).cloned();
    }
    // An unmet supply goal should not prevent another immediate bodily action.
    // After eating, wait for observed sources rather than starting a lesson.
    if unmet_water {
        if body.activity == Activity::Rest {
            return None;
        }
        return offered
            .iter()
            .find(|command| matches!(command, Command::Rest))
            .cloned();
    }
    offered
        .iter()
        .find(|c| {
            matches!(
                c,
                Command::Learn { .. }
                    | Command::Social {
                        action: SocialAction::Learn { .. } | SocialAction::Practice { .. }
                    }
            )
        })
        .cloned()
}

fn main() {
    let population = std::env::args()
        .nth(1)
        .map_or(1_000, |s| s.parse().expect("population"));
    let workers = std::env::args()
        .nth(2)
        .map_or(1, |s| s.parse().expect("workers"));
    let minutes = std::env::args()
        .nth(3)
        .map_or(120, |s| s.parse::<u64>().expect("simulated minutes"));
    let policy = std::env::args().nth(4).as_deref() == Some("policy") || minutes > 120;
    let trace_player = std::env::args().nth(5).as_deref() == Some("trace-player");
    let config = Config::default();
    let start = Instant::now();
    let mut sim = Simulation::populated_world(
        1,
        Execution {
            workers,
            reverse_jobs: false,
        },
        config.clone(),
        Plan {
            people: population,
            settlement_size: 100,
        },
    )
    .unwrap();
    let init = start.elapsed();
    eprintln!(
        "full_world phase=initialized people={population} milliseconds={:.3}",
        init.as_secs_f64() * 1e3
    );
    let start = Instant::now();
    let initial_events = sim.canonical_state().events.len();
    let before_time = sim.canonical_state().time;
    let before_profile = sim.profile().clone();
    let target = before_time
        .0
        .checked_add(minutes.checked_mul(60_000).expect("duration"))
        .expect("target");
    let mut commands = 0;
    let mut successions = 0;
    let mut input_stop = None;
    let mut reported_day = 0;
    let mut traced_day = None;
    let mut inspected = BTreeMap::new();
    while sim.canonical_state().time.0 < target {
        let mut poll_interval = if policy { 3_600_000 } else { u64::MAX };
        if policy {
            let view = sim.observe();
            if view
                .health
                .as_ref()
                .is_some_and(|health| health.thirst >= config.health.drink_relief / 2)
                || view
                    .physical
                    .as_ref()
                    .is_some_and(|body| body.vitals.hunger >= 86_400_000)
            {
                poll_interval = 60_000;
            }
            if let game_api::Location::AtArea(area) = view.location {
                inspected.insert(area, view.time);
            }
            let selected = player_input(&view, &config, &inspected);
            let day = view.time.0 / config.environment.calendar.day.0;
            if trace_player
                && traced_day != Some(day)
                && view
                    .health
                    .as_ref()
                    .is_some_and(|health| health.thirst >= config.health.thirst_harm / 2)
            {
                traced_day = Some(day);
                let supplies = view.physical.as_ref().map(|body| {
                    body.local_stocks
                        .iter()
                        .filter(|stock| {
                            matches!(stock.good, Good::Water | Good::Timber | Good::Tool)
                        })
                        .collect::<Vec<_>>()
                });
                let actions = view
                    .available_commands
                    .iter()
                    .filter(|command| {
                        matches!(
                            command,
                            Command::Health { .. }
                                | Command::Ecology { .. }
                                | Command::Load { .. }
                                | Command::Move { .. }
                        )
                    })
                    .collect::<Vec<_>>();
                eprintln!(
                    "player_policy day={day} location={:?} selected={selected:?} activity={:?} hunger={:?} thirst={:?} supplies={supplies:?} ecology={:?} offered={actions:?}",
                    view.location,
                    view.physical.as_ref().map(|body| &body.activity),
                    view.physical.as_ref().map(|body| body.vitals.hunger),
                    view.health.as_ref().map(|health| health.thirst),
                    view.ecology.resources
                );
                // Diagnostic state is never supplied to the client policy.
                let state = sim.canonical_state();
                let carried = game_cargo::inventories(state.components)
                    .map(|(_, inventory)| inventory)
                    .filter(|stock| {
                        stock.location == game_assembly::InventoryLocation::Carried(view.ruler)
                    })
                    .map(|stock| u128::from(stock.quantity.0))
                    .sum::<u128>();
                let resource_jobs = state.state.ecology.as_ref().map(|ecology| view.ecology.resources.iter()
                    .filter(|source| matches!(view.location, game_api::Location::AtArea(area) if source.area == area))
                    .map(|source| (source.id, ecology.resource(sim.canonical_state().components, source.id).expect("resource exists").workers,
                        game_ecology::gatherings(state.components).filter(|(_, job)| job.resource == source.id)
                            .map(|(actor, job)| (actor, job.quantity, job.completes)).collect::<Vec<_>>()))
                    .collect::<Vec<_>>());
                eprintln!(
                    "player_diagnostic day={day} carried={carried} resource_jobs={resource_jobs:?}"
                );
            }
            if let Some(command) = selected {
                sim.submit(Submission {
                    caller_sequence: sim.next_caller_sequence().unwrap(),
                    command,
                })
                .unwrap();
                commands += 1;
            }
        }
        let elapsed = (target - sim.canonical_state().time.0).min(poll_interval);
        let result = sim.advance(SimDuration(elapsed)).unwrap();
        match result.reason {
            StopReason::TargetReached => {}
            StopReason::SuccessorBorn { .. } if policy => {
                successions += 1;
            }
            StopReason::InvocationWarning { .. } if policy => {
                // Explicitly dismiss rather than accepting a new cost barrier.
                let command = Command::Invocation {
                    action: InvocationAction::Dismiss,
                };
                if sim.observe().available_commands.contains(&command) {
                    sim.submit(Submission {
                        caller_sequence: sim.next_caller_sequence().unwrap(),
                        command,
                    })
                    .unwrap();
                    commands += 1;
                } else {
                    input_stop = Some(result.reason);
                    break;
                }
            }
            reason => {
                input_stop = Some(reason);
                break;
            }
        }
        let day =
            (sim.canonical_state().time.0 - before_time.0) / config.environment.calendar.day.0;
        if policy && day > reported_day {
            reported_day = day;
            let state = sim.canonical_state();
            let living = game_physiology::bodies(state.components)
                .filter(|(_, body)| body.vitals.alive)
                .count();
            let player_alive = game_physiology::body(state.components, state.state.ruler)
                .expect("body exists")
                .vitals
                .alive;
            eprintln!(
                "full_world day={day} living={living} people={} player_alive={player_alive}",
                state.state.people.len()
            );
        }
    }
    let advance = start.elapsed();
    eprintln!(
        "full_world phase=advanced people={population} milliseconds={:.3}",
        advance.as_secs_f64() * 1e3
    );
    let start = Instant::now();
    let final_player = std::hint::black_box(sim.observe());
    let view = start.elapsed();
    eprintln!(
        "player_end ruler={} location={:?} vitals={:?} thirst={:?}",
        final_player.ruler.0,
        final_player.location,
        final_player.physical.as_ref().map(|body| &body.vitals),
        final_player.health.as_ref().map(|health| health.thirst)
    );
    let start = Instant::now();
    let save = sim.save().unwrap();
    let save_time = start.elapsed();
    let start = Instant::now();
    let loaded = Simulation::load(
        &save,
        Execution {
            workers,
            reverse_jobs: false,
        },
    )
    .unwrap();
    let restore = start.elapsed();
    assert_eq!(sim.canonical_state(), loaded.canonical_state());
    let alive = game_physiology::bodies(sim.canonical_state().components)
        .filter(|(_, body)| body.vitals.alive)
        .count();
    let mut journeys = 0;
    let mut transfers = 0;
    let mut production = 0;
    let mut cultivation = 0;
    let mut gatherings = 0;
    let mut social = 0;
    let mut births = 0;
    let mut conceptions = 0;
    let mut deaths = 0;
    for event in &sim.canonical_state().events[initial_events..] {
        match event.event {
            EventKind::Departed { .. } => journeys += 1,
            EventKind::Transferred { .. } => transfers += 1,
            EventKind::Produced { .. } => production += 1,
            EventKind::Cultivated { .. } => cultivation += 1,
            EventKind::ResourceCollected { .. } => gatherings += 1,
            EventKind::Socialized { .. } | EventKind::InstructionCompleted { .. } => social += 1,
            EventKind::Born { .. } => births += 1,
            EventKind::Conceived { .. } => conceptions += 1,
            EventKind::Died { .. } => deaths += 1,
            _ => {}
        }
    }
    if minutes >= 120 && input_stop.is_none() {
        assert!(
            production + cultivation + gatherings > 0,
            "no actual productive activity"
        );
        assert!(
            journeys + transfers > 0,
            "no actual movement or goods transfers"
        );
        assert!(
            social + conceptions > 0,
            "no completed interpersonal activities"
        );
    }
    println!(
        "full_world people={population} living={alive} areas={} events={} workers={workers} simulated_minutes={minutes} reached_ms={} policy={policy} commands={commands} successions={successions} input_stop={input_stop:?} journeys={journeys} transfers={transfers} production={production} cultivation={cultivation} gathering={gatherings} social={social} conceptions={conceptions} births={births} deaths={deaths} history_events={} initialization_ms={:.3} advance_ms={:.3} view_ms={:.3} save_ms={:.3} restore_ms={:.3} save_bytes={} rss={}",
        sim.canonical_state().state.areas.len(),
        sim.canonical_state().events.len(),
        sim.canonical_state().time.0 - before_time.0,
        initial_events,
        init.as_secs_f64() * 1e3,
        advance.as_secs_f64() * 1e3,
        view.as_secs_f64() * 1e3,
        save_time.as_secs_f64() * 1e3,
        restore.as_secs_f64() * 1e3,
        save.len(),
        std::fs::read_to_string("/proc/self/status")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("VmHWM"))
            .unwrap_or("unknown")
    );
    let profile = sim.profile();
    println!(
        "profile evaluation_ms={:.3} arbitration_ms={:.3} commit_ms={:.3} boundaries={} evaluated_people={}",
        (profile.evaluation_ns - before_profile.evaluation_ns) as f64 / 1e6,
        (profile.arbitration_ns - before_profile.arbitration_ns) as f64 / 1e6,
        (profile.commit_ns - before_profile.commit_ns) as f64 / 1e6,
        profile.boundaries - before_profile.boundaries,
        profile.evaluated_people - before_profile.evaluated_people
    );
    let mut phases: Vec<_> = profile
        .phases_ns
        .iter()
        .map(|(name, value)| {
            (
                *name,
                value - before_profile.phases_ns.get(name).copied().unwrap_or(0),
            )
        })
        .collect();
    phases.sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));
    for (phase, elapsed) in phases {
        println!(
            "profile phase={phase} milliseconds={:.3}",
            elapsed as f64 / 1e6
        );
    }
    for (kind, count) in &profile.command_attempts {
        println!(
            "profile command={kind} attempts={}",
            count
                - before_profile
                    .command_attempts
                    .get(kind)
                    .copied()
                    .unwrap_or(0)
        );
    }

    let player_alive = game_physiology::body(
        sim.canonical_state().components,
        sim.canonical_state().state.ruler,
    )
    .expect("body exists")
    .vitals
    .alive;
    if alive < population * 9 / 10 || !player_alive {
        let state = sim.canonical_state();
        for (actor, body) in game_physiology::bodies(state.components) {
            let actor = &actor;
            if !body.vitals.alive {
                let clinical = game_health::clinical(state.components, *actor).unwrap();
                eprintln!(
                    "dead_body actor={} ruler={} hunger={} fatigue={} thirst={} cold={} illnesses={} age_ms={} activity={:?}",
                    actor.0,
                    *actor == state.state.ruler,
                    body.vitals.hunger,
                    body.vitals.fatigue,
                    clinical.thirst,
                    clinical.cold,
                    clinical.illnesses.len(),
                    game_lifecycle::life(state.components, *actor)
                        .unwrap()
                        .age_at(state.time)
                        .unwrap(),
                    state.components.activity(*actor).expect("activity exists"),
                );
            }
        }
    }
    assert!(
        alive >= population * 9 / 10,
        "ordinary workload became mostly dead"
    );
}
