//! Limited interactive access to the interval host through a controlled person.
use game_economy::core_interval::{CoreHost, CoreHostSnapshot, CorePromiseReply};
use game_economy_player::interval::Player;
use std::fs::{File, OpenOptions};
use std::path::Path;

fn numbers<T: std::str::FromStr>(text: &str) -> Result<Vec<T>, String> {
    text.split(',')
        .map(|part| part.parse().map_err(|_| "invalid number list".to_owned()))
        .collect()
}

pub(super) fn save(host: &CoreHost, path: &[&str]) -> Result<(), String> {
    let path = path.join(" ");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path.trim_matches('"'))
        .map_err(|error| error.to_string())?;
    serde_json::to_writer(file, &host.snapshot()).map_err(|error| error.to_string())?;
    println!("saved");
    Ok(())
}

pub(super) fn load(path: &[&str]) -> Result<CoreHost, String> {
    let path = path.join(" ");
    let file = File::open(path.trim_matches('"')).map_err(|error| error.to_string())?;
    let snapshot: CoreHostSnapshot =
        serde_json::from_reader(file).map_err(|error| error.to_string())?;
    CoreHost::resume(snapshot)
}

fn number<T: std::str::FromStr>(value: &str) -> Result<T, String> {
    value.parse().map_err(|_| format!("invalid number {value}"))
}

fn part(specification: &str) -> Result<game_mechanics::Part, String> {
    let fields: Vec<_> = specification.split(':').collect();
    let [item, component, parent, share] = fields.as_slice() else {
        return Err("expected ITEM:COMPONENT:PARENT:SHARE".into());
    };
    let parent: u64 = number(parent)?;
    Ok(game_mechanics::Part {
        id: number(item)?,
        component: number(component)?,
        parent: (parent != 0).then_some(parent),
        share_per_mille: number(share)?,
        broken: false,
    })
}

