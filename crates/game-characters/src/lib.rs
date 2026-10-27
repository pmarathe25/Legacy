use game_character_choices as choices;
use game_character_clock as scheduling;
use game_provisioning as plans;
mod cohort;
mod trace;
pub use game_autonomy_policy::{AttentionRates, Configure, Mind, Resume, Rules, load};
pub use game_character_clock::{Alert, ConfigureAttention, ConfigureBusyAttention};
pub use game_character_state::Minds;
pub use game_choice::Review as IdleReview;
use game_core::*;
pub use game_provisioning::Plan;
use game_sim::*;
pub use game_utility::{Candidate, Decision, choose};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize)]
pub struct Register {
    pub person: PersonId,
    pub mind: Mind,
}
impl Request for Register {
    const KEY: &'static str = "characters.register";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Abandon {
    pub person: PersonId,
}
impl Request for Abandon {
    const KEY: &'static str = "characters.abandon";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Start;
impl Request for Start {
    const KEY: &'static str = "characters.start";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Review {
    after: Option<PersonId>,
}
impl Request for Review {
    const KEY: &'static str = "characters.review";
    type Response = ();
}
fn review(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let trace = trace::begin(tx, person);
    if !scheduling::begin_review(tx, person)? {
        return Ok(());
    }
    let now = tx.now();
    tx.edit::<Minds>()?.schedule.waking.insert(person, now);
    let tuning = game_choice::frame_tuning(tx);
    let tracking = tuning.nonurgent_millis > 0 || tuning.full_review_millis > 0;
    if tracking {
        require(
            !tx.get::<Minds>()?.cohort_urgency.contains_key(&person),
            "review already active",
        )?;
        tx.edit::<Minds>()?.cohort_urgency.insert(person, 0);
    }
    let result = choices::review(tx, person);
    let interrupted = tuning.full_review_millis > 0
        && tx
            .get::<Minds>()?
            .cohort_urgency
            .get(&person)
            .is_some_and(|n| *n > 0);
    if result.is_ok() && tx.get::<Minds>()?.people.contains_key(&person) {
        let next = scheduling::next(tx, person)?;
        tx.edit::<Minds>()?.schedule.set(person, next);
    }
    let state = tx.edit::<Minds>()?;
    state.clear_wake_at(person, now);
    if tracking {
        state.cohort_urgency.remove(&person);
    }
    if interrupted && result.is_ok() && tx.get::<Minds>()?.people.contains_key(&person) {
        scheduling::wake(tx, person, false, true)?;
    }
    trace.annotate("after", || trace::state(tx, person));
    trace.annotate("success", || serde_json::json!(result.is_ok()));
    result
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_choice::plugin(b)?;
    plans::plugin(b)?;
    b.plugin("characters", 1)?;
    b.resource::<Minds>()?;
    scheduling::register(b, review, cohort::resolve)?;
    game_decision_food::register(b, |tx| Ok(tx.get::<Minds>()?.rules.gathering_meals))?;
    game_decision_intents::register(b)?;
    choices::register(b)?;
    game_character_activities::register(b)?;
    cohort::register(b)?;
    b.request::<Configure>(|tx, r| {
        r.0.validate()?;
        require(
            tx.get::<Minds>()?.people.is_empty() && !tx.get::<Minds>()?.running,
            "autonomy already initialized",
        )?;
        tx.edit::<Minds>()?.rules = Arc::new(r.0);
        Ok(())
    })?;
    b.request::<Abandon>(|tx, r| {
        scheduling::cancel_consideration(tx, r.person)?;
        let execution = tx.get::<game_choice::Offers>()?.goal_execution;
        if let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&r.person) {
            if execution {
                mind.chosen = None;
            }
            mind.plan = None;
            mind.routine = None;
            mind.considered = None;
        }
        tx.cancel_action(r.person)
    })?;
    b.request::<Register>(|tx, r| {
        require(
            tx.get::<game_physiology::Bodies>()?
                .people
                .contains_key(&r.person),
            "unknown person",
        )?;
        require(r.mind.plan.is_none(), "register without an active plan")?;
        tx.call(game_knowledge::ObserveSurroundings { person: r.person })?;
        tx.edit::<Minds>()?.people.insert(r.person, r.mind);
        scheduling::wake(tx, r.person, false, true)
    })?;
    b.subscribe::<game_physiology::Died>("forget-dead-mind", |tx, event| {
        scheduling::cancel_consideration(tx, event.person)?;
        let state = tx.edit::<Minds>()?;
        state.people.erase(&event.person);
        state.schedule.agenda.set(event.person, None);
        state.schedule.ready.remove(&event.person);
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("characters.arrived", |tx, e| {
        scheduling::wake(tx, e.person, false, true)
    })?;
    b.subscribe::<ActionFinished>("characters.wake", |tx, event| {
        scheduling::completed(tx, event.person, false, false)
    })?;
    b.validate_save(|host| {
        let state = host.get::<Minds>()?;
        let actions = &host.get::<Actions>()?.0;
        require(
            !state.running
                || (state.schedule.valid_at(host.now(), state.rules.review)
                    && state.people.keys().all(|p| {
                        state.schedule.ready.contains_key(p)
                            || state.schedule.agenda.get(p).is_some()
                            || actions.contains_key(p)
                    })),
            "invalid review clock",
        )?;
        let bodies = &host.get::<game_physiology::Bodies>()?.people;
        for (person, mind) in state.people.iter() {
            require(bodies.contains_key(person), "mind without a body")?;
            require(
                mind.plan.is_none() || mind.last.is_some(),
                "plan without a decision",
            )?;
        }
        Ok(())
    });
    b.request::<Start>(|tx, _| {
        require(!tx.get::<Minds>()?.running, "already reviewing")?;
        let now = tx.now();
        let state = tx.edit::<Minds>()?;
        state.running = true;
        state.schedule.epoch = now;
        state.schedule.ready = state.people.keys().map(|p| (*p, ())).collect();
        tx.schedule(
            tx.now().after(tx.get::<Minds>()?.rules.review)?,
            50,
            Review { after: None },
        )?;
        Ok(())
    })?;
    b.request::<Review>(|tx, _| {
        let now = tx.now();
        if tx.get::<Minds>()?.schedule.cycle != now {
            game_food_memory::retain_people(
                tx.get::<game_physiology::Bodies>()?
                    .people
                    .rows()
                    .filter(|(_, p)| p.alive)
                    .map(|(p, _)| *p),
            );
        }
        let people: Vec<_> = tx.get::<Minds>()?.schedule.due(&now).take(128).collect();
        let state = tx.edit::<Minds>()?;
        if state.schedule.cycle != now {
            state.schedule.cycle = now;
            state.schedule.cursor = None;
        }
        for person in people {
            let state = tx.edit::<Minds>()?;
            state.schedule.cursor = Some(person);
            // A gathered actor's bounded cohort job owns this frame review.
            if game_choice::frame_tuning(tx).meal_arbitration
                && game_choice::frame_start(tx) == Some(now)
                && tx
                    .get::<game_choice::Offers>()?
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.people.contains(&person))
            {
                continue;
            }
            review(tx, person)?;
        }
        if tx.get::<Minds>()?.schedule.due(&now).next().is_some() {
            tx.schedule(
                now,
                50,
                Review {
                    after: tx.get::<Minds>()?.schedule.cursor,
                },
            )?;
        } else {
            tx.edit::<Minds>()?.schedule.cursor = Some(PersonId(u64::MAX));
            tx.schedule(
                now.after(tx.get::<Minds>()?.rules.review)?,
                50,
                Review { after: None },
            )?;
        }
        Ok(())
    })
}
