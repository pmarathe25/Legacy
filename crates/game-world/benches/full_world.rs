use game_api::{
    Activity, ChildhoodInterest, Command, EcologyAction, EventKind, Good, HealthAction,
    InvocationAction, LifeStage, PlayerView, Quantity, ResourceKind, SimDuration, SocialAction,
    StockPosition, StopReason, Submission,
};
use game_world::{Execution, Simulation, population::Plan, scenarios::Config};
use std::time::Instant;

// A benchmark actor uses the same observations and public commands as a client.
// This policy never supplies goods, accepts invocation costs, or directs NPCs.
fn player_input(view: &PlayerView, config: &Config) -> Option<Command> {
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
            if let Some(command) = offered.iter().find(|command| matches!(command,
                Command::Ecology { action: EcologyAction::Gather { resource, unauthorized: false, .. } }
                    if view.ecology.resources.iter().any(|source| source.id == *resource
                        && source.kind == ResourceKind::Spring && source.accessible && source.contamination == 0))) {
                return Some(command.clone());
            }
            if let Some(command) = offered.iter().find(|command| matches!(command,
                Command::Move { destination } if view.known_stocks.iter().any(|stock|
                    stock.good == Good::Water && stock.accessible && stock.quantity >= config.health.drink_quantity
                        && stock.area == *destination)
                    || view.ecology.resources.iter().any(|source| source.kind == ResourceKind::Spring
                        && source.accessible && source.contamination == 0 && source.available >= config.health.drink_quantity
                        && source.area == *destination))) {
                return Some(command.clone());
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
    while sim.canonical_state().time.0 < target {
        if policy {
            let view = sim.observe();
            if let Some(command) = player_input(&view, &config) {
                sim.submit(Submission {
                    caller_sequence: sim.next_caller_sequence().unwrap(),
                    command,
                })
                .unwrap();
                commands += 1;
            }
        }
        let elapsed =
            (target - sim.canonical_state().time.0).min(if policy { 3_600_000 } else { u64::MAX });
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
            let living = state
                .physical
                .as_ref()
                .unwrap()
                .actors
                .values()
                .filter(|body| body.vitals.alive)
                .count();
            let player_alive = state.physical.as_ref().unwrap().actors[&state.ruler]
                .vitals
                .alive;
            eprintln!(
                "full_world day={day} living={living} people={} player_alive={player_alive}",
                state.people.len()
            );
        }
    }
    let advance = start.elapsed();
    eprintln!(
        "full_world phase=advanced people={population} milliseconds={:.3}",
        advance.as_secs_f64() * 1e3
    );
    let start = Instant::now();
    let _ = std::hint::black_box(sim.observe());
    let view = start.elapsed();
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
    let alive = sim
        .canonical_state()
        .physical
        .as_ref()
        .unwrap()
        .actors
        .values()
        .filter(|a| a.vitals.alive)
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
        match event.kind {
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
        sim.canonical_state().areas.len(),
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

    if alive < population * 9 / 10 {
        let state = sim.canonical_state();
        for (actor, body) in &state.physical.as_ref().unwrap().actors {
            if !body.vitals.alive {
                let clinical = &state.health.as_ref().unwrap().clinical[actor];
                eprintln!(
                    "dead_body actor={} hunger={} fatigue={} thirst={} cold={} illnesses={} age_ms={}",
                    actor.0,
                    body.vitals.hunger,
                    body.vitals.fatigue,
                    clinical.thirst,
                    clinical.cold,
                    clinical.illnesses.len(),
                    state.development.as_ref().unwrap().lives[actor]
                        .age_at(state.time)
                        .unwrap()
                );
            }
        }
    }
    assert!(
        alive >= population * 9 / 10,
        "ordinary workload became mostly dead"
    );
}
