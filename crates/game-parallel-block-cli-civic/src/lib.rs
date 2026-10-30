//! Administrative commands for property, conflict, and care.
mod agreements;
mod care;
mod family;
use game_parallel_fused_physical::{Kernel as Engine, PropertyCommand};
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
fn handle_civic(engine: &mut Engine, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("social-pressure") => {
            let person = number(&mut words, "person")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("expected enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_social_pressure(person, enabled)?;
            println!("social-pressure person={person} enabled={enabled}");
        }
        Some("observe") => {
            end(&mut words)?;
            let view = game_parallel_observation::ruler(&engine.export())?;
            println!(
                "tick={} ruler={} health={} hunger={} location={:?}",
                view.tick, view.ruler.id, view.ruler.health, view.ruler.hunger, view.location
            );
            game_parallel_block_cli_goals::print_personal(&view);
            game_parallel_block_cli_goals::print_season(view.season.as_deref());
            println!("carried_food={}", view.carried_food);
            if let Some(tactical) = view.tactical {
                println!(
                    "tactical encounter={} x={} y={} nearby={:?}",
                    tactical.encounter, tactical.position.x, tactical.position.y, tactical.nearby
                );
            }
            for item in view.carried_goods {
                println!("carried good={} quantity={}", item.good, item.quantity);
            }
            for job in view.jobs {
                println!(
                    "job={} recipe={} remaining={} phase={:?}",
                    job.id, job.recipe, job.remaining, job.phase
                );
            }
            if let Some(fatigue) = view.fatigue {
                println!("fatigue={} resting={}", fatigue.fatigue, fatigue.resting);
            }
            if let Some(nutrition) = view.nutrition {
                println!(
                    "nutrients energy={} protein={} fat={} fibre={} vitamins={} minerals={}",
                    nutrition.reserves[0],
                    nutrition.reserves[1],
                    nutrition.reserves[2],
                    nutrition.reserves[3],
                    nutrition.reserves[4],
                    nutrition.reserves[5]
                );
            }
            if view.illness_symptoms {
                println!("symptoms=illness");
            }
            if let Some(isolation) = view.isolation {
                println!("isolation={isolation}");
            }
            if let Some(disposition) = view.disposition {
                println!(
                    "travel_caution={} hunger_urgency={}",
                    disposition.travel_caution, disposition.hunger_urgency
                );
            }
            if let Some(purpose) = view.purpose {
                println!("purpose_deprivation={}", purpose.deprivation);
            }
            game_parallel_block_cli_goals::print(&view.goals);
            game_parallel_block_cli_goals::print_dreams(&view.dreams);
            game_parallel_block_cli_missions::print_view(&view.missions, &view.outgoing_routes);
            for event in view.caregiving {
                println!("care child={} caregiver={:?}", event.child, event.caregiver);
            }
            for skill in view.skills {
                println!(
                    "skill recipe={} completions={}",
                    skill.recipe, skill.completions
                );
            }
            for opinion in view.opinions {
                println!(
                    "opinion target={} trust={} fear={} obligation={}",
                    opinion.target, opinion.trust, opinion.fear, opinion.obligation
                );
            }
            for record in view.records {
                println!(
                    "report region={} food={} observed_at={} witness={} learned_from={:?}",
                    record.region,
                    record.food,
                    record.observed_at,
                    record.witness,
                    record.learned_from
                );
            }
        }
        Some("spectate-guardian") => {
            let id = number(&mut words, "guardian")?;
            end(&mut words)?;
            println!(
                "{:?}",
                game_parallel_observation::guardian_search(&engine.export(), id)?
            );
        }
        Some("opinions") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let opinions = world.opinions.as_ref().ok_or("opinions are disabled")?;
            for opinion in opinions
                .records
                .iter()
                .filter(|opinion| opinion.observer == person)
            {
                println!(
                    "observer={} target={} trust={} fear={} obligation={}",
                    opinion.observer,
                    opinion.target,
                    opinion.trust,
                    opinion.fear,
                    opinion.obligation
                );
            }
        }
        Some("disease") => {
            end(&mut words)?;
            let world = engine.export();
            let disease = world.disease.as_ref().ok_or("disease is disabled")?;
            for case in &disease.cases {
                println!(
                    "person={} strain={} infected_at={} source={:?}",
                    case.person, case.strain, case.infected_at, case.source
                );
            }
            for event in &disease.events {
                println!(
                    "tick={} person={} strain={} event={:?} source={:?}",
                    event.tick, event.person, event.strain, event.kind, event.source
                );
            }
        }
        Some("exposure") => {
            end(&mut words)?;
            let world = engine.export();
            let environment = world
                .environment
                .as_ref()
                .ok_or("environment is disabled")?;
            for hazard in &environment.regions {
                let goods = hazard
                    .shelter_good
                    .and_then(|good| {
                        world
                            .production
                            .as_ref()
                            .and_then(|production| {
                                production
                                    .inventories
                                    .iter()
                                    .find(|inventory| inventory.region == hazard.region)
                            })
                            .and_then(|inventory| {
                                inventory.stocks.iter().find(|stock| stock.good == good)
                            })
                            .map(|stock| stock.quantity)
                    })
                    .unwrap_or(0);
                println!(
                    "region={} damage={} shelter={} shelter_good={:?} reduction={}",
                    hazard.region,
                    hazard.damage,
                    u64::from(hazard.shelter).saturating_add(goods),
                    hazard.shelter_good,
                    hazard.shelter_reduction
                );
            }
        }
        Some("properties") => {
            end(&mut words)?;
            let world = engine.export();
            let property = world.property.as_ref().ok_or("property is disabled")?;
            for claim in &property.claims {
                println!(
                    "region={} good={} owner={} delegates={:?}",
                    claim.region, claim.good, claim.owner, claim.delegates
                );
            }
            for offer in &property.offers {
                println!(
                    "offer={} region={} good={} proposer={} recipient={}",
                    offer.id, offer.region, offer.good, offer.proposer, offer.recipient
                );
            }
        }
        Some("property-offer") => {
            let owner = number(&mut words, "owner")?;
            let recipient = number(&mut words, "recipient")?;
            let region = number(&mut words, "region")?;
            let good = number(&mut words, "good")?;
            end(&mut words)?;
            println!(
                "property_offer={}",
                engine
                    .property_command(PropertyCommand::Offer {
                        owner,
                        recipient,
                        region,
                        good
                    })?
                    .expect("offer id")
            );
        }
        Some("property-decide") => {
            let id = number(&mut words, "offer")?;
            let actor = number(&mut words, "actor")?;
            let accept = match words.next() {
                Some("accept") => true,
                Some("decline") => false,
                _ => return Err("decision must be accept or decline".into()),
            };
            end(&mut words)?;
            engine.property_command(PropertyCommand::Decide { id, actor, accept })?;
        }
        Some("property-access") => {
            let owner = number(&mut words, "owner")?;
            let recipient = number(&mut words, "person")?;
            let region = number(&mut words, "region")?;
            let good = number(&mut words, "good")?;
            let grant = match words.next() {
                Some("grant") => true,
                Some("revoke") => false,
                _ => return Err("access must be grant or revoke".into()),
            };
            end(&mut words)?;
            if grant {
                engine.property_command(PropertyCommand::Grant {
                    owner,
                    recipient,
                    region,
                    good,
                })?;
            } else {
                engine.property_command(PropertyCommand::Revoke {
                    owner,
                    recipient,
                    region,
                    good,
                })?;
            }
        }
        Some("property-withdraw") => {
            let id = number(&mut words, "offer")?;
            let actor = number(&mut words, "actor")?;
            end(&mut words)?;
            engine.property_command(PropertyCommand::Withdraw { id, actor })?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub fn handle(engine: &mut Engine, line: &str) -> Result<bool, String> {
    if family::handle(engine, line)?
        || agreements::handle(engine, line)?
        || game_parallel_block_cli_caregiving::handle(engine, line)?
        || game_parallel_block_cli_missions::handle(engine, line)?
        || game_parallel_block_cli_goals::handle(engine, line)?
        || care::handle(engine, line)?
        || game_parallel_block_cli_interface::combat::handle(engine, line)?
    {
        return Ok(true);
    }
    handle_civic(engine, line)
}
