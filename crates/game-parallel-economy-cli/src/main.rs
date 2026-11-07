//! Small interactive entry point for the composed economic simulation.
use game_parallel_economy::{Departure, Economy, PromiseDecision, PromiseProposal};
use game_parallel_lots::Place;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

fn command(world: &mut Economy, line: &str) -> Result<bool, String> {
    let words: Vec<_> = line.split_whitespace().collect();
    if game_parallel_economy_cli_commands::handle(world, &words)? {
        return Ok(true);
    }
    match words.as_slice() {
        [] => {}
        ["help"] => println!(
            "status | people | knowledge PERSON | past-knowledge LIFE | meet PERSON TARGET | eat PERSON GOOD | start-work PERSON RECIPE UNITS | continue-work PERSON | ruler | invoke LIFE... | dismiss-invocation | accept-warning REVISION | guardian-release ID | health | handle-animal PERSON ANIMAL | capture-animal PERSON ANIMAL | release-animal PERSON ANIMAL | defend-animal PERSON ANIMAL | animal-handle PERSON on|off | animal-capture PERSON on|off | animal-release-choice PERSON on|off | animal-defense PERSON on|off | skills PERSON | catalog | fields | claims | claim PERSON REGION GOOD | unclaim PERSON REGION GOOD | take-claimed PERSON on|off | claim-output PERSON on|off | goods PERSON | stock REGION | work PERSON on|off | haul PERSON on|off | carry-local PERSON on|off | capture-choice PERSON on|off | guard-choice PERSON on|off | guard-opening PERSON on|off | release-choice PERSON on|off | fetch-goal PERSON on|off | goal-help PERSON RECIPE on|off | work-offer PERSON on|off | practices PERSON | practice-choice PERSON FIELD on|off | work-practice-propose AUTHOR WORKER RECIPE GOOD QUANTITY LIFETIME | work-practice-choice PERSON FIELD on|off | work-practice-reserve PERSON AMOUNT | work-practice-evaluation PERSON off|BASE AFFINITY TRUST THRESHOLD | work-outcome PERSON off|WAIT MISSING_TRUST MISSING_AFFINITY [RECEIVED_TRUST RECEIVED_AFFINITY PARTIAL_TRUST|default PARTIAL_AFFINITY|default] | work-practice-decide PERSON AUTHOR SEQUENCE accept|decline | practice-decide PERSON AUTHOR SEQUENCE accept|decline | practice-withdraw PERSON AUTHOR SEQUENCE | practice-report SPEAKER LISTENER SEQUENCE ready|unready | practice-belief PERSON SPEAKER SEQUENCE believe|doubt|unassessed | practice-reporting PERSON MODE | practice-attitude PERSON ignore|trust|doubt|min THRESHOLD | practice-unanswered PERSON MODE | practice-value PERSON off|VALUE MIN MAX RECEIVED GAVE UNANSWERED | practice-precedent PERSON off|PERSON GOOD DIST RECEIVED GAVE UNANSWERED SHORTAGE | food-misreport PERSON off|DELTA [TARGET] | food-report-reception PERSON default|all|none|MIN | work-value PERSON GOOD VALUE MAX | farm PERSON on|off | social PERSON on|off | yield PERSON on|off | treat PERSON on|off | go PERSON REGION | expeditions | expedition PERSON REGION VISIT | meet-expedition PERSON ID TARGET | invite-expedition PERSON ID TARGET | cancel-expedition PERSON ID | promises | promise DEBTOR CREDITOR GOOD QUANTITY DUE | decide ID PERSON accept|decline | honor PERSON on|off | step [TICKS] | save PATH | load PATH | quit"
        ),
        ["quit"] | ["exit"] => return Ok(false),
        ["status"] => {
            let state = world.snapshot();
            println!(
                "tick={} people={} living={} active_jobs={}",
                world.tick(),
                state.people.len(),
                state.people.iter().filter(|p| p.health > 0).count(),
                state.physical.work.len()
            );
        }
        ["people"] => {
            for person in world.snapshot().people {
                println!(
                    "person={} region={:?} health={} hunger={} willing_to_work={}",
                    person.id, person.region, person.health, person.hunger, person.willing_to_work
                );
            }
        }
        ["catalog"] => {
            let physical = world.snapshot().physical;
            for good in physical.goods {
                println!("good={} name={}", good.id, good.name);
            }
            for recipe in physical.recipes {
                println!(
                    "recipe={} name={} ticks={} inputs={:?} outputs={:?}",
                    recipe.id, recipe.name, recipe.ticks, recipe.inputs, recipe.outputs
                );
            }
        }
        ["fields"] => {
            let state = world.snapshot();
            for field in &state
                .agriculture
                .as_ref()
                .ok_or("cultivation is disabled")?
                .fields
            {
                println!(
                    "field={} region={} crop={} phase={:?} remaining={}",
                    field.id, field.region, field.crop, field.phase, field.remaining
                );
            }
        }
        ["promises"] => {
            let state = world.snapshot();
            for promise in &state
                .agreements
                .as_ref()
                .ok_or("promises are disabled")?
                .contracts
            {
                println!(
                    "id={} debtor={} creditor={} good={} quantity={} delivered={} due={} status={:?}",
                    promise.id,
                    promise.debtor,
                    promise.creditor,
                    promise.good,
                    promise.quantity,
                    promise.delivered,
                    promise.due,
                    promise.status
                );
            }
        }
        ["promise", debtor, creditor, good, quantity, due] => {
            let ids = world.propose_promises(&[PromiseProposal {
                debtor: debtor.parse().map_err(|_| "invalid debtor")?,
                creditor: creditor.parse().map_err(|_| "invalid creditor")?,
                good: good.parse().map_err(|_| "invalid good")?,
                quantity: quantity.parse().map_err(|_| "invalid quantity")?,
                due: due.parse().map_err(|_| "invalid due tick")?,
            }])?;
            println!("promise={}", ids[0]);
        }
        ["decide", id, person, choice] => world.decide_promises(&[PromiseDecision {
            contract: id.parse().map_err(|_| "invalid promise")?,
            actor: person.parse().map_err(|_| "invalid person")?,
            accept: match *choice {
                "accept" => true,
                "decline" => false,
                _ => return Err("expected accept or decline".into()),
            },
        }])?,
        ["honor", person, choice] => world.set_promise_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["goods", person] => {
            let person = person.parse().map_err(|_| "invalid person")?;
            let state = world.snapshot();
            let container = state
                .physical
                .custody
                .containers
                .iter()
                .find(|row| row.place == Place::Person(person))
                .ok_or("unknown person")?;
            for lot in &container.lots {
                println!(
                    "good={} quantity={} expires_at={:?}",
                    lot.good, lot.quantity, lot.expires_at
                );
            }
        }
        ["stock", region] => {
            let region: u64 = region.parse().map_err(|_| "invalid region")?;
            let state = world.snapshot();
            for container in
                state.physical.custody.containers.iter().filter(
                    |row| matches!(row.place, Place::Stock { region: id, .. } if id == region),
                )
            {
                for lot in &container.lots {
                    let name = state
                        .physical
                        .goods
                        .iter()
                        .find(|good| good.id == lot.good)
                        .map_or("?", |good| good.name.as_str());
                    println!(
                        "good={} name={} quantity={} expires_at={:?}",
                        lot.good, name, lot.quantity, lot.expires_at
                    );
                }
            }
        }
        ["work", person, choice] => {
            let person = person.parse().map_err(|_| "invalid person")?;
            let willing = match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            };
            world.set_work_choice(person, willing)?;
        }
        ["carry-local", person, choice] => world.set_local_carrying_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["capture-choice", person, choice] => world.set_capture_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["guard-choice", person, choice] => world.set_guard_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["animal-handle", person, choice] => world.set_animal_handling_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["animal-capture", person, choice] => world.set_animal_capture_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["animal-release-choice", person, choice] => world.set_animal_release_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["animal-defense", person, choice] => world.set_animal_defense_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["guard-opening", person, choice] => world.set_guard_opening_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["release-choice", person, choice] => world.set_release_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["fetch-goal", person, choice] => world.set_goal_fetch_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["goal-help", person, recipe, choice] => world.set_goal_help_recognition(
            person.parse().map_err(|_| "invalid person")?,
            recipe.parse().map_err(|_| "invalid recipe")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["work-offer", person, choice] => world.set_work_promise_offer_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["practices", person] => println!(
            "{:?}",
            world.personal_practice(person.parse().map_err(|_| "invalid person")?)
        ),
        ["practice-choice", person, field, choice] => {
            let person = person.parse().map_err(|_| "invalid person")?;
            let enabled = match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            };
            match *field {
                "invent" => world.set_practice_invention(person, enabled)?,
                "approve" => world.set_practice_approval(person, enabled)?,
                "willing" => world.set_practice_willingness(person, enabled)?,
                "ask" => world.set_practice_asking(person, enabled)?,
                "require-report" => world.set_practice_require_report(person, enabled)?,
                _ => return Err("expected invent, approve, willing, ask, or require-report".into()),
            }
        }
        ["work-practice-choice", person, field, choice] => world.set_work_practice_choice(
            person.parse().map_err(|_| "invalid person")?,
            field,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["work-practice-reserve", person, reserve] => world.set_work_practice_reserve(
            person.parse().map_err(|_| "invalid person")?,
            reserve.parse().map_err(|_| "invalid reserve")?,
        )?,
        ["work-practice-evaluation", person, "off"] => world
            .set_work_practice_evaluation(person.parse().map_err(|_| "invalid person")?, None)?,
        ["work-outcome", person, "off"] => {
            world.set_work_outcome_policy(person.parse().map_err(|_| "invalid person")?, None)?
        }
        ["work-outcome", person, wait, trust, affinity] => world.set_work_outcome_policy(
            person.parse().map_err(|_| "invalid person")?,
            Some(game_parallel_economy::WorkOutcomePolicy {
                wait_ticks: wait.parse().map_err(|_| "invalid wait")?,
                missing_trust: trust.parse().map_err(|_| "invalid trust change")?,
                missing_affinity: affinity.parse().map_err(|_| "invalid affinity change")?,
                ..Default::default()
            }),
        )?,
        [
            "work-outcome",
            person,
            wait,
            missing_trust,
            missing_affinity,
            received_trust,
            received_affinity,
            partial_trust,
            partial_affinity,
        ] => {
            let partial = |value: &str| -> Result<Option<i32>, String> {
                if value == "default" {
                    Ok(None)
                } else {
                    value
                        .parse()
                        .map(Some)
                        .map_err(|_| "invalid partial reaction".into())
                }
            };
            world.set_work_outcome_policy(
                person.parse().map_err(|_| "invalid person")?,
                Some(game_parallel_economy::WorkOutcomePolicy {
                    wait_ticks: wait.parse().map_err(|_| "invalid wait")?,
                    missing_trust: missing_trust.parse().map_err(|_| "invalid trust change")?,
                    missing_affinity: missing_affinity
                        .parse()
                        .map_err(|_| "invalid affinity change")?,
                    received_trust: received_trust.parse().map_err(|_| "invalid trust change")?,
                    received_affinity: received_affinity
                        .parse()
                        .map_err(|_| "invalid affinity change")?,
                    partial_trust: partial(partial_trust)?,
                    partial_affinity: partial(partial_affinity)?,
                }),
            )?;
        }
        [
            "work-practice-evaluation",
            person,
            base,
            affinity,
            trust,
            threshold,
        ] => world.set_work_practice_evaluation(
            person.parse().map_err(|_| "invalid person")?,
            Some(game_parallel_economy::WorkPracticeEvaluation {
                base: base.parse().map_err(|_| "invalid base")?,
                affinity_weight: affinity.parse().map_err(|_| "invalid affinity weight")?,
                trust_weight: trust.parse().map_err(|_| "invalid trust weight")?,
                threshold: threshold.parse().map_err(|_| "invalid threshold")?,
            }),
        )?,
        ["work-practice-decide", person, author, sequence, choice] => world.decide_work_practice(
            person.parse().map_err(|_| "invalid person")?,
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
            author,
            worker,
            recipe,
            good,
            quantity,
            lifetime,
        ] => println!(
            "delivered={}",
            world.propose_work_practice(
                author.parse().map_err(|_| "invalid author")?,
                worker.parse().map_err(|_| "invalid worker")?,
                recipe.parse().map_err(|_| "invalid recipe")?,
                good.parse().map_err(|_| "invalid good")?,
                quantity.parse().map_err(|_| "invalid quantity")?,
                lifetime.parse().map_err(|_| "invalid lifetime")?,
            )?
        ),
        ["practice-decide", person, author, sequence, choice] => world.decide_practice(
            person.parse().map_err(|_| "invalid person")?,
            author.parse().map_err(|_| "invalid author")?,
            sequence.parse().map_err(|_| "invalid proposal sequence")?,
            match *choice {
                "accept" => true,
                "decline" => false,
                _ => return Err("expected accept or decline".into()),
            },
        )?,
        ["practice-withdraw", person, author, sequence] => world.withdraw_practice(
            person.parse().map_err(|_| "invalid person")?,
            author.parse().map_err(|_| "invalid author")?,
            sequence.parse().map_err(|_| "invalid proposal sequence")?,
        )?,
        ["practice-report", speaker, listener, sequence, claim] => println!(
            "communicated={}",
            world.report_practice(
                speaker.parse().map_err(|_| "invalid speaker")?,
                listener.parse().map_err(|_| "invalid listener")?,
                sequence.parse().map_err(|_| "invalid sequence")?,
                match *claim {
                    "ready" => true,
                    "unready" => false,
                    _ => return Err("expected ready or unready".into()),
                },
            )?
        ),
        ["practice-belief", person, speaker, sequence, belief] => world.assess_practice_report(
            person.parse().map_err(|_| "invalid person")?,
            speaker.parse().map_err(|_| "invalid speaker")?,
            sequence.parse().map_err(|_| "invalid sequence")?,
            match *belief {
                "believe" => game_parallel_economy::PracticeBelief::Believes,
                "doubt" => game_parallel_economy::PracticeBelief::Disbelieves,
                "unassessed" => game_parallel_economy::PracticeBelief::Unassessed,
                _ => return Err("expected believe, doubt, or unassessed".into()),
            },
        )?,
        ["practice-reporting", person, reporting] => world.set_practice_reporting(
            person.parse().map_err(|_| "invalid person")?,
            match *reporting {
                "silent" => game_parallel_economy::PracticeReporting::Silent,
                "truthful" => game_parallel_economy::PracticeReporting::Truthful,
                "lie" => game_parallel_economy::PracticeReporting::Lie,
                "ready" => game_parallel_economy::PracticeReporting::Ready,
                "unready" => game_parallel_economy::PracticeReporting::Unready,
                _ => return Err("invalid reporting choice".into()),
            },
        )?,
        ["practice-attitude", person, attitude] => world.set_practice_report_attitude(
            person.parse().map_err(|_| "invalid person")?,
            match *attitude {
                "ignore" => game_parallel_economy::PracticeReportAttitude::Ignore,
                "trust" => game_parallel_economy::PracticeReportAttitude::Trust,
                "doubt" => game_parallel_economy::PracticeReportAttitude::Doubt,
                _ => return Err("invalid report attitude".into()),
            },
        )?,
        ["practice-attitude", person, "min", threshold] => world.set_practice_report_attitude(
            person.parse().map_err(|_| "invalid person")?,
            game_parallel_economy::PracticeReportAttitude::TrustAtLeast(
                threshold.parse().map_err(|_| "invalid trust threshold")?,
            ),
        )?,
        ["practice-unanswered", person, choice] => world.set_practice_unanswered_belief(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "retain" => None,
                "believe" => Some(game_parallel_economy::PracticeBelief::Believes),
                "doubt" => Some(game_parallel_economy::PracticeBelief::Disbelieves),
                "unassessed" => Some(game_parallel_economy::PracticeBelief::Unassessed),
                _ => return Err("expected retain, believe, doubt, or unassessed".into()),
            },
        )?,
        ["practice-value", person, "off"] => world
            .set_practice_value_learning(person.parse().map_err(|_| "invalid person")?, None)?,
        [
            "practice-value",
            person,
            value,
            minimum,
            maximum,
            received,
            gave,
            unanswered,
        ] => world.set_practice_value_learning(
            person.parse().map_err(|_| "invalid person")?,
            Some(game_parallel_economy::PracticeValueLearning {
                value: value.parse().map_err(|_| "invalid value")?,
                minimum: minimum.parse().map_err(|_| "invalid minimum")?,
                maximum: maximum.parse().map_err(|_| "invalid maximum")?,
                received: received.parse().map_err(|_| "invalid received change")?,
                gave: gave.parse().map_err(|_| "invalid giving change")?,
                unanswered: unanswered
                    .parse()
                    .map_err(|_| "invalid unanswered change")?,
            }),
        )?,
        ["food-misreport", person] => println!(
            "{:?}",
            world.food_misreport(person.parse().map_err(|_| "invalid person")?)?
        ),
        ["food-misreport", person, "off"] => {
            world.set_food_misreport(person.parse().map_err(|_| "invalid person")?, None)?
        }
        ["food-misreport", person, delta] => world.set_food_misreport(
            person.parse().map_err(|_| "invalid person")?,
            Some(game_parallel_economy::Misreport {
                target: None,
                food_delta: delta.parse().map_err(|_| "invalid food offset")?,
            }),
        )?,
        ["food-misreport", person, delta, target] => world.set_food_misreport(
            person.parse().map_err(|_| "invalid person")?,
            Some(game_parallel_economy::Misreport {
                target: Some(target.parse().map_err(|_| "invalid target")?),
                food_delta: delta.parse().map_err(|_| "invalid food offset")?,
            }),
        )?,
        ["food-report-reception", person] => println!(
            "{:?}",
            world.food_report_reception(person.parse().map_err(|_| "invalid person")?)?
        ),
        ["food-report-reception", person, choice] => world.set_food_report_reception(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "default" => None,
                "all" => Some(game_parallel_economy::ReportReception::Always),
                "none" => Some(game_parallel_economy::ReportReception::Never),
                value => Some(game_parallel_economy::ReportReception::TrustAtLeast(
                    value.parse().map_err(|_| "invalid trust threshold")?,
                )),
            },
        )?,
        ["work-value", person, "off"] => {
            world.set_work_offer_value(person.parse().map_err(|_| "invalid person")?, None)?
        }
        ["work-value", person, good, value, maximum] => world.set_work_offer_value(
            person.parse().map_err(|_| "invalid person")?,
            Some(game_parallel_economy::WorkOfferValue {
                good: good.parse().map_err(|_| "invalid good")?,
                value_per_unit: value.parse().map_err(|_| "invalid value")?,
                max_quantity: maximum.parse().map_err(|_| "invalid quantity")?,
            }),
        )?,
        ["farm", person, choice] => world.set_farming_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["social", person, choice] => world.set_social_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["yield", person, choice] => world.set_social_pressure(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["treat", person, choice] => world.set_treatment_choice(
            person.parse().map_err(|_| "invalid person")?,
            match *choice {
                "on" => true,
                "off" => false,
                _ => return Err("expected on or off".into()),
            },
        )?,
        ["go", person, destination] => world.depart(&[Departure {
            person: person.parse().map_err(|_| "invalid person")?,
            destination: destination.parse().map_err(|_| "invalid destination")?,
        }])?,
        ["step"] => advance(world, 1)?,
        ["step", count] => advance(world, count.parse().map_err(|_| "invalid tick count")?)?,
        ["save", path] => game_parallel_economy_cli::save(world, Path::new(path))?,
        ["load", path] => *world = game_parallel_economy_cli::load(Path::new(path))?,
        _ => return Err("unknown command or invalid arguments; enter help".into()),
    }
    Ok(true)
}
fn advance(world: &mut Economy, count: u64) -> Result<(), String> {
    for _ in 0..count {
        let event = world.advance()?;
        println!(
            "tick={} started={} finished={} delivered={} spoiled={}",
            event.tick,
            event.started.len(),
            event.finished.len(),
            event.delivered.len(),
            event.spoiled.len()
        );
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut args = std::env::args_os().skip(1);
    let mut player_mode = false;
    let mut scenario = None;
    let mut seed = None;
    let mut population = None;
    while let Some(arg) = args.next() {
        if arg == "--player" && !player_mode {
            player_mode = true;
        } else if arg == "--seed" && seed.is_none() {
            seed = Some(
                args.next()
                    .ok_or("missing seed")?
                    .to_str()
                    .ok_or("invalid seed")?
                    .parse::<u64>()
                    .map_err(|_| "invalid seed")?,
            );
        } else if arg == "--population" && population.is_none() {
            population = Some(PathBuf::from(
                args.next().ok_or("missing population profile")?,
            ));
        } else if scenario.is_none() && !arg.to_string_lossy().starts_with("--") {
            scenario = Some(PathBuf::from(arg));
        } else {
            return Err("unexpected argument".into());
        }
    }
    let scenario = scenario.unwrap_or_else(|| {
        root.join(if player_mode {
            "content/parallel/economy-campaign.toml"
        } else {
            "content/parallel/economy.toml"
        })
    });
    let mut world = game_parallel_economy_cli::scenario_with_options(
        &scenario,
        &root.join("content"),
        seed,
        population.as_deref(),
    )?;
    println!("Economic simulation; enter help for commands.");
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut lines = stdin.lock().lines();
    loop {
        if interactive {
            print!("> ");
            io::stdout().flush().map_err(|error| error.to_string())?;
        }
        let Some(line) = lines.next() else { break };
        let line = line.map_err(|error| error.to_string())?;
        let result = if player_mode {
            let words: Vec<_> = line.split_whitespace().collect();
            match words.as_slice() {
                ["save", path] => {
                    game_parallel_economy_cli::save(&world, Path::new(path)).map(|()| true)
                }
                ["load", path] => game_parallel_economy_cli::load(Path::new(path)).map(|loaded| {
                    world = loaded;
                    true
                }),
                _ => game_parallel_economy_cli_commands::player::handle(&mut world, &words),
            }
        } else {
            command(&mut world, &line)
        };
        match result {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("{error}"),
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
