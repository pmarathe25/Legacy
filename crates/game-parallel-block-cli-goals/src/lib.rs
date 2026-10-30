//! Personal work-goal commands.
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
use game_parallel::{SetDream, SetGoal};
use game_parallel_fused_physical::Kernel;
pub fn handle(engine: &mut Kernel, line: &str) -> Result<bool, String> {
    let mut words = line.split_whitespace();
    match words.next() {
        Some("request-work") => {
            let requester = number(&mut words, "requester")?;
            let worker = number(&mut words, "worker")?;
            let region = number(&mut words, "region")?;
            let recipe = number(&mut words, "recipe")?;
            let expires = number(&mut words, "expiry tick")?;
            end(&mut words)?;
            let id = engine.request_work(game_parallel::work_requests::Proposal {
                requester,
                worker,
                region,
                recipe,
                expires,
            })?;
            println!("work_request={id}");
        }
        Some("work-requests") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let requests = world
                .autowork
                .as_ref()
                .and_then(|owner| owner.requests.as_ref())
                .ok_or("work requests are disabled")?;
            for row in requests.pending.iter().filter(|row| row.worker == person) {
                println!(
                    "work_request={} requester={} worker={} region={} recipe={} expires={}",
                    row.id, row.requester, row.worker, row.region, row.recipe, row.expires
                );
            }
            for event in requests
                .events
                .iter()
                .filter(|event| event.worker == person)
            {
                println!(
                    "work_request={} worker={} tick={} outcome={:?}",
                    event.request, event.worker, event.tick, event.outcome
                );
            }
        }
        Some("respond-work") => {
            let id = number(&mut words, "request")?;
            let worker = number(&mut words, "worker")?;
            let start = match words.next() {
                Some("start") => true,
                Some("refuse") => false,
                _ => return Err("expected start or refuse".into()),
            };
            end(&mut words)?;
            engine.respond_work(id, worker, start)?;
            println!(
                "work_request={id} response={}",
                if start { "started" } else { "refused" }
            );
        }
        Some("learn") => {
            let student = number(&mut words, "person")?;
            let teacher = number(&mut words, "teacher")?;
            let recipe = number(&mut words, "recipe")?;
            end(&mut words)?;
            let accepted = engine.learn(student, teacher, recipe)?;
            println!(
                "lesson={} tick={}",
                if accepted { "accepted" } else { "declined" },
                engine.tick()
            );
        }
        Some("skills") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let skills = world.skills.as_ref().ok_or("skills are disabled")?;
            let index = skills.index();
            for rule in &skills.rules {
                let rate = index.rate(person, rule.recipe);
                if rate > 1 {
                    println!(
                        "person={person} skill_rate recipe={} rate={rate}",
                        rule.recipe
                    );
                }
            }
            for lesson in skills
                .lessons
                .iter()
                .filter(|lesson| lesson.teacher == person || lesson.student == person)
            {
                println!(
                    "lesson teacher={} student={} recipe={} tick={} source_life={:?}",
                    lesson.teacher, lesson.student, lesson.recipe, lesson.tick, lesson.source_life
                );
            }
        }
        Some("dream-set") => {
            let person = number(&mut words, "person")?;
            let recipe = number(&mut words, "recipe")?;
            let priority = number(&mut words, "priority")?;
            let batch = number(&mut words, "batch")?;
            let review_interval = number(&mut words, "review interval")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("aspiration must be enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_dream(SetDream {
                person,
                recipe,
                priority,
                batch,
                review_interval,
                enabled,
            })?;
        }
        Some("dreams") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let dreams = world.dreams.as_ref().ok_or("aspirations are disabled")?;
            for dream in dreams.people.iter().filter(|dream| dream.person == person) {
                println!(
                    "person={} recipe={} priority={} batch={} review_interval={} next_review={} enabled={}",
                    dream.person,
                    dream.recipe,
                    dream.priority,
                    dream.batch,
                    dream.review_interval,
                    dream.next_review,
                    dream.enabled
                );
            }
        }
        Some("goal-set") => {
            let person = number(&mut words, "person")?;
            let recipe = number(&mut words, "recipe")?;
            let priority = number(&mut words, "priority")?;
            let target = number(&mut words, "target")?;
            let enabled = match words.next() {
                Some("enable") => true,
                Some("disable") => false,
                _ => return Err("goal must be enable or disable".into()),
            };
            end(&mut words)?;
            engine.set_goal(SetGoal {
                person,
                recipe,
                priority,
                target,
                enabled,
            })?;
        }
        Some("goals") => {
            let person: u64 = number(&mut words, "person")?;
            end(&mut words)?;
            let world = engine.export();
            let goals = world.goals.as_ref().ok_or("goals are disabled")?;
            for goal in goals.people.iter().filter(|goal| goal.person == person) {
                println!(
                    "person={} recipe={} priority={} progress={} target={} enabled={} next_review={}",
                    goal.person,
                    goal.recipe,
                    goal.priority,
                    goal.progress,
                    goal.target,
                    goal.enabled,
                    goal.next_review
                );
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Show the current player's own work goals.
pub fn print(goals: &[game_parallel::Goal]) {
    for goal in goals {
        println!(
            "goal recipe={} progress={} target={} priority={} enabled={}",
            goal.recipe, goal.progress, goal.target, goal.priority, goal.enabled
        );
    }
}

/// Show only the current player's own aspirations.
pub fn print_dreams(dreams: &[game_parallel::Dream]) {
    for dream in dreams {
        println!(
            "aspiration recipe={} batch={} priority={} enabled={}",
            dream.recipe, dream.batch, dream.priority, dream.enabled
        );
    }
}

/// Show the current player's own stable dispositions.
pub fn print_personality(person: Option<&game_parallel::PersonalityPerson>) {
    if let Some(person) = person {
        println!(
            "sociability={} diligence={}",
            person.sociability, person.diligence
        );
    }
}

/// Show the currently experienced season.
pub fn print_season(season: Option<&str>) {
    if let Some(season) = season {
        println!("season={season}");
    }
}

/// Show the ruler's own disposition and locally witnessed property incidents.
pub fn print_personal(view: &game_parallel_observation::View) {
    print_personality(view.personality.as_ref());
    for skill in &view.skill_rates {
        println!("skill_rate recipe={} rate={}", skill.recipe, skill.rate);
    }
    for lesson in &view.lessons {
        println!(
            "lesson teacher={} student={} recipe={} tick={} source_life={:?}",
            lesson.teacher, lesson.student, lesson.recipe, lesson.tick, lesson.source_life
        );
    }
    for skill in &view.borrowed_skills {
        println!(
            "invoked_skill life={} recipe={} rate={}",
            skill.life, skill.recipe, skill.rate
        );
    }

    for claim in &view.property_claims {
        println!(
            "property region={} good={} owner={} delegates={:?}",
            claim.region, claim.good, claim.owner, claim.delegates
        );
    }
    for offer in &view.property_offers {
        println!(
            "property_offer={} region={} good={} proposer={} recipient={}",
            offer.id, offer.region, offer.good, offer.proposer, offer.recipient
        );
    }
    for event in &view.estates {
        println!(
            "inheritance region={} good={} previous_owner={} heir={} tick={}",
            event.region, event.good, event.previous_owner, event.heir, event.tick
        );
    }
    for event in &view.thefts {
        println!(
            "theft actor={} owner={} region={} good={} quantity={} witnessed={} tick={}",
            event.actor,
            event.owner,
            event.region,
            event.good,
            event.quantity,
            event.witnessed,
            event.tick
        );
    }
}