fn command(host: &mut CoreHost, person: u64, words: &[&str]) -> Result<bool, String> {
    match words {
        [] => {}
        ["help"] => println!(
            "look | wait MILLISECONDS | move REGION | walk stock|store ID | pause-walk | resume-walk | trip REGION VISIT_MILLISECONDS [food] | trip go|survey|back|cancel | trip leave-in MILLISECONDS | fight PERSON | join-fight BENEFICIARY OPPONENT | tactic wait|defend|retreat | tactic attack PERSON | tactic attack-site SITE | tactic move X Y | confine PERSON SITE | guard SITE | release PERSON | escape | build BLUEPRINT | demolish SITE | salvage SITE GOOD QUANTITY | gather NODE | cultivate FIELD [CARRIED_SEED_BATCH] | amend FIELD | assemble APPARATUS PART... | mount APPARATUS STORE | unmount APPARATUS STORE | experiment APPARATUS OUTPUT FORCE DISTANCE LOAD | promise PERSON GOOD QUANTITY DUE_IN_MILLISECONDS | respond PROMISE accept|decline|withdraw | deliver PROMISE QUANTITY | eat GOOD | treat GOOD | wear GOOD | unwear | work RECIPE UNITS | stop JOB | stop-assist | rest on|off | word place|person|object ID SOUNDS | form SOUNDS MARKS | inscribe SOUNDS | read PAGE | recall PAGE | save PATH | load PATH | quit (SOUNDS and MARKS are comma-separated numbers)"
        ),
        ["quit"] | ["exit"] => return Ok(false),
        ["look"] => {
            let view = Player::new(host, person)?.view()?;
            println!(
                "tick={} offset={} person={} region={:?} health={} hunger={}",
                view.tick,
                view.offset_millis,
                view.person.id,
                view.person.region,
                view.person.health,
                view.person.hunger
            );
            for other in &view.nearby {
                println!("nearby={} health={}", other.id, other.health);
            }
            for lot in &view.possessions {
                if let Some(item) = lot.item {
                    println!("carried_item={item} good={}", lot.good);
                } else if let Some(batch) = lot.batch {
                    println!(
                        "carried_good={} quantity={} batch={batch}",
                        lot.good, lot.quantity
                    );
                } else {
                    println!("carried_good={} quantity={}", lot.good, lot.quantity);
                }
            }
            for site in &view.accessible_sites {
                if let Some(kind) = &site.kind {
                    println!("site={} kind={kind}", site.id);
                }
                for lot in &site.debris {
                    println!(
                        "debris_site={} good={} quantity={}",
                        site.id, lot.good, lot.quantity
                    );
                }
            }
            for node in &view.accessible_resources {
                println!(
                    "resource={} good={} available={} yield={}",
                    node.id, node.good, node.available, node.yield_per_action
                );
            }
            for field in &view.accessible_fields {
                if let Some(fertility) = field.fertility {
                    println!("field={} phase={:?} fertility={fertility}", field.id, field.phase);
                } else {
                    println!("field={} phase={:?}", field.id, field.phase);
                }
            }
            let apparatus = view
                .carried_apparatus
                .iter()
                .map(|device| (device, None))
                .chain(
                    view.shared_apparatus
                        .iter()
                        .map(|placed| (&placed.apparatus, Some(placed.store))),
                );
            for (apparatus, store) in apparatus {
                if let Some(store) = store {
                    println!("apparatus={} store={store}", apparatus.id);
                } else {
                    println!("apparatus={}", apparatus.id);
                }
                for part in apparatus.parts.iter() {
                    println!(
                        "part={} component={} parent={:?} share={} broken={}",
                        part.id, part.component, part.parent, part.share_per_mille, part.broken
                    );
                }
            }
            for promise in &view.promises {
                println!(
                    "promise={} debtor={} creditor={} good={} delivered={} quantity={} due={} status={:?}",
                    promise.id,
                    promise.debtor,
                    promise.creditor,
                    promise.good,
                    promise.delivered,
                    promise.quantity,
                    promise.due,
                    promise.status
                );
            }
            if let Some(wound) = view.wound {
                println!("wound_severity={}", wound.severity);
            }
            if let Some(journey) = view.journey {
                println!(
                    "travel_destination={} remaining={}",
                    journey.destination, journey.remaining
                );
            }
            if let Some(location) = view.local_location {
                println!("local_location={location:?}");
            }
            if let Some(site) = view.held_at {
                println!("held_at={site}");
            }
            if let Some(plan) = &view.expedition {
                println!(
                    "trip={} destination={} stage={:?} objective_met={}",
                    plan.id, plan.destination, plan.stage, plan.objective_met
                );
            }
            if let Some(tactics) = &view.tactics {
                println!(
                    "fight={} x={} y={} next_round={}",
                    tactics.engagement,
                    tactics.position.x,
                    tactics.position.y,
                    tactics.next_round_millis
                );
                for (person, position) in &tactics.visible {
                    println!("fighter={person} x={} y={}", position.x, position.y);
                }
                for (site, position) in &tactics.visible_sites {
                    println!("tactical_site={site} at={},{}", position.x, position.y);
                }
            }
            for report in view.route_reports {
                let observed = report.observed_at_millis.map_or_else(
                    || format!("tick {}", report.observed_at),
                    |instant| format!("{instant} simulated ms"),
                );
                println!(
                    "route {} to {} was reported {} at {observed}",
                    report.source,
                    report.destination,
                    if report.open { "open" } else { "closed" }
                );
            }
            if let Some(job) = view.job {
                println!(
                    "job={} recipe={} remaining={}",
                    job.id, job.recipe, job.remaining
                );
            }
            if let Some(fatigue) = view.fatigue {
                println!("fatigue={} resting={}", fatigue.fatigue, fatigue.resting);
            }
            if let Some(language) = view.language {
                for word in language.words {
                    println!("word={:?} sounds={:?}", word.meaning, word.sounds);
                }
                for form in language.associations {
                    println!("form_sounds={:?} marks={:?}", form.sounds, form.marks);
                }
                for memory in language.observations {
                    println!(
                        "remembered_page={} marks={:?} heard={:?}",
                        memory.page, memory.marks, memory.heard
                    );
                }
            }
            for page in view.marked_pages {
                println!("marked_page={} marks={:?}", page.item, page.marks);
            }
        }
        ["wait", millis] => {
            let millis = millis.parse::<u64>().map_err(|_| "invalid milliseconds")?;
            let result = Player::new(host, person)?.advance(millis)?;
            println!(
                "elapsed={} boundaries={}",
                result.elapsed_millis, result.boundaries
            );
            if let Some(error) = result.failure {
                return Err(error);
            }
        }
        ["move", region] => {
            let region = region.parse::<u64>().map_err(|_| "invalid region")?;
            println!("departed={}", Player::new(host, person)?.depart(region)?);
        }
        ["walk", "stock"] => {
            println!(
                "walk={:?}",
                Player::new(host, person)?.walk(game_economy::LocalEndpoint::Stock)?
            );
        }
        ["walk", "store", store] => {
            let id = store.parse::<u64>().map_err(|_| "invalid store")?;
            println!(
                "walk={:?}",
                Player::new(host, person)?.walk(game_economy::LocalEndpoint::Store { id })?
            );
        }
        ["pause-walk"] | ["resume-walk"] => {
            println!(
                "walk_changed={}",
                Player::new(host, person)?.pause_walk(words[0] == "pause-walk")?
            );
        }
        ["trip", "invites"] => {
            for invitation in Player::new(host, person)?.expedition_invitations()? {
                println!(
                    "invitation={} from={} destination={} depart_after_millis={:?}",
                    invitation.id,
                    invitation.inviter,
                    invitation.destination,
                    invitation.depart_after_millis
                );
            }
        }
        ["trip", "invite", target] => {
            let target = target.parse().map_err(|_| "invalid invitee")?;
            let offer = Player::new(host, person)?.invite_expedition(target)?;
            println!(
                "invitation={} sent=true response={:?}",
                offer.invitation.id,
                offer.response.map(|reply| reply.response)
            );
        }
        ["trip", "reply", invitation, "yes" | "no"] => {
            let invitation = invitation.parse().map_err(|_| "invalid invitation ID")?;
            let reply =
                Player::new(host, person)?.reply_to_expedition(invitation, words[3] == "yes")?;
            println!("invitation={} response={:?}", invitation, reply.response);
        }
        ["trip", "withdraw", invitation] => {
            let invitation = invitation.parse().map_err(|_| "invalid invitation ID")?;
            Player::new(host, person)?.withdraw_expedition_invitation(invitation)?;
            println!("invitation={} withdrawn=true", invitation);
        }
        ["trip", "leave-in", delay] => {
            let delay = delay.parse().map_err(|_| "invalid departure delay")?;
            let event = Player::new(host, person)?.schedule_expedition_departure(delay)?;
            println!("trip={} event={:?}", event.itinerary, event.kind);
        }
        ["trip", "go" | "survey" | "back" | "cancel"] => {
            let mut player = Player::new(host, person)?;
            let events = match words[1] {
                "go" => player.continue_expedition()?,
                "survey" => player.survey_expedition()?,
                "back" => player.return_from_expedition()?,
                _ => vec![player.abandon_expedition()?],
            };
            for event in events {
                println!(
                    "trip={} event={:?} region={} at_millis={}",
                    event.itinerary, event.kind, event.region, event.at_millis
                );
                if let Some(food) = event.food_observed {
                    println!("food_observed={food}");
                }
            }
        }
        ["trip", region, duration] | ["trip", region, duration, "food"] => {
            let region = region.parse().map_err(|_| "invalid region")?;
            let duration = duration.parse().map_err(|_| "invalid visit duration")?;
            let objective =
                (words.len() == 4).then_some(game_economy::MissionObjective::ObserveFood);
            let event = Player::new(host, person)?.plan_expedition(region, duration, objective)?;
            println!("trip={} planned=true", event.itinerary);
        }
        ["fight", target] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            let event = Player::new(host, person)?.confront(target)?;
            println!("fight={} started=true", event.engagement);
        }
        ["join-fight", beneficiary, opponent] => {
            let beneficiary = beneficiary.parse().map_err(|_| "invalid beneficiary")?;
            let opponent = opponent.parse().map_err(|_| "invalid opponent")?;
            let event = host.join_controlled_engagement(person, beneficiary, opponent)?;
            println!("engagement={} outcome={:?}", event.engagement, event.kind);
        }
        ["tactic", "wait" | "defend" | "retreat"] => {
            use game_economy::core_interval::TacticalChoice;
            let action = match words[1] {
                "wait" => TacticalChoice::Wait,
                "defend" => TacticalChoice::Defend,
                _ => TacticalChoice::Retreat,
            };
            Player::new(host, person)?.choose_tactics(action)?;
        }
        ["tactic", "attack", target] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            Player::new(host, person)?.choose_tactics(
                game_economy::core_interval::TacticalChoice::Attack {
                    target,
                    pursue: true,
                },
            )?;
        }
        ["tactic", "attack-site", site] => {
            use game_economy::core_interval::TacticalChoice;
            let site = site.parse().map_err(|_| "invalid site")?;
            Player::new(host, person)?.choose_tactics(TacticalChoice::AttackSite { site })?;
        }
        ["tactic", "move", x, y] => {
            use game_economy::core_interval::{TacticalChoice, TacticalPoint};
            let x = x.parse::<u32>().map_err(|_| "invalid position")?;
            let y = y.parse::<u32>().map_err(|_| "invalid position")?;
            Player::new(host, person)?.choose_tactics(TacticalChoice::Move {
                destination: TacticalPoint { x, y },
            })?;
        }
        ["confine", target, site] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            println!(
                "confined={}",
                Player::new(host, person)?.confine(target, site)?
            );
        }
        ["guard", site] => {
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            Player::new(host, person)?.guard(site)?;
            println!("guarded={site}");
        }
        ["release", target] => {
            let target = target.parse::<u64>().map_err(|_| "invalid person")?;
            println!("released={}", Player::new(host, person)?.release(target)?);
        }
        ["escape"] => println!("escaped={}", Player::new(host, person)?.escape()?),
        ["build", blueprint] => {
            let blueprint = blueprint.parse::<u32>().map_err(|_| "invalid blueprint")?;
            println!(
                "built_site={:?}",
                Player::new(host, person)?.build(blueprint)?
            );
        }
        ["demolish", site] => {
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            println!("demolished={}", Player::new(host, person)?.demolish(site)?);
        }
        ["salvage", site, good, quantity] => {
            let site = site.parse::<u64>().map_err(|_| "invalid site")?;
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            let quantity = quantity.parse::<u64>().map_err(|_| "invalid quantity")?;
            println!(
                "salvaged={}",
                Player::new(host, person)?.salvage(site, good, quantity)?
            );
        }
        ["eat", good] => {
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            println!("consumed={}", Player::new(host, person)?.eat(good)?);
        }
        ["wear", good] => {
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            Player::new(host, person)?.choose_clothing(Some(good))?;
            println!("wearing={good}");
        }
        ["unwear"] => {
            Player::new(host, person)?.choose_clothing(None)?;
            println!("no protective garment selected");
        }
        ["treat", good] => {
            let good = good.parse::<u32>().map_err(|_| "invalid good")?;
            println!("consumed={}", Player::new(host, person)?.treat_wound(good)?);
        }
        ["assemble", id, specs @ ..] if !specs.is_empty() => {
            let parts = specs
                .iter()
                .map(|spec| part(spec))
                .collect::<Result<Vec<_>, _>>()?;
            let apparatus = game_mechanics::Apparatus {
                id: number(id)?,
                parts: parts.into(),
            };
            let assembled = Player::new(host, person)?.assemble(apparatus)?;
            println!("assembled={assembled}");
        }
        ["experiment", id, output, force, distance, load] => {
            let result = Player::new(host, person)?.experiment(game_mechanics::Experiment {
                actor: person,
                apparatus: number(id)?,
                output_part: number(output)?,
                input_force: number(force)?,
                input_distance: number(distance)?,
                load_force: number(load)?,
            })?;
            println!(
                "outcome={:?} available_force={} travel={} delivered_work={} broken_parts={:?}",
                result.outcome,
                result.available_force,
                result.travel,
                result.delivered_work,
                result.broken_parts
            );
        }
        ["promise", recipient, good, quantity, until] => {
            let id = Player::new(host, person)?.promise(
                number(recipient)?,
                number(good)?,
                number(quantity)?,
                number(until)?,
            )?;
            println!("promise={id} proposed");
        }
        ["respond", pledge, choice] => {
            let reply = match *choice {
                "accept" => CorePromiseReply::Accept,
                "decline" => CorePromiseReply::Decline,
                "withdraw" => CorePromiseReply::Withdraw,
                _ => return Err("expected accept, decline, or withdraw".into()),
            };
            Player::new(host, person)?.respond_to_promise(number(pledge)?, reply)?;
            println!("promise response recorded");
        }
        ["deliver", pledge, quantity] => {
            let accepted =
                Player::new(host, person)?.deliver_promise(number(pledge)?, number(quantity)?)?;
            println!("delivered={accepted}");
        }
        ["gather", node] => {
            let node = node.parse::<u64>().map_err(|_| "invalid resource node")?;
            if let Some(yielded) = Player::new(host, person)?.gather(node)? {
                println!("gathered={} quantity={}", yielded.good, yielded.quantity);
            } else {
                println!("nothing gathered");
            }
        }
        [command @ ("mount" | "unmount"), apparatus, store] => {
            let moved = Player::new(host, person)?.move_apparatus(
                number(apparatus)?,
                number(store)?,
                *command == "mount",
            )?;
            println!("apparatus_moved={moved}");
        }
        ["cultivate", field, seeds @ ..] if seeds.len() <= 1 => {
            let field = number(field)?;
            let batch = seeds.first().map(|batch| number(batch)).transpose()?;
            if let Some(phase) = Player::new(host, person)?.cultivate(field, batch)? {
                println!("field={field} phase={phase:?}");
            } else {
                println!("field={field} cultivation denied");
            }
        }
        ["amend", field] => {
            let field = number(field)?;
            let amended = Player::new(host, person)?.amend(field)?;
            println!("field={field} amended={amended}");
        }
        ["work", recipe, units] => {
            let recipe = recipe.parse::<u32>().map_err(|_| "invalid recipe")?;
            let units = units.parse::<u32>().map_err(|_| "invalid units")?;
            if let Some(job) = Player::new(host, person)?.start_work(recipe, units)? {
                println!("job={} started=true", job);
            } else {
                println!("started=false");
            }
        }
        ["stop", job] => {
            let job = job.parse::<u64>().map_err(|_| "invalid job")?;
            Player::new(host, person)?.stop_work(job)?;
            println!("stopped");
        }
        ["stop-assist"] => {
            Player::new(host, person)?.stop_assisting()?;
            println!("stopped assisting");
        }
        ["rest", choice @ ("on" | "off")] => {
            let resting = *choice == "on";
            Player::new(host, person)?.set_rest(resting)?;
            println!("resting={resting}");
        }
        ["form", sounds, marks] => {
            Player::new(host, person)?.choose_written_form(numbers(sounds)?, numbers(marks)?)?;
            println!("form chosen");
        }
        ["word", kind, referent, sounds] => {
            let meaning = match *kind {
                "place" => {
                    game_language::Meaning::Place(referent.parse().map_err(|_| "invalid place")?)
                }
                "person" => {
                    game_language::Meaning::Person(referent.parse().map_err(|_| "invalid person")?)
                }
                "object" => {
                    game_language::Meaning::Object(referent.parse().map_err(|_| "invalid object")?)
                }
                _ => return Err("unknown word referent".into()),
            };
            Player::new(host, person)?.choose_word(meaning, numbers(sounds)?)?;
            println!("word chosen");
        }
        ["inscribe", sounds] => {
            if let Some(item) = Player::new(host, person)?.inscribe(numbers(sounds)?)? {
                println!("inscribed_page={item}");
            } else {
                println!("inscribed=false");
            }
        }
        ["read", item] => {
            let item = item.parse::<u64>().map_err(|_| "invalid page")?;
            let observation = Player::new(host, person)?.observe_marked_page(item)?;
            println!(
                "marks={:?} meanings={:?}",
                observation.marks, observation.meanings
            );
        }
        ["recall", item] => {
            let item = item.parse::<u64>().map_err(|_| "invalid page")?;
            Player::new(host, person)?.adopt_remembered_form(item)?;
            println!("remembered form adopted");
        }
        ["save", path @ ..] if !path.is_empty() => {
            host.controlled_person(person)?;
            save(host, path)?;
        }
        ["load", path @ ..] if !path.is_empty() => {
            let loaded = load(path)?;
            loaded.controlled_person(person)?;
            *host = loaded;
            println!("loaded");
        }
        _ => return Err("unknown interval player command; enter help".into()),
    }
    Ok(true)
}

