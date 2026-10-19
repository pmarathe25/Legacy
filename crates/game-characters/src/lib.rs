use game_packed::Packed;
use game_provisioning as plans;
mod choices;
mod rules;
mod scheduling;
pub use game_choice::Review as IdleReview;
use game_core::*;
pub use game_provisioning::Plan;
use game_sim::*;
pub use game_utility::{Candidate, Decision, choose};
pub use rules::{Configure, Rules, load};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    pub plan: Option<Plan>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<String>,
    #[serde(
        default,
        deserialize_with = "engagement",
        skip_serializing_if = "Option::is_none"
    )]
    pub engagement: Option<u64>,
    pub retry_after: BTreeMap<u64, SimTime>,
    pub last: Option<Decision>,
    pub failures: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expectation: Option<Candidate>,
}
fn engagement<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<u64>, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(value.as_u64().or_else(|| value.get("id")?.as_u64()))
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Minds {
    pub people: Packed<PersonId, Mind>,
    pub rules: Arc<Rules>,
    pub running: bool,
    schedule: scheduling::Schedule,
}
type MindMark = (game_packed::Mark, Arc<Rules>, bool, scheduling::Schedule);
impl Resource for Minds {
    const KEY: &'static str = "characters.minds";
    fn validate(&self) -> Result<()> {
        self.rules.validate()?;
        require(
            self.running || self.schedule == scheduling::Schedule::default(),
            "unstarted review clock",
        )?;
        require(
            self.schedule.agenda.valid()
                && self
                    .schedule
                    .ready
                    .keys()
                    .all(|p| self.people.contains_key(p) && self.schedule.agenda.get(p).is_none())
                && self
                    .schedule
                    .agenda
                    .iter()
                    .all(|(person, _)| self.people.contains_key(person)),
            "invalid review agenda",
        )
    }
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new((
            self.people.begin(),
            self.rules.clone(),
            self.running,
            self.schedule.clone(),
        ))
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        let (mark, rules, running, schedule) =
            *checkpoint.downcast::<MindMark>().expect("mind checkpoint");
        self.people.rollback(mark);
        self.rules = rules;
        self.running = running;
        self.schedule = schedule;
    }
    fn commit(&mut self, checkpoint: Checkpoint) {
        let checkpoint = checkpoint.downcast::<MindMark>().expect("mind checkpoint");
        self.people.commit(checkpoint.0);
    }
    fn validate_checkpoint(&self, _checkpoint: &dyn std::any::Any) -> Result<()> {
        self.rules.validate()
    }
}
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
    let now = tx.now();
    tx.edit::<Minds>()?.schedule.waking.insert(person, now);
    let result = choices::review(tx, person);
    if result.is_ok() && tx.get::<Minds>()?.people.contains_key(&person) {
        let next = scheduling::next(tx, person)?;
        tx.edit::<Minds>()?.schedule.set(person, next);
    }
    tx.edit::<Minds>()?.schedule.waking.remove(&person);
    result
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_choice::plugin(b)?;
    plans::plugin(b)?;
    b.plugin("characters", 1)?;
    b.resource::<Minds>()?;
    scheduling::register(b)?;
    choices::register(b)?;
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
        if let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&r.person) {
            mind.plan = None;
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
        scheduling::wake(tx, r.person, false)
    })?;
    b.subscribe::<game_physiology::Died>("forget-dead-mind", |tx, event| {
        let state = tx.edit::<Minds>()?;
        state.people.erase(&event.person);
        state.schedule.agenda.set(event.person, None);
        state.schedule.ready.remove(&event.person);
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("characters.arrived", |tx, e| {
        scheduling::wake(tx, e.person, false)
    })?;
    b.subscribe::<ActionFinished>("characters.wake", |tx, event| {
        scheduling::wake(tx, event.person, false)
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
