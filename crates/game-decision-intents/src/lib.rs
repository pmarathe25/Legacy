//! Complete scored alternatives; sensing effects and action commits stay ordered.
use game_character_state::Minds;
use game_choice::{Proposal, Wait, motive};
use game_core::*;
use game_provisioning::{self as plans, Plan};
use game_sim::*;
use serde::{Deserialize, Serialize};
mod batch;
mod cursor;
mod review_stamp;
pub use batch::{Prepared, register};
pub use cursor::SelectionCursor;
pub use review_stamp::{ReviewStamp, ReviewStatus};
#[derive(Clone, Serialize, Deserialize)]
pub struct Proceed {
    pub person: PersonId,
    pub plan: Plan,
}
impl Request for Proceed {
    const KEY: &'static str = "characters.proceed";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scored {
    pub offers: Vec<Proposal>,
    pub expected: Vec<game_choice::Candidate>,
    pub continuing: Option<u64>,
    pub margin: i64,
}
fn food(p: PersonId, key: String, plan: Plan, c: game_choice::Candidate) -> Result<Proposal> {
    Proposal::new(key, c, Proceed { person: p, plan })
}
pub fn score(
    tx: &Transaction<'_>,
    p: PersonId,
    mut offers: Vec<Proposal>,
    options: Vec<plans::Opportunity>,
    acting: Option<&Action>,
) -> Result<Scored> {
    let _span = game_diagnostics::span("decision scoring");
    let state = tx.get::<Minds>()?;
    let mind = state
        .people
        .get(&p)
        .ok_or_else(|| Error::Missing("mind".into()))?;
    let rules = state.rules.clone();
    let pressure = |hunger| plans::hunger_pressure(hunger, rules.meal_hunger, rules.urgent_hunger);
    let body = &tx.get::<game_physiology::Bodies>()?.people[&p];
    let hunger = body.hunger;
    let fatigue = body.fatigue;
    let current = mind.chosen.clone();
    let plan = mind.plan.clone();
    let mut wait = Proposal::new("wait", motive(0, 0, 0, 0, 0), Wait)?;
    wait.motive.id = 0;
    offers.push(wait);
    if fatigue > rules.rest_fatigue {
        offers.push(Proposal::new(
            "rest",
            motive(
                fatigue.saturating_sub(rules.rest_fatigue).min(10000) as i64,
                0,
                0,
                0,
                0,
            ),
            game_physiology::Rest { person: p },
        )?);
    }
    if let Some(plan) = plan {
        let key = current
            .clone()
            .filter(|k| k.starts_with("food:") || k == "reserve")
            .unwrap_or("food:committed".into());
        offers.push(food(p, key, plan, motive(pressure(hunger), 5, 0, 0, 0))?);
    }
    let independent = tx
        .get::<game_relationships::Social>()?
        .values
        .get(&p)
        .and_then(|v| v.get("independence"))
        .copied()
        .unwrap_or(0);
    let scores = plans::FoodScores {
        hunger,
        value: independent,
        independence: independent,
        risk: 0,
        travel_per_minute: rules.travel_effort_per_minute,
        gathering_effort: rules.gathering_effort,
    };
    let discomfort = game_digestion::discomfort(tx, p);
    let mut keys = game_choice::Keys::new(&offers);
    for (o, mut c) in options.iter().zip(scores.candidates(&options)) {
        let key = format!("food:{}", o.id);
        if !keys.insert(&key, &offers) {
            continue;
        }
        c.need = pressure(hunger.max(o.appetite));
        c.effort = c.effort.saturating_add(discomfort);
        offers.push(food(p, key, o.plan.clone(), c)?);
    }
    if let Some(o) = plans::reserve_plan(tx, p)?
        && !offers.iter().any(|o| o.key == "reserve")
    {
        let foresight = game_vitality::ability(tx, p, "foresight").min(16000) as i64;
        offers.push(food(
            p,
            "reserve".into(),
            o.plan,
            motive(
                0,
                foresight / 25,
                0,
                (o.travel.0 / MINUTE.0).min(10000) as i64,
                0,
            ),
        )?);
    }
    let continuing = game_autonomy_policy::continuation(
        tx,
        p,
        &tx.get::<Minds>()?.people[&p],
        &rules,
        acting,
        &mut offers,
    )?;
    let expected: Vec<_> = offers.iter().map(|p| p.motive).collect();
    game_choice::reweight(tx, p, &mut offers);
    game_autonomy_policy::failure_risk(tx, p, &tx.get::<Minds>()?.people[&p], &rules, &mut offers);

    Ok(Scored {
        offers,
        expected,
        continuing,
        margin: game_vitality::switching_margin(tx, p, rules.switching_margin),
    })
}