fn campaign_command(host: &mut CoreHost, words: &[&str]) -> Result<bool, String> {
    let view = host
        .campaign_view()
        .ok_or("this scenario has no interval campaign")?;
    let controlled = || {
        view.controlled
            .ok_or("no incarnation currently available for direct action")
    };
    match words {
        [] => {}
        ["quit" | "exit"] => return Ok(false),
        ["campaign"] | ["look"] if view.controlled.is_none() || words[0] == "campaign" => {
            println!(
                "incarnation={:?} controlled={:?}",
                view.current, view.controlled
            );
            for life in &view.past_lives {
                println!(
                    "past_life={} started={} ended={}",
                    life.person, life.started, life.ended
                );
            }
            for guardian in &view.guardians {
                println!(
                    "guardian={} student={:?} mentorship_millis={} released={}",
                    guardian.person, guardian.student, guardian.observed_millis, guardian.released
                );
            }
            if let Some(warning) = view.warning {
                println!(
                    "invocation_warning={:?} revision={} projected_burden={}",
                    warning.kind, warning.revision, warning.projected_burden
                );
            }
        }
        ["wait", duration] => {
            let duration = duration.parse().map_err(|_| "invalid duration")?;
            let run = host.advance_continuous(duration, &[])?;
            println!(
                "elapsed_millis={} boundaries={}",
                run.elapsed_millis, run.boundaries
            );
            if let Some(error) = run.failure {
                return Err(error);
            }
        }
        ["invoke", lives] => host.activate_past_lives(controlled()?, &numbers(lives)?)?,
        ["recall-life", life] => {
            let life = life.parse().map_err(|_| "invalid past life")?;
            let memory = host.consult_past_life(controlled()?, life)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&memory).map_err(|error| error.to_string())?
            );
        }
        ["dismiss"] => host.dismiss_past_lives(controlled()?)?,
        ["accept-cost", revision] => {
            let revision = revision.parse().map_err(|_| "invalid warning revision")?;
            host.accept_past_life_warning(controlled()?, revision)?;
        }
        ["release-mentor", guardian] => {
            let guardian = guardian.parse().map_err(|_| "invalid guardian")?;
            println!(
                "mentorship_released={}",
                host.ask_guardian_release(controlled()?, guardian)?
                    .is_some()
            );
        }
        ["save", path @ ..] if !path.is_empty() => save(host, path)?,
        ["load", path @ ..] if !path.is_empty() => {
            let loaded = load(path)?;
            loaded.campaign_view().ok_or("save has no campaign")?;
            *host = loaded;
            println!("loaded");
        }
        ["help"] => {
            println!(
                "campaign | invoke PAST_LIVES | recall-life LIFE | dismiss | accept-cost REVISION | release-mentor GUARDIAN"
            );
            if let Some(person) = view.controlled {
                return command(host, person, words);
            }
        }
        _ => return command(host, controlled()?, words),
    }
    Ok(true)
}

pub fn run(
    root: &Path,
    scenario: &Path,
    person: Option<u64>,
    seed: Option<u64>,
    population: Option<&Path>,
) -> Result<(), String> {
    let mut host = game_economy_cli::core_scenario_with_options(
        scenario,
        &root.join("content"),
        seed,
        population,
    )?;
    if let Some(person) = person {
        host.controlled_person(person)?;
    } else {
        host.campaign_view()
            .ok_or("this scenario has no interval campaign")?;
    }
    println!("Interval player; enter help for commands.");
    crate::terminal::run(|words| {
        if let Some(person) = person {
            command(&mut host, person, words)
        } else {
            campaign_command(&mut host, words)
        }
    })
}
