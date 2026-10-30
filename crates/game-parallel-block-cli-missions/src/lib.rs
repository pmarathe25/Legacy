//! Explicit personal expedition commands.
use game_parallel::MissionStart;
use game_parallel_fused_physical::Kernel;
fn number<T: std::str::FromStr>(
    words: &mut std::str::SplitWhitespace<'_>,
    name: &str,
) -> Result<T, String> {
    words
        .next()
        .ok_or_else(|| format!("missing {name}"))?
        .parse()
        .map_err(|_| format!("invalid {name}"))
}
fn end(words: &mut std::str::SplitWhitespace<'_>) -> Result<(), String> {
    if words.next().is_some() {
        Err("unexpected argument".into())
    } else {
        Ok(())
    }
}
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("mission-start") => {
            let person = number(&mut words, "person")?;
            let destination = number(&mut words, "destination")?;
            let visit_ticks = number(&mut words, "visit ticks")?;
            end(&mut words)?;
            let id = engine.start_mission(MissionStart {
                person,
                destination,
                visit_ticks,
            })?;
            println!("mission={id}");
        }
        Some("mission-meet") => {
            let person = number(&mut words, "person")?;
            let mission = number(&mut words, "mission")?;
            let target = number(&mut words, "target")?;
            end(&mut words)?;
            engine.set_mission_meeting(person, mission, target)?;
        }
        Some("mission-candidates") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let missions = world.missions.as_ref().ok_or("missions are disabled")?;
            let own = missions
                .entries
                .iter()
                .find(|mission| {
                    mission.person == person && mission.phase == game_parallel::MissionPhase::Ready
                })
                .ok_or("no ready expedition")?;
            let local = world
                .regions
                .iter()
                .find(|region| {
                    region.id == own.origin && region.people.iter().any(|row| row.id == person)
                })
                .ok_or("person must be local")?;
            for candidate in missions.entries.iter().filter(|candidate| {
                candidate.person != person
                    && candidate.party.is_none()
                    && candidate.party_opt_in
                    && candidate.phase == game_parallel::MissionPhase::Ready
                    && (
                        candidate.origin,
                        candidate.destination,
                        candidate.visit_ticks,
                    ) == (own.origin, own.destination, own.visit_ticks)
                    && local
                        .people
                        .iter()
                        .any(|row| row.id == candidate.person && row.health > 0)
            }) {
                println!(
                    "candidate person={} mission={}",
                    candidate.person, candidate.id
                );
            }
        }
        Some("mission-consent") => {
            let person = number(&mut words, "person")?;
            let mission = number(&mut words, "mission")?;
            let consent = match words.next() {
                Some("accept") => true,
                Some("withdraw") => false,
                _ => return Err("consent must be accept or withdraw".into()),
            };
            end(&mut words)?;
            engine.set_mission_party_consent(person, mission, consent)?;
        }
        Some("mission-invite") => {
            let person = number(&mut words, "person")?;
            let mission = number(&mut words, "mission")?;
            let target = number(&mut words, "target")?;
            end(&mut words)?;
            match engine.invite_mission(person, mission, target)? {
                Some(id) => println!("invitation=joined mission={id}"),
                None => println!("invitation=declined"),
            }
        }
        Some("mission-party") => {
            let ids: Vec<u64> = words
                .map(|word| word.parse().map_err(|_| "invalid mission".to_string()))
                .collect::<Result<_, _>>()?;
            println!("party={}", engine.form_mission_party(&ids)?);
        }
        Some("mission-cancel") => {
            let person = number(&mut words, "person")?;
            let id = number(&mut words, "mission")?;
            end(&mut words)?;
            engine.cancel_mission(person, id)?;
        }
        Some("missions") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let missions = world.missions.as_ref().ok_or("missions are disabled")?;
            print(
                &missions
                    .entries
                    .iter()
                    .filter(|mission| mission.person == person)
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            for event in missions
                .events
                .iter()
                .filter(|event| event.person == person)
            {
                println!(
                    "mission-event={} kind={:?} tick={}",
                    event.mission, event.kind, event.tick
                );
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
pub fn print(missions: &[game_parallel::Mission]) {
    for mission in missions {
        println!(
            "mission={} person={} party={:?} party_opt_in={} party_compliant={} origin={} destination={} phase={:?} remaining={} objective={:?} objective_met={}",
            mission.id,
            mission.person,
            mission.party,
            mission.party_opt_in,
            mission.party_compliant,
            mission.origin,
            mission.destination,
            mission.phase,
            mission.remaining,
            mission.objective,
            mission.objective_met
        );
    }
}

/// Show local route choices and the ruler's own expeditions.
pub fn print_view(missions: &[game_parallel::Mission], routes: &[game_parallel::Route]) {
    print(missions);
    for route in routes {
        println!(
            "route destination={} ticks={}",
            route.destination, route.ticks
        );
    }
}
