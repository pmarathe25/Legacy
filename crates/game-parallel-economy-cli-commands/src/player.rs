//! Commands restricted to the current ruler's perspective.
use game_parallel_economy::Economy;
use game_parallel_economy_combat::{Order, Point};
use game_parallel_economy_player::Player;

pub fn handle(world: &mut Economy, words: &[&str]) -> Result<bool, String> {
    let mut player = Player::new(world);
    match words {
        [] => {}
        ["quit"] | ["exit"] => return Ok(false),
        ["help"] => println!(
            "view | catalog | guardians | past-knowledge LIFE | meet PERSON | ask-work PERSON RECIPE LIFETIME | respond-work REQUEST accept|decline | goal-help RECIPE on|off | practice-decide AUTHOR SEQUENCE accept|decline | work-practice-propose WORKER RECIPE GOOD QUANTITY LIFETIME | work-practice-choice FIELD on|off | work-practice-reserve AMOUNT | work-practice-evaluation off|BASE AFFINITY TRUST THRESHOLD | work-outcome off|WAIT TRUST AFFINITY | work-practice-decide AUTHOR SEQUENCE accept|decline | practice-withdraw AUTHOR SEQUENCE | practice-report PERSON SEQUENCE ready|unready | practice-belief PERSON SEQUENCE believe|doubt|unassessed | practice-require-report on|off | practice-unanswered retain|believe|doubt|unassessed | practice-value off|VALUE MIN MAX RECEIVED GAVE UNANSWERED | practice-precedent off|PERSON GOOD DIST RECEIVED GAVE UNANSWERED SHORTAGE | food-misreport off|DELTA [TARGET] | food-report-reception default|all|none|MIN | promise-trust default|FULFILLED UNFULFILLED | practice-willing on|off | practice-ask on|off | practice-invent on|off | practice-approve on|off | ask-food PERSON LIFETIME | ask-haul PERSON SOURCE DEST GOOD LIFETIME | attack PERSON | combat-order attack PERSON|defend|retreat | combat-move X Y | eat GOOD | treat | treat-other PERSON MEDICINE QUANTITY | feed-animal ANIMAL GOOD QUANTITY | hunt-animal ANIMAL | rest | farm FIELD | build-site BLUEPRINT | destroy-site SITE | salvage-site SITE GOOD QUANTITY | take-store STORE GOOD QUANTITY | put-store STORE GOOD QUANTITY | claim GOOD | withdraw-claim REGION GOOD | claim-site SITE | withdraw-site-claim SITE | claim-built-sites on|off | gather SITE | give PERSON GOOD QUANTITY | take GOOD QUANTITY | store GOOD QUANTITY | escape | guard SITE | capture-yield on|off | confine PERSON SITE | release-held PERSON | recover PERSON GOOD QUANTITY | promise PERSON GOOD QUANTITY DUE | decide-promise ID accept|decline | fulfill-promise ID | start-work RECIPE UNITS | continue-work | go REGION | expedition REGION VISIT | invite-expedition ID PERSON | meet-expedition ID PERSON | cancel-expedition ID | invoke LIFE... | dismiss-invocation | accept-warning REVISION | guardian-release ID | step [TICKS] | save PATH | load PATH | quit"
        ),
        ["view"] => {
            let view = player.view()?;
            println!(
                "tick={} person={:?} held={} nearby={:?} possessions={:?} job={:?}",
                view.tick, view.person, view.held, view.nearby, view.possessions, view.job
            );
            if let Some(journey) = view.journey {
                println!("journey={journey:?}");
            }
            for record in view.food_reports {
                println!("report={record:?}");
            }
            println!("local_stock={:?}", view.local_stock);
            for body in view.bodies {
                println!("body={body:?}");
            }
            for animal in view.animals {
                println!("animal={animal:?}");
            }
            println!(
                "lives={:?} active={:?} burden={} exposure={}",
                view.past_lives,
                view.active_lives,
                view.invocation_burden,
                view.invocation_exposure
            );
            for mission in view.expeditions {
                println!("expedition={mission:?}");
            }
            for promise in view.promises {
                println!("promise={promise:?}");
            }
            for goal in view.goals {
                println!("goal={goal:?}");
            }
            if let Some(battle) = view.battle {
                println!("battle={battle:?}");
            }
            if let Some(treatment) = view.treatment {
                println!("treatment={treatment:?}");
            }
            if let Some(fatigue) = view.fatigue {
                println!("fatigue={fatigue:?}");
            }
            if let Some(conditions) = view.conditions {
                println!("conditions={conditions:?}");
            }
            if let Some(isolation) = view.isolation {
                println!("isolation={isolation}");
            }
            if let Some(deprivation) = view.purpose_deprivation {
                println!("purpose-deprivation={deprivation}");
            }
            for field in view.fields {
                println!("field={field:?}");
            }
            for site in view.resource_sites {
                println!("resource-site={site:?}");
            }
            for site in view.work_sites {
                println!("work-site={site:?}");
            }
            for debris in view.debris {
                println!("debris={debris:?}");
            }
            for store in view.local_stores {
                println!("local-store={store:?}");
            }
            for claim in view.claims {
                println!("claim={claim:?}");
            }
            for site in view.site_claims {
                println!("site-claim={site}");
            }
            if let Some(enabled) = view.claim_built_sites {
                println!("claim-built-sites={enabled}");
            }
            for observation in view.site_observations {
                println!("site-observation={observation:?}");
            }
            for request in view.pending_work_requests {
                println!("work-request={request:?}");
            }
            for relationship in view.relationships {
                println!("relationship={relationship:?}");
            }
            if let Some(learning) = view.sharing_learning {
                println!("sharing-experience={learning:?}");
            }
            if let Some(practices) = view.practices {
                println!("personal-practices={practices:?}");
            }
            for request in view.food_requests {
                println!("food-request={request:?}");
            }
            for request in view.hauling_requests {
                println!("hauling-request={request:?}");
            }
            for work in view.observed_work {
                println!("observed-work={work:?}");
            }
            for memory in view.work_memories {
                println!("work-memory={memory:?}");
            }
            if let Some(warning) = view.warning {
                println!("warning={warning:?}");
            }
        }
        ["guardians"] => {
            for guardian in player.guardians()? {
                println!("{guardian:?}");
            }
        }
        ["catalog"] => {
            let (goods, recipes) = player.catalog()?;
            for good in goods {
                println!("good={} name={}", good.id, good.name);
            }
            for recipe in recipes {
                println!(
                    "recipe={} name={} ticks={} inputs={:?} outputs={:?}",
                    recipe.id, recipe.name, recipe.ticks, recipe.inputs, recipe.outputs
                );
            }
            let (tools, sites) = player.work_requirements()?;
            for tool in tools {
                println!("work-tool={tool:?}");
            }
            for site in sites {
                println!("work-site-requirement={site:?}");
            }
            for blueprint in player.building_blueprints()? {
                println!("building-blueprint={blueprint:?}");
            }
        }
        ["past-knowledge", life] => {
            for record in player.past_knowledge(life.parse().map_err(|_| "invalid past life")?)? {
                println!("{record:?}");
            }
        }
        ["build-site", blueprint] => println!(
            "built={}",
            player.build_site(blueprint.parse().map_err(|_| "invalid blueprint")?)?
        ),
        ["destroy-site", site] => println!(
            "destroyed={}",
            player.destroy_site(site.parse().map_err(|_| "invalid work site")?)?
        ),
        ["salvage-site", site, good, quantity] => println!(
            "recovered={}",
            player.salvage_site(
                site.parse().map_err(|_| "invalid site")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?
            )?
        ),
        [action @ ("take-store" | "put-store"), store, good, quantity] => println!(
            "moved={}",
            player.move_local_store(
                store.parse().map_err(|_| "invalid store")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?,
                *action == "put-store"
            )?
        ),
        ["claim", good] => player.claim_stock(good.parse().map_err(|_| "invalid good")?)?,
        ["claim-site", site] => player.claim_site(site.parse().map_err(|_| "invalid site")?)?,
        ["withdraw-site-claim", site] => {
            player.withdraw_site_claim(site.parse().map_err(|_| "invalid site")?)?
        }
        ["claim-built-sites", choice] => player.set_built_site_claim_choice(match *choice {
            "on" => true,
            "off" => false,
            _ => return Err("expected on or off".into()),
        })?,
        ["withdraw-claim", region, good] => player.withdraw_claim(
            region.parse().map_err(|_| "invalid region")?,
            good.parse().map_err(|_| "invalid good")?,
        )?,
        ["meet", target] => println!(
            "accepted={}",
            player.meet(target.parse().map_err(|_| "invalid target")?)?
        ),
        ["goal-help", recipe, choice] => player.set_goal_help_recognition(
            recipe.parse().map_err(|_| "invalid recipe")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        [
            action @ ("practice-willing" | "practice-ask" | "practice-invent" | "practice-approve"),
            choice,
        ] => {
            let enabled = match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            };
            match *action {
                "practice-willing" => player.set_practice_willingness(enabled)?,
                "practice-ask" => player.set_practice_asking(enabled)?,
                "practice-invent" => player.set_practice_invention(enabled)?,
                _ => player.set_practice_approval(enabled)?,
            }
        }
        ["practice-decide", author, sequence, choice] => player.decide_practice(
            author.parse().map_err(|_| "invalid author")?,
            sequence.parse().map_err(|_| "invalid proposal sequence")?,
            match *choice {
                "accept" => true,
                "decline" => false,
                _ => return Err("expected accept or decline".into()),
            },
        )?,
        ["work-practice-choice", field, choice] => player.set_work_practice_choice(
            field,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["work-practice-reserve", reserve] => {
            player.set_work_practice_reserve(reserve.parse().map_err(|_| "invalid reserve")?)?
        }
        ["work-practice-evaluation", "off"] => player.set_work_practice_evaluation(None)?,
        ["work-outcome", "off"] => player.set_work_outcome_policy(None)?,
        ["work-outcome", wait, trust, affinity] => {
            player.set_work_outcome_policy(Some(game_parallel_economy::WorkOutcomePolicy {
                wait_ticks: wait.parse().map_err(|_| "invalid wait")?,
                missing_trust: trust.parse().map_err(|_| "invalid trust change")?,
                missing_affinity: affinity.parse().map_err(|_| "invalid affinity change")?,
            }))?
        }
        ["work-practice-evaluation", base, affinity, trust, threshold] => player
            .set_work_practice_evaluation(Some(game_parallel_economy::WorkPracticeEvaluation {
                base: base.parse().map_err(|_| "invalid base")?,
                affinity_weight: affinity.parse().map_err(|_| "invalid affinity weight")?,
                trust_weight: trust.parse().map_err(|_| "invalid trust weight")?,
                threshold: threshold.parse().map_err(|_| "invalid threshold")?,
            }))?,
        ["work-practice-decide", author, sequence, choice] => player.decide_work_practice(
            author.parse().map_err(|_| "invalid author")?,
            sequence.parse().map_err(|_| "invalid proposal sequence")?,
            match *choice {
                "accept" => true,
                "decline" => false,
                _ => return Err("expected accept or decline".into()),
            },
        )?,
        [
            "work-practice-propose",
            worker,
            recipe,
            good,
            quantity,
            lifetime,
        ] => println!(
            "delivered={}",
            player.propose_work_practice(
                worker.parse().map_err(|_| "invalid worker")?,
                recipe.parse().map_err(|_| "invalid recipe")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?,
                lifetime.parse().map_err(|_| "invalid lifetime")?,
            )?
        ),
        ["practice-withdraw", author, sequence] => player.withdraw_practice(
            author.parse().map_err(|_| "invalid author")?,
            sequence.parse().map_err(|_| "invalid proposal sequence")?,
        )?,
        ["practice-report", listener, sequence, claim] => println!(
            "communicated={}",
            player.report_practice(
                listener.parse().map_err(|_| "invalid listener")?,
                sequence.parse().map_err(|_| "invalid proposal sequence")?,
                match *claim {
                    "ready" => true,
                    "unready" => false,
                    _ => return Err("expected ready or unready".into()),
                },
            )?
        ),
        ["practice-belief", speaker, sequence, belief] => player.assess_practice_report(
            speaker.parse().map_err(|_| "invalid speaker")?,
            sequence.parse().map_err(|_| "invalid proposal sequence")?,
            match *belief {
                "believe" => game_parallel_economy::PracticeBelief::Believes,
                "doubt" => game_parallel_economy::PracticeBelief::Disbelieves,
                "unassessed" => game_parallel_economy::PracticeBelief::Unassessed,
                _ => return Err("expected believe, doubt, or unassessed".into()),
            },
        )?,
        ["practice-require-report", choice] => {
            player.set_practice_require_report(match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            })?
        }
        ["practice-unanswered", choice] => {
            player.set_practice_unanswered_belief(match *choice {
                "retain" => None,
                "believe" => Some(game_parallel_economy::PracticeBelief::Believes),
                "doubt" => Some(game_parallel_economy::PracticeBelief::Disbelieves),
                "unassessed" => Some(game_parallel_economy::PracticeBelief::Unassessed),
                _ => return Err("expected retain, believe, doubt, or unassessed".into()),
            })?
        }
        ["practice-value", "off"] => player.set_practice_value_learning(None)?,
        [
            "practice-value",
            value,
            minimum,
            maximum,
            received,
            gave,
            unanswered,
        ] => player.set_practice_value_learning(Some(
            game_parallel_economy::PracticeValueLearning {
                value: value.parse().map_err(|_| "invalid value")?,
                minimum: minimum.parse().map_err(|_| "invalid minimum")?,
                maximum: maximum.parse().map_err(|_| "invalid maximum")?,
                received: received.parse().map_err(|_| "invalid received change")?,
                gave: gave.parse().map_err(|_| "invalid giving change")?,
                unanswered: unanswered
                    .parse()
                    .map_err(|_| "invalid unanswered change")?,
            },
        ))?,
        ["practice-precedent", "off"] => player.set_practice_precedent(None)?,
        [
            "practice-precedent",
            person,
            good,
            distance,
            received,
            gave,
            unanswered,
            shortage,
        ] => player.set_practice_precedent(Some(game_parallel_economy::PracticePrecedent {
            same_person: person.parse().map_err(|_| "invalid person weight")?,
            same_good: good.parse().map_err(|_| "invalid good weight")?,
            quantity_difference: distance.parse().map_err(|_| "invalid quantity weight")?,
            received: received
                .parse()
                .map_err(|_| "invalid received interpretation")?,
            gave: gave.parse().map_err(|_| "invalid giving interpretation")?,
            unanswered: unanswered
                .parse()
                .map_err(|_| "invalid unanswered interpretation")?,
            shortage: shortage
                .parse()
                .map_err(|_| "invalid shortage interpretation")?,
        }))?,
        ["food-misreport"] => println!("{:?}", player.food_misreport()?),
        ["food-misreport", "off"] => player.set_food_misreport(None)?,
        ["food-misreport", delta] => {
            player.set_food_misreport(Some(game_parallel_economy::Misreport {
                target: None,
                food_delta: delta.parse().map_err(|_| "invalid food offset")?,
            }))?
        }
        ["food-misreport", delta, target] => {
            player.set_food_misreport(Some(game_parallel_economy::Misreport {
                target: Some(target.parse().map_err(|_| "invalid target")?),
                food_delta: delta.parse().map_err(|_| "invalid food offset")?,
            }))?
        }
        ["food-report-reception"] => println!("{:?}", player.food_report_reception()?),
        ["food-report-reception", choice] => player.set_food_report_reception(match *choice {
            "default" => None,
            "all" => Some(game_parallel_economy::ReportReception::Always),
            "none" => Some(game_parallel_economy::ReportReception::Never),
            value => Some(game_parallel_economy::ReportReception::TrustAtLeast(
                value.parse().map_err(|_| "invalid trust threshold")?,
            )),
        })?,
        ["promise-trust"] => println!("{:?}", player.promise_trust_reactions()?),
        ["promise-trust", "default"] => player.set_promise_trust_reactions(None)?,
        ["promise-trust", fulfilled, unfulfilled] => player.set_promise_trust_reactions(Some((
            fulfilled
                .parse()
                .map_err(|_| "invalid fulfilled reaction")?,
            unfulfilled
                .parse()
                .map_err(|_| "invalid unfulfilled reaction")?,
        )))?,
        ["ask-work", worker, recipe, lifetime] => println!(
            "request={}",
            player.ask_work(
                worker.parse().map_err(|_| "invalid worker")?,
                recipe.parse().map_err(|_| "invalid recipe")?,
                lifetime.parse().map_err(|_| "invalid lifetime")?
            )?
        ),
        ["respond-work", request, choice] => println!(
            "started={}",
            player.respond_work_request(
                request.parse().map_err(|_| "invalid request")?,
                match *choice {
                    "accept" => true,
                    "decline" => false,
                    _ => return Err("expected accept or decline".into()),
                },
            )?
        ),
        ["ask-food", target, lifetime] => player.ask_food(
            target.parse().map_err(|_| "invalid target")?,
            lifetime.parse().map_err(|_| "invalid lifetime")?,
        )?,
        ["ask-haul", carrier, source, destination, good, lifetime] => player.ask_haul(
            carrier.parse().map_err(|_| "invalid carrier")?,
            source.parse().map_err(|_| "invalid source")?,
            destination.parse().map_err(|_| "invalid destination")?,
            good.parse().map_err(|_| "invalid good")?,
            lifetime.parse().map_err(|_| "invalid lifetime")?,
        )?,
        ["attack", target] => println!(
            "encounter={}",
            player.attack(target.parse().map_err(|_| "invalid target")?)?
        ),
        ["combat-order", "attack", target] => player.combat_order(Order::Attack {
            target: target.parse().map_err(|_| "invalid target")?,
        })?,
        ["combat-order", "defend"] => player.combat_order(Order::Defend)?,
        ["combat-order", "retreat"] => player.combat_order(Order::Retreat)?,
        ["combat-move", x, y] => player.combat_move(Point {
            x: x.parse().map_err(|_| "invalid x")?,
            y: y.parse().map_err(|_| "invalid y")?,
        })?,
        ["start-work", recipe, units] => println!(
            "started={}",
            player.start_work(
                recipe.parse().map_err(|_| "invalid recipe")?,
                units.parse().map_err(|_| "invalid quantity")?
            )?
        ),
        ["continue-work"] => println!("finished={}", player.continue_work()?),
        ["feed-animal", animal, good, quantity] => println!(
            "food-used={}",
            player.feed_animal(
                animal.parse().map_err(|_| "invalid animal")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?,
            )?
        ),
        ["hunt-animal", animal] => println!(
            "hunted={}",
            player.hunt_animal(animal.parse().map_err(|_| "invalid animal")?)?
        ),
        ["promise", target, good, quantity, due] => println!(
            "promise={}",
            player.promise(
                target.parse().map_err(|_| "invalid target")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?,
                due.parse().map_err(|_| "invalid due tick")?
            )?
        ),
        ["decide-promise", id, decision] => player.decide_promise(
            id.parse().map_err(|_| "invalid promise")?,
            match *decision {
                "accept" => true,
                "decline" => false,
                _ => return Err("expected accept or decline".into()),
            },
        )?,
        ["fulfill-promise", id] => println!(
            "fulfilled={}",
            player.fulfill_promise(id.parse().map_err(|_| "invalid promise")?)?
        ),
        ["eat", good] => println!(
            "eaten={}",
            player.eat(good.parse().map_err(|_| "invalid good")?)?
        ),
        ["treat"] => println!("medicine_consumed={}", player.treat()?),
        ["rest"] => player.rest()?,
        ["escape"] => println!("escaped={}", player.escape_confinement()?),
        ["guard", site] => println!(
            "guarded={}",
            player.guard_cell(site.parse().map_err(|_| "invalid site")?)?
        ),
        ["capture-yield", choice] => player.set_capture_yield(match *choice {
            "on" => true,
            "off" => false,
            _ => return Err("expected on or off".into()),
        })?,
        ["confine", target, site] => println!(
            "confined={}",
            player.confine_person(
                target.parse().map_err(|_| "invalid person")?,
                site.parse().map_err(|_| "invalid site")?,
            )?
        ),
        ["release-held", target] => println!(
            "released={}",
            player.release_held_person(target.parse().map_err(|_| "invalid person")?,)?
        ),
        ["farm", field] => println!(
            "cultivated={}",
            player.farm(field.parse().map_err(|_| "invalid field")?)?
        ),
        ["take", good, quantity] | ["store", good, quantity] => println!(
            "transferred={}",
            player.transfer_stock(
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?,
                words[0] == "take"
            )?
        ),
        ["recover", deceased, good, quantity] => println!(
            "recovered={}",
            player.recover_possessions(
                deceased.parse().map_err(|_| "invalid person")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?
            )?
        ),
        ["gather", node] => println!(
            "gathered={}",
            player.gather(node.parse().map_err(|_| "invalid resource site")?)?
        ),
        ["give", recipient, good, quantity] => println!(
            "given={}",
            player.give(
                recipient.parse().map_err(|_| "invalid recipient")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?
            )?
        ),
        ["treat-other", patient, medicine, quantity] => println!(
            "medicine_consumed={}",
            player.treat_other(
                patient.parse().map_err(|_| "invalid patient")?,
                medicine.parse().map_err(|_| "invalid medicine")?,
                quantity.parse().map_err(|_| "invalid quantity")?
            )?
        ),
        ["go", region] => player.depart(region.parse().map_err(|_| "invalid region")?)?,
        ["expedition", region, visit] => println!(
            "expedition={}",
            player.start_expedition(
                region.parse().map_err(|_| "invalid region")?,
                visit.parse().map_err(|_| "invalid visit")?
            )?
        ),
        ["cancel-expedition", id] => {
            player.cancel_expedition(id.parse().map_err(|_| "invalid expedition")?)?
        }
        ["meet-expedition", id, target] => player.set_expedition_meeting(
            id.parse().map_err(|_| "invalid expedition")?,
            target.parse().map_err(|_| "invalid target")?,
        )?,
        ["invite-expedition", id, target] => println!(
            "joined={:?}",
            player.invite_expedition(
                id.parse().map_err(|_| "invalid expedition")?,
                target.parse().map_err(|_| "invalid target")?
            )?
        ),
        ["invoke", lives @ ..] if !lives.is_empty() => {
            let lives = lives
                .iter()
                .map(|life| life.parse().map_err(|_| "invalid past life".to_owned()))
                .collect::<Result<Vec<_>, _>>()?;
            player.invoke(&lives)?;
        }
        ["dismiss-invocation"] => player.dismiss_invocation()?,
        ["accept-warning", revision] => {
            player.accept_warning(revision.parse().map_err(|_| "invalid revision")?)?
        }
        ["guardian-release", id] => println!(
            "{:?}",
            player.release_guardian(id.parse().map_err(|_| "invalid guardian")?)?
        ),
        ["step"] => player.advance()?,
        ["step", count] => {
            for _ in 0..count.parse::<u64>().map_err(|_| "invalid tick count")? {
                player.advance()?;
            }
        }
        _ => return Err("unknown command or invalid arguments; enter help".into()),
    }
    Ok(true)
}
