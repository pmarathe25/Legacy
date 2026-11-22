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
            "look | wait MILLISECONDS | move REGION | walk stock|store ID | pause-walk | resume-walk | trip REGION VISIT_MILLISECONDS [food] | trip go|survey|back|cancel | trip leave-in MILLISECONDS | trip meet GROUP MINIMUM WAIT_MILLISECONDS | trip meet off | fight PERSON | join-fight BENEFICIARY OPPONENT | tactic wait|defend|retreat | tactic attack PERSON | tactic attack-site SITE | tactic move X Y | confine PERSON SITE | guard SITE | release PERSON | escape | build BLUEPRINT | demolish SITE | salvage SITE GOOD QUANTITY | gather NODE | cultivate FIELD [CARRIED_SEED_BATCH] | amend FIELD | water FIELD QUANTITY | pastime ACTIVITY | pastime stop | report-food on|off | misreport-food DELTA [PERSON] | food-reports | listen-for ACTIVITY VALUE | performances | memories [PERSON] | reflect-memory PERSON | reflect-memory activity ACTIVITY | findings | assemble APPARATUS PART... | mount APPARATUS STORE | unmount APPARATUS STORE | experiment APPARATUS OUTPUT FORCE DISTANCE LOAD | promise PERSON GOOD QUANTITY DUE_IN_MILLISECONDS | respond PROMISE accept|decline|withdraw | deliver PROMISE QUANTITY | eat GOOD | treat GOOD | wear GOOD | unwear | work RECIPE UNITS | stop JOB | stop-assist | rest on|off | word place|person|object ID SOUNDS | form SOUNDS MARKS | writing-surfaces | writing-surface GOOD|default | inscribe SOUNDS | compose SOUNDS SOUNDS ... | read-work PAGE | read PAGE | recall PAGE | save PATH | load PATH | quit (SOUNDS and MARKS are comma-separated numbers)"
        ),
        ["quit"] | ["exit"] => return Ok(false),
        ["look"] => {
            let player = Player::new(host, person)?;
            let view = player.view()?;
            for (id, name) in player.available_pastimes()? {
                println!("activity={id} {name}");
            }
            for record in player.pastime_practice()? {
                println!("practiced_skill={} elapsed_millis={} completed={}", record.skill, record.millis, record.completed);
            }
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
                print!("field={} phase={:?}", field.id, field.phase);
                if let Some(fertility) = field.fertility {
                    print!(" fertility={fertility}");
                }
                if let Some(moisture) = field.moisture {
                    print!(" moisture={moisture}");
                }
                println!();
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
                if let Some(rendezvous) = plan.rendezvous {
                    println!("rendezvous={} minimum_companions={} wait_until_millis={}",
                        rendezvous.group, rendezvous.minimum_companions, rendezvous.wait_until_millis);
                }
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
                if let Some(marker) = language.role_marker {
                    println!("role_marker_sound={} role={:?}", marker.sound, marker.role);
                }
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
            for item in view.written_works {
                println!("written_work={item}");
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
                    "invitation={} from={} destination={} depart_after_millis={:?} rendezvous_group={:?}",
                    invitation.id,
                    invitation.inviter,
                    invitation.destination,
                    invitation.depart_after_millis,
                    invitation.rendezvous_group
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
        ["trip", "meet", "off"] => {
            let event = Player::new(host, person)?.set_expedition_rendezvous(None)?;
            println!("rendezvous_cleared_at_millis={}", event.at_millis);
        }
        ["trip", "meet", group, minimum, wait] => {
            let minimum = minimum.parse::<u32>().map_err(|_| "invalid minimum companions")?;
            let event = Player::new(host, person)?.set_expedition_rendezvous(
                Some((number(group)?, minimum, number(wait)?)),
            )?;
            println!("rendezvous_set_at_millis={}", event.at_millis);
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
        ["pastime", "stop"] => {
            let stopped = Player::new(host, person)?.stop_pastime()?;
            println!("activity_stopped={stopped}");
        }
        ["pastime", id] => {
            let activity = id.parse::<u32>().map_err(|_| "invalid activity")?;
            let started = Player::new(host, person)?.start_pastime(activity)?;
            println!("activity={activity} started={started}");
        }
        ["report-food", choice @ ("on" | "off")] => {
            host.set_food_reporting_controlled(person, *choice == "on")?;
            println!("food disclosure preference updated");
        }
        ["misreport-food", delta, targets @ ..] if targets.len() <= 1 => {
            let target = targets.first().map(|target| number(target)).transpose()?;
            host.set_food_misreport_controlled(person, number(delta)?, target)?;
            println!("food statement preference updated");
        }
        ["food-reports"] => {
            let records = host.food_reports_controlled(person)?;
            if records.is_empty() {
                println!("No remembered food reports.");
            }
            for row in records {
                println!(
                    "region={} food={} observed_tick={} witness={} learned_from={:?}",
                    row.region, row.food, row.observed_at, row.witness, row.learned_from,
                );
            }
        }
        ["listen-for", activity, value] => {
            host.listen_for_controlled(person, number(activity)?, number(value)?)?;
            println!("listening preference updated");
        }
        ["present", activity, choice @ ("on" | "off")] => {
            host.present_performance_controlled(person, number(activity)?, *choice == "on")?;
            println!("presentation preference updated");
        }
        ["performances"] => {
            for record in host.heard_performances_controlled(person)? {
                println!("heard performer={} activity={} at_millis={}", record.performer, record.activity, record.at_millis);
            }
        }
        ["memories", other @ ..] if other.len() <= 1 => {
            let other = other.first().map(|id| number(id)).transpose()?;
            let memories = Player::new(host, person)?.memories(other)?;
            if memories.is_empty() {
                println!("No accessible memories.");
            }
            for memory in memories {
                let time = memory.precise_time.map_or_else(|| "uncertain".to_owned(), |now| now.to_string());
                match memory.cue {
                    game_economy::core_interval::EpisodeCue::Meeting { other, region } => {
                        println!("met person={other} region={region} time_millis={time} strength={}", memory.strength);
                    }
                    game_economy::core_interval::EpisodeCue::CreativePractice { activity } => {
                        println!("practiced activity={activity} time_millis={time} strength={}", memory.strength);
                    }
                }
            }
        }
        ["reflect-memory", other] => {
            let mut player = Player::new(host, person)?;
            let memories = player.memories(Some(number(other)?))?;
            let reflected = match memories.first() {
                Some(memory) => player.reflect_on_memory(memory.cue)?,
                None => false,
            };
            println!("memory_reinforced={reflected}");
        }
        ["reflect-memory", "activity", id] => {
            let activity = id.parse::<u32>().map_err(|_| "invalid activity")?;
            let reflected = Player::new(host, person)?.reflect_on_memory(
                game_economy::core_interval::EpisodeCue::CreativePractice { activity },
            )?;
            println!("memory_reinforced={reflected}");
        }
        ["findings"] => {
            let player = Player::new(host, person)?;
            let comparisons = player.mechanical_comparisons()?;
            if comparisons.is_empty() {
                println!("No retained trial comparisons.");
            }
            for row in comparisons {
                let status = if row.contradicted() { "contradicted" } else { "provisional" };
                println!("apparatus={} output={} input_force={} input_distance={} largest_load_moved={} smallest_load_stalled={} distinct_moved_loads={} distinct_stalled_loads={} status={status}",
                    row.conditions.apparatus, row.conditions.output_part, row.conditions.input_force,
                    row.conditions.input_distance, row.largest_load_moved, row.smallest_load_stalled,
                    row.distinct_moved_loads, row.distinct_stalled_loads);
            }
        }
        ["amend", field] => {
            let field = number(field)?;
            let amended = Player::new(host, person)?.amend(field)?;
            println!("field={field} amended={amended}");
        }
        ["water", field, quantity] => {
            let field = number(field)?;
            let attempt = Player::new(host, person)?.water(field, number(quantity)?)?;
            let added = attempt
                .filter(|row| row.accepted)
                .map_or(0, |row| row.quantity);
            println!("field={field} water_added={added}");
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
        ["writing-surfaces"] => {
            let (selected, surfaces) = Player::new(host, person)?.writing_surfaces()?;
            println!("writing_surface={selected} surfaces={surfaces:?}");
        }
        ["writing-surface", surface] => {
            let surface = (*surface != "default").then(|| number(surface)).transpose()?;
            Player::new(host, person)?.choose_writing_surface(surface)?;
        }
        ["inscribe", sounds] => {
            if let Some(item) = Player::new(host, person)?.inscribe(numbers(sounds)?)? {
                println!("inscribed_page={item}");
            } else {
                println!("inscribed=false");
            }
        }
        ["compose", words @ ..] => {
            let words = words.iter().map(|sounds| numbers(sounds)).collect::<Result<_, _>>()?;
            if let Some(item) = Player::new(host, person)?.compose(words)? {
                println!("written_work={item}");
            } else {
                println!("writing materials or carrying space unavailable");
            }
        }
        ["read-work", item] => {
            let work = Player::new(host, person)?.observe_written_work(number(item)?)?;
            if let Some(surface) = work.surface {
                println!("written_surface={surface}");
            }
            for (index, form) in work.forms.iter().enumerate() {
                println!("form={} marks={:?} meanings={:?}", index + 1, form.marks, form.meanings);
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
