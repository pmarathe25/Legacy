//! Interactive host for compiled block worlds.
use game_parallel::{
    Amount, GoodsDirection, GoodsTransfer, Move, Shipment, StartWork, TradeOutcome, TradeProposal,
    World,
};
use game_parallel_fused_physical::Kernel as Engine;
use std::io::{self, BufRead, IsTerminal, Write};
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
fn command(engine: &mut Engine, line: &str, player_mode: bool) -> Result<bool, String> {
    if player_mode {
        game_parallel_block_cli_interface::player::check(engine, line)?;
        if line.trim() == "help" {
            println!(
                "observe | social-pressure PERSON enable|disable | talk PERSON TARGET | teach PERSON TARGET RECIPE | learn PERSON TEACHER RECIPE | social PERSON | skills PERSON | spectate-guardian GUARDIAN | missions PERSON | mission-start PERSON DESTINATION VISIT_TICKS | mission-cancel PERSON ID | mission-meet PERSON ID TARGET | mission-consent PERSON ID accept|withdraw | mission-candidates PERSON | mission-invite PERSON ID TARGET | mission-party ID ID... | caregiving PERSON | care-policy PERSON enable|disable | farm-status PERSON | farm-policy PERSON enable|disable | dreams PERSON | dream-set PERSON RECIPE PRIORITY BATCH INTERVAL enable|disable | goals PERSON | request-work REQUESTER WORKER REGION RECIPE EXPIRES | work-requests PERSON | respond-work ID PERSON start|refuse | goal-set PERSON RECIPE PRIORITY TARGET enable|disable | opinions PERSON | property-offer PERSON RECIPIENT REGION GOOD | property-decide OFFER PERSON accept|decline | property-access PERSON RECIPIENT REGION GOOD grant|revoke | property-withdraw OFFER PERSON | agreements PERSON | agreement-policy PERSON ignore|honor|bulk | agreement-propose DEBTOR CREDITOR GOOD QUANTITY DUE | agreement-decide ID ACTOR accept|decline | agreement-deliver ID ACTOR QUANTITY | run TICKS | move PERSON DESTINATION | work ID WORKER REGION RECIPE UNITS | work-unpermitted ID WORKER REGION RECIPE UNITS | food-seeking PERSON enable|disable | food-source PERSON STORE|region | food-stores PERSON | store-build PERSON STORE BLUEPRINT | store-building PERSON enable|disable | store-claim PERSON STORE assert|withdraw | store-objection PERSON STORE enable|disable | confrontation PERSON enable|disable | reproduction PERSON enable|disable | partnership-preference PERSON enable|disable | leave-partnership PERSON | store-take PERSON STORE AMOUNT | store-put PERSON STORE AMOUNT | food-aid PERSON enable|disable | food-deposit PERSON enable|disable | deposit-claim PERSON none|accept|object | food-asking PERSON enable|disable | request-food REQUESTER GIVER ID AMOUNT EXPIRES | food-requests PERSON | withdraw-food PERSON ID | give-food GIVER RECIPIENT FOOD | take PERSON FOOD | store PERSON FOOD | eat PERSON | eat-good PERSON GOOD | pickup-good PERSON GOOD QUANTITY | steal-good PERSON GOOD QUANTITY | deposit-good PERSON GOOD QUANTITY | offer A GOOD_A QUANTITY_A B GOOD_B QUANTITY_B | offers PERSON | accept OFFER PERSON | decline OFFER PERSON | withdraw OFFER PERSON | invoke LIFE... | dismiss-invocation | accept-invocation REVISION | memories | request-release GUARDIAN | quit"
            );
            return Ok(true);
        }
    }
    if game_parallel_block_cli_interface::food::handle(engine, line)? {
        return Ok(true);
    }
    if game_parallel_block_cli_civic::handle(engine, line)? {
        return Ok(true);
    }
    let mut words = line.split_whitespace();
    match words.next() {
        None => {}
        Some("help") => {
            end(&mut words)?;
            println!(
                "observe | social-pressure PERSON enable|disable | talk PERSON TARGET | teach PERSON TARGET RECIPE | learn PERSON TEACHER RECIPE | social PERSON | skills PERSON | spectate-guardian GUARDIAN | missions PERSON | mission-start PERSON DESTINATION VISIT_TICKS | mission-cancel PERSON ID | mission-meet PERSON ID TARGET | mission-consent PERSON ID accept|withdraw | mission-candidates PERSON | mission-invite PERSON ID TARGET | mission-party ID ID... | caregiving PERSON | care-policy PERSON enable|disable | farm-status PERSON | farm-policy PERSON enable|disable | dreams PERSON | dream-set PERSON RECIPE PRIORITY BATCH INTERVAL enable|disable | goals PERSON | request-work REQUESTER WORKER REGION RECIPE EXPIRES | work-requests PERSON | respond-work ID PERSON start|refuse | goal-set PERSON RECIPE PRIORITY TARGET enable|disable | opinions PERSON | agreements PERSON | agreement-policy PERSON ignore|honor|bulk | agreement-propose DEBTOR CREDITOR GOOD QUANTITY DUE | agreement-decide ID ACTOR accept|decline | agreement-deliver ID ACTOR QUANTITY | exposure | disease | care | treat ID CAREGIVER PATIENT | combat | fight ID REGION A B | tactical-fight ID REGION A B AX AY BX BY | tactical-move ID PERSON X Y | combat-order ID PERSON attack TARGET|defend|retreat | catalog-goods | catalog-recipes | status | ruler | properties | property-offer OWNER RECIPIENT REGION GOOD | property-decide OFFER ACTOR accept|decline | property-access OWNER PERSON REGION GOOD grant|revoke | property-withdraw OFFER ACTOR | guardians | request-release GUARDIAN | invocation | invoke LIFE... | dismiss-invocation | accept-invocation REVISION | memories | region ID | inventory REGION | goods PERSON | social PERSON | knowledge PERSON | decisions | run TICKS | move PERSON DESTINATION | work ID WORKER REGION RECIPE UNITS | work-unpermitted ID WORKER REGION RECIPE UNITS | cancel-work ID | assign-work ID WORKER | pickup-good PERSON GOOD QUANTITY | steal-good PERSON GOOD QUANTITY | deposit-good PERSON GOOD QUANTITY | recover-good ACTOR DECEASED GOOD QUANTITY | offer A GOOD_A QUANTITY_A B GOOD_B QUANTITY_B | offers PERSON | accept OFFER PERSON | decline OFFER PERSON | withdraw OFFER PERSON | food-seeking PERSON enable|disable | food-source PERSON STORE|region | food-stores PERSON | store-build PERSON STORE BLUEPRINT | store-building PERSON enable|disable | store-claim PERSON STORE assert|withdraw | store-objection PERSON STORE enable|disable | confrontation PERSON enable|disable | reproduction PERSON enable|disable | partnership-preference PERSON enable|disable | leave-partnership PERSON | store-take PERSON STORE AMOUNT | store-put PERSON STORE AMOUNT | food-aid PERSON enable|disable | food-deposit PERSON enable|disable | deposit-claim PERSON none|accept|object | food-asking PERSON enable|disable | request-food REQUESTER GIVER ID AMOUNT EXPIRES | food-requests PERSON | withdraw-food PERSON ID | give-food GIVER RECIPIENT FOOD | take PERSON FOOD | store PERSON FOOD | eat PERSON | eat-good PERSON GOOD | recover ACTOR DECEASED FOOD | ship ID SOURCE DESTINATION FOOD | save PATH | load PATH | quit"
            );
        }
        Some("catalog-goods") => {
            end(&mut words)?;
            let world = engine.export();
            let production = world.production.as_ref().ok_or("production is disabled")?;
            for good in &production.goods {
                println!("good={} name={}", good.id, good.name);
            }
        }
        Some("catalog-recipes") => {
            end(&mut words)?;
            let world = engine.export();
            let production = world.production.as_ref().ok_or("production is disabled")?;
            for recipe in &production.recipes {
                let category = production
                    .recipe_categories
                    .iter()
                    .find(|row| row.recipe == recipe.id)
                    .map(|row| row.category.as_str());
                println!(
                    "recipe={} name={} ticks={} inputs={:?} outputs={:?} skill={category:?}",
                    recipe.id, recipe.name, recipe.ticks, recipe.inputs, recipe.outputs
                );
            }
        }
        Some("status") => {
            end(&mut words)?;
            let w = engine.export();
            let living = w
                .regions
                .iter()
                .flat_map(|r| &r.people)
                .filter(|p| p.health > 0)
                .count()
                + w.travelers.iter().filter(|t| t.person.health > 0).count();
            println!(
                "tick={} regions={} living={} travelers={} shipments={}",
                w.tick,
                w.regions.len(),
                living,
                w.travelers.len(),
                w.shipments.len()
            );
        }
        Some("ruler") => {
            end(&mut words)?;
            let w = engine.export();
            let lineage = w.lineage.as_ref().ok_or("lineage is disabled")?;
            println!(
                "ruler={:?} started={} waiting_since={:?} past_lives={}",
                lineage.current,
                lineage.started,
                lineage.waiting_since,
                lineage.history.len()
            );
        }
        Some("guardians") => {
            end(&mut words)?;
            let world = engine.export();
            let guardians = world.guardians.as_ref().ok_or("guardians are disabled")?;
            for guardian in &guardians.guardians {
                println!(
                    "guardian={} region={} health={} confined={} journey={:?} student={:?} mentorship={} ready={}",
                    guardian.id,
                    guardian.region,
                    guardian.health,
                    guardian.confined,
                    guardian.journey,
                    guardian.student,
                    guardian.mentorship,
                    guardians.ready(guardian)
                );
            }
        }
        Some("request-release") => {
            let guardian = number(&mut words, "guardian")?;
            end(&mut words)?;
            println!("release={:?}", engine.request_guardian_release(guardian)?);
        }
        Some("invocation") => {
            end(&mut words)?;
            let world = engine.export();
            let invocation = world
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.invocation.as_ref())
                .ok_or("invocation is disabled")?;
            println!(
                "owner={:?} lives={:?} burden={} exposure={} control_until={:?} warning={:?}",
                invocation.owner,
                invocation.active,
                invocation.burden,
                invocation.exposure,
                invocation.control_until,
                invocation.pending
            );
        }
        Some("invoke") => {
            let lives = words
                .map(|word| {
                    word.parse::<u64>()
                        .map_err(|_| "invalid past life".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            engine.invoke_lives(&lives)?;
        }
        Some("dismiss-invocation") => {
            end(&mut words)?;
            engine.dismiss_invocation()?;
        }
        Some("accept-invocation") => {
            let revision = number(&mut words, "warning revision")?;
            end(&mut words)?;
            engine.accept_invocation_warning(revision)?;
        }
        Some("memories") => {
            end(&mut words)?;
            for (life, record) in engine.borrowed_memories()? {
                println!(
                    "life={} region={} food={} observed_at={} witness={} learned_from={:?}",
                    life,
                    record.region,
                    record.food,
                    record.observed_at,
                    record.witness,
                    record.learned_from
                );
            }
        }
        Some("region") => {
            let id: u64 = number(&mut words, "region")?;
            end(&mut words)?;
            let w = engine.export();
            let r = w
                .regions
                .iter()
                .find(|r| r.id == id)
                .ok_or("unknown region")?;
            println!("region={} food={} people={}", r.id, r.food, r.people.len());
            for p in &r.people {
                println!("person={} health={} hunger={}", p.id, p.health, p.hunger);
            }
        }
        Some("inventory") => {
            let id: u64 = number(&mut words, "region")?;
            end(&mut words)?;
            let w = engine.export();
            let p = w.production.as_ref().ok_or("production is disabled")?;
            let inventory = p
                .inventories
                .iter()
                .find(|i| i.region == id)
                .ok_or("unknown inventory")?;
            for s in &inventory.stocks {
                println!(
                    "good={} quantity={} capacity={}",
                    s.good, s.quantity, s.capacity
                );
            }
            for j in p.jobs.iter().filter(|j| j.region == id) {
                println!(
                    "job={} worker={} recipe={} remaining={} phase={:?}",
                    j.id, j.worker, j.recipe, j.remaining, j.phase
                );
            }
        }
        Some("goods") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let w = engine.export();
            let cargo = w.goods_cargo.as_ref().ok_or("goods cargo is disabled")?;
            if let Some(h) = cargo.holdings.iter().find(|h| h.person == person) {
                for item in &h.items {
                    println!(
                        "person={person} good={} quantity={}",
                        item.good, item.quantity
                    );
                }
            }
        }
        Some("teach") => {
            let actor = number(&mut words, "person")?;
            let target = number(&mut words, "target")?;
            let recipe = number(&mut words, "recipe")?;
            end(&mut words)?;
            let accepted = engine.teach(actor, target, recipe)?;
            println!(
                "lesson={} tick={}",
                if accepted { "accepted" } else { "declined" },
                engine.tick()
            );
        }
        Some("talk") => {
            let actor = number(&mut words, "person")?;
            let target = number(&mut words, "target")?;
            end(&mut words)?;
            let accepted = engine.converse(actor, target)?;
            println!(
                "conversation={} tick={}",
                if accepted { "accepted" } else { "declined" },
                engine.tick()
            );
        }
        Some("social") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let w = engine.export();
            let social = w.social.as_ref().ok_or("social is disabled")?;
            for r in social
                .relationships
                .iter()
                .filter(|r| r.a == person || r.b == person)
            {
                println!(
                    "a={} b={} familiarity={} encounters={} last_met={}",
                    r.a, r.b, r.familiarity, r.encounters, r.last_met
                );
            }
        }
        Some("knowledge") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let w = engine.export();
            let mind = w
                .knowledge
                .as_ref()
                .ok_or("knowledge is disabled")?
                .minds
                .iter()
                .find(|m| m.person == person)
                .ok_or("unknown mind")?;
            for r in &mind.records {
                println!(
                    "region={} food={} observed_at={} witness={} learned_from={:?}",
                    r.region, r.food, r.observed_at, r.witness, r.learned_from
                );
            }
        }
        Some("decisions") => {
            end(&mut words)?;
            let w = engine.export();
            for e in &w.autonomy.as_ref().ok_or("autonomy is disabled")?.events {
                println!(
                    "tick={} person={} source={} destination={} outcome={:?} reason={:?}",
                    e.tick, e.person, e.source, e.destination, e.outcome, e.reason
                );
            }
        }
        Some("run") => {
            let ticks: u64 = number(&mut words, "ticks")?;
            end(&mut words)?;
            for _ in 0..ticks {
                if let Err(error) = engine.step() {
                    if error == "invocation warning needs a choice" {
                        let world = engine.export();
                        let warning = world
                            .lineage
                            .as_ref()
                            .and_then(|lineage| lineage.invocation.as_ref())
                            .and_then(|invocation| invocation.pending.as_ref())
                            .expect("warning boundary");
                        println!(
                            "invocation warning: {:?}; accept with accept-invocation {} or dismiss",
                            warning, warning.revision
                        );
                        break;
                    }
                    return Err(format!("tick {}: {error}", engine.tick()));
                }
            }
            println!("tick={}", engine.tick());
        }
        Some("move") => {
            let person = number(&mut words, "person")?;
            let destination = number(&mut words, "destination")?;
            end(&mut words)?;
            engine.move_people(&[Move {
                person,
                destination,
            }])?;
            println!("travel started for person={person}");
        }
        Some(verb @ ("work" | "work-unpermitted")) => {
            let job = StartWork {
                id: number(&mut words, "job")?,
                worker: number(&mut words, "worker")?,
                region: number(&mut words, "region")?,
                recipe: number(&mut words, "recipe")?,
                units: number(&mut words, "units")?,
            };
            end(&mut words)?;
            if verb == "work-unpermitted" {
                engine.work_without_permission(job)?;
            } else {
                engine.start_work(&[job])?;
            }
            println!("job={} started", job.id);
        }
        Some("cancel-work") => {
            let id = number(&mut words, "job")?;
            end(&mut words)?;
            engine.cancel_work(id)?;
            println!("job={id} cancelled");
        }
        Some("assign-work") => {
            let id = number(&mut words, "job")?;
            let worker = number(&mut words, "worker")?;
            end(&mut words)?;
            engine.reassign_work(id, worker)?;
            println!("job={id} worker={worker}");
        }
        Some(verb @ ("pickup-good" | "deposit-good" | "steal-good")) => {
            let person = number(&mut words, "person")?;
            let good = number(&mut words, "good")?;
            let quantity = number(&mut words, "quantity")?;
            end(&mut words)?;
            if verb == "steal-good" {
                let theft = engine.steal_goods(person, good, quantity)?;
                println!("theft owner={} witnessed={}", theft.owner, theft.witnessed);
            } else {
                let direction = if verb == "pickup-good" {
                    GoodsDirection::Pickup
                } else {
                    GoodsDirection::Deposit
                };
                engine.transfer_goods(&[GoodsTransfer {
                    person,
                    good,
                    quantity,
                    direction,
                }])?;
            }
            println!("person={person} good={good} quantity={quantity} {verb}");
        }
        Some("recover-good") => {
            let actor = number(&mut words, "actor")?;
            let deceased = number(&mut words, "deceased")?;
            let good = number(&mut words, "good")?;
            let quantity = number(&mut words, "quantity")?;
            end(&mut words)?;
            engine.recover_goods(actor, deceased, good, quantity)?;
            println!("person={actor} recovered good={good} quantity={quantity}");
        }
        Some("offer") => {
            let a = number(&mut words, "person a")?;
            let ga = number(&mut words, "good a")?;
            let qa = number(&mut words, "quantity a")?;
            let b = number(&mut words, "person b")?;
            let gb = number(&mut words, "good b")?;
            let qb = number(&mut words, "quantity b")?;
            end(&mut words)?;
            let give = if qa == 0 {
                vec![]
            } else {
                vec![Amount {
                    good: ga,
                    quantity: qa,
                }]
            };
            let receive = if qb == 0 {
                vec![]
            } else {
                vec![Amount {
                    good: gb,
                    quantity: qb,
                }]
            };
            let id = engine.propose_trade(TradeProposal {
                proposer: a,
                recipient: b,
                give,
                receive,
            })?;
            println!("offer={id} proposer={a} recipient={b}");
        }
        Some("offers") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let w = engine.export();
            for o in w
                .market
                .as_ref()
                .ok_or("market is disabled")?
                .offers
                .iter()
                .filter(|o| o.proposer == person || o.recipient == person)
            {
                println!(
                    "offer={} proposer={} recipient={} give={:?} receive={:?}",
                    o.id, o.proposer, o.recipient, o.give, o.receive
                );
            }
        }
        Some(verb @ ("accept" | "decline" | "withdraw")) => {
            let id = number(&mut words, "offer")?;
            let actor = number(&mut words, "actor")?;
            end(&mut words)?;
            let outcome = match verb {
                "accept" => TradeOutcome::Accepted,
                "decline" => TradeOutcome::Declined,
                _ => TradeOutcome::Withdrawn,
            };
            engine.decide_trade(id, actor, outcome)?;
            println!("offer={id} actor={actor} outcome={outcome:?}");
        }
        Some("take") => {
            let p = number(&mut words, "person")?;
            let n = number(&mut words, "food")?;
            end(&mut words)?;
            engine.pickup_food(p, n)?;
            println!("food picked up by person={p}");
        }
        Some("store") => {
            let p = number(&mut words, "person")?;
            let n = number(&mut words, "food")?;
            end(&mut words)?;
            engine.deposit_food(p, n)?;
            println!("food stored by person={p}");
        }
        Some("eat") => {
            let p = number(&mut words, "person")?;
            end(&mut words)?;
            engine.eat_carried_food(p)?;
            println!("person={p} ate carried food");
        }
        Some("recover") => {
            let a = number(&mut words, "actor")?;
            let d = number(&mut words, "deceased")?;
            let n = number(&mut words, "food")?;
            end(&mut words)?;
            engine.recover_food(a, d, n)?;
            println!("food recovered by person={a}");
        }
        Some("ship") => {
            let id = number(&mut words, "shipment")?;
            let source = number(&mut words, "source")?;
            let destination = number(&mut words, "destination")?;
            let food = number(&mut words, "food")?;
            end(&mut words)?;
            let w = engine.export();
            let ticks_remaining = w
                .routes
                .iter()
                .find(|r| r.source == source && r.destination == destination)
                .ok_or("no route")?
                .ticks;
            engine.launch_shipments(&[Shipment {
                id,
                source,
                destination,
                food,
                ticks_remaining,
            }])?;
            println!("shipment={id} launched");
        }
        Some("save") => {
            let path = words.next().ok_or("missing path")?;
            end(&mut words)?;
            let w = engine.export();
            w.validate()?;
            game_parallel_persistence::save(
                std::path::Path::new(path),
                &serde_json::to_vec_pretty(&w).map_err(|e| e.to_string())?,
            )?;
            println!("saved {path}");
        }
        Some("load") => {
            let path = words.next().ok_or("missing path")?;
            end(&mut words)?;
            let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
            *engine = Engine::compile(World::from_json(&text)?)?;
            println!("loaded {path}");
        }
        Some("quit") => {
            end(&mut words)?;
            return Ok(false);
        }
        Some(_) => return Err("unknown command; use help".into()),
    }
    Ok(true)
}
fn execute() -> Result<(), String> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    let player_mode = args.first().is_some_and(|arg| arg == "--player");
    if player_mode {
        args.remove(0);
    }
    let world = match game_parallel_block_cli_interface::catalog::select(&args, player_mode)? {
        game_parallel_block_cli_interface::catalog::Selection::List(entries) => {
            for entry in entries {
                println!("{} - {}", entry.id, entry.name);
            }
            return Ok(());
        }
        game_parallel_block_cli_interface::catalog::Selection::World { world, label } => {
            if let Some(label) = label {
                println!("{label}");
            }
            *world
        }
    };
    let mut engine = Engine::compile(world)?;
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    let mut input = stdin.lock();
    let mut line = String::new();
    loop {
        if interactive {
            print!("> ");
            io::stdout().flush().map_err(|e| e.to_string())?;
        }
        line.clear();
        if input.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        match command(&mut engine, &line, player_mode) {
            Ok(true) => {}
            Ok(false) => break,
            Err(error) => eprintln!("error: {error}"),
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
