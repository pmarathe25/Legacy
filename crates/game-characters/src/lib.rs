//! Autonomous survival uses individual knowledge, committed plans, and explainable utility.
use game_provisioning as plans;
mod rules;
mod utility;
use game_core::*;
pub use game_provisioning::Plan;
use game_sim::*;
pub use rules::{Configure, Rules, load};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub use utility::{Candidate, Decision, choose};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mind {
    pub plan: Option<Plan>,
    pub retry_after: BTreeMap<u64, SimTime>,
    pub last: Option<Decision>,
    pub failures: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Minds {
    pub people: BTreeMap<PersonId, Mind>,
    pub rules: Rules,
    pub running: bool,
}
impl Resource for Minds {
    const KEY: &'static str = "characters.minds";
    fn validate(&self) -> Result<()> {
        self.rules.validate()
    }
}
#[derive(Serialize, Deserialize)]
pub struct Register {
    pub person: PersonId,
    pub mind: Mind,
}
impl Request for Register {
    const KEY: &'static str = "characters.register";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Start;
impl Request for Start {
    const KEY: &'static str = "characters.start";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Review;
impl Request for Review {
    const KEY: &'static str = "characters.review";
    type Response = ();
}
fn proceed(tx: &mut Transaction<'_>, person: PersonId, mut plan: Plan, id: u64) -> Result<()> {
    let result = plans::step(tx, person, &mut plan);
    if result.is_err() {
        plans::learn_failure(tx, person, &plan);
    }
    let retry = tx.now().after(tx.get::<Minds>()?.rules.retry)?;
    let mind = tx.edit::<Minds>()?.people.get_mut(&person).unwrap();
    mind.plan = if matches!(result, Ok(false)) {
        Some(plan)
    } else {
        None
    };
    if result.is_err() {
        mind.failures = mind.failures.saturating_add(1);
        mind.retry_after.insert(id, retry);
    }
    Ok(())
}
fn review(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if game_physiology::capable(tx, person).is_err() {
        return Ok(());
    }
    let ruler = tx.get::<game_lineage::Lineage>()?.current;
    if ruler == Some(person) && game_invocation::controlled(tx.get()?, tx.now()) {
        return Ok(());
    }
    let rules = tx.get::<Minds>()?.rules.clone();
    let body = &tx.get::<game_physiology::Bodies>()?.people[&person];
    let hunger = body.hunger.min((i64::MAX / 4) as u64);
    let fatigue = body.fatigue.min(i64::MAX as u64);
    // Keep food acquisition committed; interrupt other actions only by the authored policy.
    if let Some(action) = tx.get::<Actions>()?.0.get(&person) {
        if hunger < rules.urgent_hunger
            || !rules.hunger_interrupts.contains(&action.kind)
            || tx.get::<Minds>()?.people[&person].plan.is_some()
        {
            return Ok(());
        }
        // There must be an apparent food alternative before abandoning current work.
        if plans::options(tx, person, rules.gathering_meals)?.is_empty() {
            return Ok(());
        }
        tx.cancel_action(person)?;
    }
    if game_transport::area(tx.get()?, person).is_ok() {
        tx.call(game_knowledge::ObserveSurroundings { person })?;
    }
    let mind = tx.get::<Minds>()?.people[&person].clone();
    if let Some(plan) = mind.plan {
        return proceed(tx, person, plan, mind.last.as_ref().unwrap().id);
    }
    let mut candidates = vec![];
    if fatigue > rules.rest_fatigue {
        candidates.push(Candidate {
            id: 1,
            need: fatigue as i64,
            goal: 0,
            value: 0,
            effort: 1,
            risk: 0,
            survival: hunger < rules.urgent_hunger,
            reasons: vec!["fatigue".into()],
        });
    }
    let value = tx
        .get::<game_relationships::Social>()?
        .values
        .get(&person)
        .and_then(|m| m.get("independence"))
        .copied()
        .unwrap_or(0);
    let (value, risk) = if ruler == Some(person) {
        let invocation = tx.get::<game_invocation::Invocation>()?;
        (
            value.saturating_sub(invocation.identity.min(100) as i32),
            invocation.instability.min(i64::MAX as u64) as i64,
        )
    } else {
        (value, 0)
    };
    let options = if hunger >= rules.meal_hunger {
        plans::options(tx, person, rules.gathering_meals)?
    } else {
        vec![]
    };
    candidates.extend(options.iter().map(|o| {
        let foraging = matches!(o.plan, Plan::Forage { .. });
        let travel = o
            .travel
            .0
            .div_ceil(MINUTE.0)
            .saturating_mul(rules.travel_effort_per_minute);
        Candidate {
            id: o.id,
            need: if foraging {
                hunger as i64
            } else {
                (hunger as i64).saturating_mul(2)
            },
            goal: if foraging { 5 } else { 0 },
            value: if foraging { i64::from(value) } else { 0 },
            effort: travel
                .saturating_add(if foraging { rules.gathering_effort } else { 0 })
                .min(i64::MAX as u64) as i64,
            risk: if foraging { risk } else { 0 },
            survival: true,
            reasons: vec![
                if foraging {
                    "obtain own food"
                } else {
                    "known meal"
                }
                .into(),
                format!("known travel {} ms", o.travel.0),
            ],
        }
    }));
    candidates.retain(|c| {
        mind.retry_after
            .get(&c.id)
            .is_none_or(|until| tx.now() >= *until)
    });
    if let Ok(traits) = tx.get::<game_traits::Traits>() {
        let weights = traits.weights(person);
        for c in &mut candidates {
            let apply = |term: &str, value: i64| {
                value.saturating_mul(*weights.get(term).unwrap_or(&100)) / 100
            };
            c.need = apply("need", c.need);
            c.goal = apply("goal", c.goal);
            c.value = apply("value", c.value);
            c.effort = apply("effort", c.effort);
            c.risk = apply("risk", c.risk);
        }
    }
    let Some(decision) = choose(
        &candidates,
        mind.last.as_ref().map(|d| d.id),
        rules.switching_margin,
        hunger >= rules.urgent_hunger,
    ) else {
        return Ok(());
    };
    let id = decision.id;
    let now = tx.now();
    let m = tx.edit::<Minds>()?.people.get_mut(&person).unwrap();
    m.last = Some(decision);
    m.retry_after.retain(|_, until| *until > now);
    if id == 1 {
        if tx.attempt(game_physiology::Rest { person }).is_err() {
            let retry = tx.now().after(rules.retry)?;
            tx.edit::<Minds>()?
                .people
                .get_mut(&person)
                .unwrap()
                .retry_after
                .insert(id, retry);
        }
        Ok(())
    } else {
        let plan = options.into_iter().find(|o| o.id == id).unwrap().plan;
        proceed(tx, person, plan, id)
    }
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("characters", 1)?;
    b.resource::<Minds>()?;
    b.request::<Configure>(|tx, r| {
        r.0.validate()?;
        require(
            tx.get::<Minds>()?.people.is_empty() && !tx.get::<Minds>()?.running,
            "autonomy already initialized",
        )?;
        tx.edit::<Minds>()?.rules = r.0;
        Ok(())
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
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("forget-dead-mind", |tx, event| {
        tx.edit::<Minds>()?.people.remove(&event.person);
        Ok(())
    })?;
    b.validate_save(|host| {
        for (person, mind) in &host.get::<Minds>()?.people {
            require(
                host.get::<game_physiology::Bodies>()?
                    .people
                    .contains_key(person),
                "mind without a body",
            )?;
            require(
                mind.plan.is_none() || mind.last.is_some(),
                "plan without a decision",
            )?;
        }
        Ok(())
    });
    b.request::<Start>(|tx, _| {
        require(!tx.get::<Minds>()?.running, "already reviewing")?;
        tx.edit::<Minds>()?.running = true;
        tx.schedule(tx.now().after(tx.get::<Minds>()?.rules.review)?, 50, Review)?;
        Ok(())
    })?;
    b.request::<Review>(|tx, _| {
        let people: Vec<_> = tx.get::<Minds>()?.people.keys().copied().collect();
        for person in people {
            review(tx, person)?;
        }
        tx.schedule(tx.now().after(tx.get::<Minds>()?.rules.review)?, 50, Review)?;
        Ok(())
    })
}
