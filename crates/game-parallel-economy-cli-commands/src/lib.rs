//! Personal health, claim and expedition commands.
pub mod player;
use game_parallel_economy::{Economy, MissionStart};
pub fn handle(world: &mut Economy, words: &[&str]) -> Result<bool, String> {
    match words {
        ["eat", person, good] => println!(
            "eaten={}",
            world.eat(
                person.parse().map_err(|_| "invalid person")?,
                good.parse().map_err(|_| "invalid good")?
            )?
        ),
        ["start-work", person, recipe, units] => {
            let events = world.start_work(
                person.parse().map_err(|_| "invalid person")?,
                recipe.parse().map_err(|_| "invalid recipe")?,
                units.parse().map_err(|_| "invalid quantity")?,
            )?;
            println!(
                "started={:?} denied={:?}",
                events.started, events.start_denied
            );
        }
        ["continue-work", person] => {
            let events = world.continue_work(person.parse().map_err(|_| "invalid person")?)?;
            println!(
                "finished={:?} interrupted={:?}",
                events.finished, events.interrupted
            );
        }
        ["knowledge", person] => {
            for record in world.known_food(person.parse().map_err(|_| "invalid person")?)? {
                println!(
                    "region={} food={} observed_at={} witness={} learned_from={:?}",
                    record.region,
                    record.food,
                    record.observed_at,
                    record.witness,
                    record.learned_from
                );
            }
        }
        ["past-knowledge", life] => {
            for record in world.invoked_knowledge(life.parse().map_err(|_| "invalid life")?)? {
                println!(
                    "region={} food={} observed_at={} witness={} learned_from={:?}",
                    record.region,
                    record.food,
                    record.observed_at,
                    record.witness,
                    record.learned_from
                );
            }
        }
        ["meet", actor, target] => {
            let accepted = world.meet(
                actor.parse().map_err(|_| "invalid person")?,
                target.parse().map_err(|_| "invalid target")?,
            )?;
            println!("meeting={}", if accepted { "accepted" } else { "declined" });
        }
        ["skills", person] => {
            let person: u64 = person.parse().map_err(|_| "invalid person")?;
            let state = world.snapshot();
            let skills = state.skills.as_ref().ok_or("skills are disabled")?;
            let index = skills.index();
            for record in skills.records.iter().filter(|r| r.person == person) {
                println!(
                    "person={} recipe={} completions={} rate={}",
                    person,
                    record.recipe,
                    record.completions,
                    index.rate(person, record.recipe)
                );
            }
            for lesson in skills.lessons.iter().filter(|l| l.student == person) {
                println!(
                    "lesson recipe={} teacher={} tick={} source_life={:?}",
                    lesson.recipe, lesson.teacher, lesson.tick, lesson.source_life
                );
            }
        }
        ["ruler"] => {
            let state = world.snapshot();
            let campaign = state.campaign.as_ref().ok_or("campaign is disabled")?;
            println!(
                "current={:?} waiting_since={:?}",
                campaign.lineage.current, campaign.lineage.waiting_since
            );
            if let Some(invocation) = &campaign.lineage.invocation {
                println!(
                    "active={:?} burden={} exposure={} control_until={:?}",
                    invocation.active,
                    invocation.burden,
                    invocation.exposure,
                    invocation.control_until
                );
                if let Some(warning) = &invocation.pending {
                    println!(
                        "warning={:?} revision={} duration={} strain_damage={} lifespan_loss={}",
                        warning.kind,
                        warning.revision,
                        warning.duration,
                        warning.strain_damage_per_tick,
                        warning.lifespan_loss_per_tick
                    );
                }
            }
            if let Some(guardians) = &campaign.guardians {
                for guardian in &guardians.guardians {
                    println!(
                        "guardian={} region={} health={} student={:?} mentorship={} released={} journey={:?}",
                        guardian.id,
                        guardian.region,
                        guardian.health,
                        guardian.student,
                        guardian.mentorship,
                        guardian.released,
                        guardian.journey
                    );
                }
            }
        }
        ["invoke", lives @ ..] if !lives.is_empty() => {
            let lives = lives
                .iter()
                .map(|id| id.parse().map_err(|_| "invalid past life".to_string()))
                .collect::<Result<Vec<u64>, _>>()?;
            world.invoke_lives(&lives)?;
        }
        ["dismiss-invocation"] => world.dismiss_invocation()?,
        ["accept-warning", revision] => world
            .accept_invocation_warning(revision.parse().map_err(|_| "invalid warning revision")?)?,
        ["guardian-release", guardian] => {
            let decision = world
                .request_guardian_release(guardian.parse().map_err(|_| "invalid guardian")?)?;
            println!("guardian decision={decision:?}");
        }
        ["health"] => {
            let state = world.snapshot();
            let dynamics = state
                .dynamics
                .as_ref()
                .ok_or("personal dynamics are disabled")?;
            if let Some(disease) = &dynamics.disease {
                for case in &disease.cases {
                    println!(
                        "person={} strain={} infected_at={} source={:?}",
                        case.person, case.strain, case.infected_at, case.source
                    );
                }
            }
            if let Some(social) = &dynamics.social {
                for event in &social.events {
                    println!(
                        "meeting={} and={} region={}",
                        event.a, event.b, event.region
                    );
                }
            }
        }
        ["claims"] => {
            let state = world.snapshot();
            for claim in &state
                .dynamics
                .as_ref()
                .and_then(|d| d.claims.as_ref())
                .ok_or("claims are disabled")?
                .claims
            {
                println!(
                    "person={} region={} good={}",
                    claim.person, claim.region, claim.good
                );
            }
        }
        ["expeditions"] => {
            let state = world.snapshot();
            for mission in &state
                .missions
                .as_ref()
                .ok_or("expeditions are disabled")?
                .entries
            {
                println!(
                    "id={} person={} origin={} destination={} phase={:?} remaining={}",
                    mission.id,
                    mission.person,
                    mission.origin,
                    mission.destination,
                    mission.phase,
                    mission.remaining
                );
            }
        }
        ["expedition", person, destination, visit] => {
            let id = world.start_mission(MissionStart {
                person: person.parse().map_err(|_| "invalid person")?,
                destination: destination.parse().map_err(|_| "invalid region")?,
                visit_ticks: visit.parse().map_err(|_| "invalid visit")?,
            })?;
            println!("expedition={id}");
        }
        ["cancel-expedition", person, id] => world.cancel_mission(
            person.parse().map_err(|_| "invalid person")?,
            id.parse().map_err(|_| "invalid expedition")?,
        )?,
        ["meet-expedition", person, id, target] => world.set_mission_meeting(
            person.parse().map_err(|_| "invalid person")?,
            id.parse().map_err(|_| "invalid expedition")?,
            target.parse().map_err(|_| "invalid target")?,
        )?,
        ["invite-expedition", person, id, target] => {
            let joined = world.invite_mission(
                person.parse().map_err(|_| "invalid person")?,
                id.parse().map_err(|_| "invalid expedition")?,
                target.parse().map_err(|_| "invalid target")?,
            )?;
            if let Some(joined) = joined {
                println!("joined expedition={joined}");
            } else {
                println!("invitation declined");
            }
        }
        ["claim", person, region, good] => world.claim_stock(
            person.parse().map_err(|_| "invalid person")?,
            region.parse().map_err(|_| "invalid region")?,
            good.parse().map_err(|_| "invalid good")?,
        )?,
        ["unclaim", person, region, good] => world.withdraw_claim(
            person.parse().map_err(|_| "invalid person")?,
            region.parse().map_err(|_| "invalid region")?,
            good.parse().map_err(|_| "invalid good")?,
        )?,
        ["claim-output", person, choice] => world.set_output_claim_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["haul", person, choice] => world.set_hauling_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["take-claimed", person, choice] => world.set_claim_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        _ => return Ok(false),
    }
    Ok(true)
}
