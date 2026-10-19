use super::*;
use game_choice::{Proposal, Wait, motive};
#[derive(Clone, Serialize, Deserialize)]
struct Proceed {
    person: PersonId,
    plan: Plan,
}
impl Request for Proceed {
    const KEY: &'static str = "characters.proceed";
    type Response = game_choice::Outcome;
}
#[derive(Clone, Serialize, Deserialize)]
struct Execute {
    person: PersonId,
    proposal: Proposal,
}
impl Request for Execute {
    const KEY: &'static str = "characters.execute";
    type Response = game_choice::Outcome;
}
fn food(p: PersonId, key: String, plan: Plan, c: Candidate) -> Result<Proposal> {
    Proposal::new(key.clone(), c, Proceed { person: p, plan })
}
pub(super) fn review(tx: &mut Transaction<'_>, p: PersonId) -> Result<()> {
    let _measurement = game_diagnostics::span("character review");
    let body = &tx.get::<game_physiology::Bodies>()?.people[&p];
    let ruler = tx.get::<game_lineage::Lineage>()?.current;
    if !body.alive
        || body.health == 0
        || (ruler == Some(p) && game_invocation::controlled(tx.get()?, tx.now()))
    {
        return Ok(());
    }
    let acting = tx.get::<Actions>()?.0.get(&p).cloned();
    if acting.as_ref().is_some_and(|a| a.kind == "combat") {
        return Ok(());
    }
    if game_transport::area(tx.get()?, p).is_ok() {
        tx.call(game_knowledge::ObserveSurroundings { person: p })?;
        let packs: Vec<_> = tx.get::<game_cargo::Cargo>()?.carried(p).collect();
        for inventory in packs {
            tx.call(game_knowledge::ObserveStore {
                person: p,
                inventory,
            })?;
        }
    }
    let mut offers = tx.call(game_choice::Collect { person: p })?;
    let state = tx.get::<Minds>()?;
    let Some(mind) = state.people.get(&p) else {
        return Ok(());
    };
    let rules = state.rules.clone();
    let pressure = |hunger| plans::hunger_pressure(hunger, rules.meal_hunger, rules.urgent_hunger);
    let body = &tx.get::<game_physiology::Bodies>()?.people[&p];
    let hunger = body.hunger;
    let fatigue = body.fatigue;
    let current = mind.chosen.clone();
    let retained = acting
        .as_ref()
        .is_some_and(|a| a.id != 0 && mind.engagement == Some(a.id));
    let plan = mind.plan.clone();
    let expectation = mind.expectation;
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
    let options = plans::options(tx, p, rules.gathering_meals)?;
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
    for (o, mut c) in options.iter().zip(scores.candidates(&options)) {
        let key = format!("food:{}", o.id);
        if offers.iter().any(|o| o.key == key) {
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
    let mut continuing = None;
    if let Some(action) = &acting {
        let active = offers.iter().position(|o| {
            o.continuation == Some(action.id) || retained && current.as_ref() == Some(&o.key)
        });
        let index = if let Some(index) = active {
            index
        } else {
            let need = if action.kind == "rest" {
                fatigue.saturating_sub(rules.rest_fatigue).min(10000) as i64
            } else {
                0
            };
            let expected = expectation
                .filter(|_| retained && action.kind != "rest")
                .unwrap_or(motive(need, 0, 0, 0, 0));
            let key = current
                .as_deref()
                .filter(|_| retained)
                .unwrap_or("continue");
            offers.push(Proposal::new(key, expected, Wait)?);
            offers.len() - 1
        };
        offers[index].continuation = Some(action.id);
        offers[index].motive.goal += action.until.map_or(0, |at| {
            20 - (at.0.saturating_sub(tx.now().0) / MINUTE.0).min(20) as i64
        });
        continuing = Some(offers[index].motive.id);
    }
    let expected: Vec<_> = offers.iter().map(|p| p.motive).collect();
    game_choice::reweight(tx, p, &mut offers);
    let now = tx.now();
    let caution = 1500 - game_vitality::disposition(tx, p, "risk-tolerance");
    let learning = game_vitality::ability(tx, p, "learning");
    for o in &mut offers {
        if o.continuation.is_none()
            && let Some(until) = tx.get::<Minds>()?.people[&p].retry_after.get(&o.motive.id)
        {
            let remaining = until.0.saturating_sub(now.0).min(rules.retry.0);
            let cost = game_utility::failure_cost(remaining, rules.retry.0, learning, caution);
            o.motive.risk = o.motive.risk.saturating_add(cost);
        }
    }
    for _ in 0..8 {
        let ongoing = tx.get::<Actions>()?.0.get(&p);
        if ongoing.is_none() {
            offers.retain(|o| o.continuation.is_none());
        }
        let Some(mut d) = choose(
            &offers.iter().map(|o| o.motive).collect::<Vec<_>>(),
            ongoing
                .filter(|a| acting.as_ref().is_some_and(|old| old.id == a.id))
                .and(continuing),
            game_vitality::switching_margin(tx, p, rules.switching_margin),
            false,
        ) else {
            break;
        };
        let index = offers.iter().position(|o| o.motive.id == d.id).unwrap();
        let proposal = offers.remove(index);
        let continues = ongoing.is_some_and(|a| proposal.continuation == Some(a.id));
        if continues
            || tx
                .attempt(Execute {
                    person: p,
                    proposal: proposal.clone(),
                })
                .is_ok_and(|outcome| outcome == game_choice::Outcome::Performed)
        {
            let engagement = tx.get::<Actions>()?.0.get(&p).map(|a| a.id);
            let Some(m) = tx.edit::<Minds>()?.people.get_mut(&p) else {
                return Ok(());
            };
            m.retry_after.retain(|_, at| *at > now);
            m.retry_after.remove(&d.id);
            if !continues {
                m.expectation = expected.iter().find(|c| c.id == d.id).copied();
            }
            d.reasons = vec![proposal.key.clone()];
            m.last = Some(d);
            m.chosen = Some(proposal.key);
            m.engagement = engagement;
            return Ok(());
        }
        failure(tx, p, d.id)?;
    }
    Ok(())
}
fn failure(tx: &mut Transaction<'_>, person: PersonId, id: u64) -> Result<()> {
    let now = tx.now();
    let retry = now.after(tx.get::<Minds>()?.rules.retry)?;
    if let Some(m) = tx.edit::<Minds>()?.people.get_mut(&person) {
        m.retry_after.retain(|_, at| *at > now);
        m.retry_after.insert(id, retry);
        m.failures = m.failures.saturating_add(1);
    }
    Ok(())
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_choice::Resolved>("characters.outcome", |tx, e| {
        let mind = tx.edit::<Minds>()?.people.get_mut(&e.person);
        let Some(mind) = mind.filter(|m| m.engagement == Some(e.action)) else {
            return Ok(());
        };
        mind.engagement = None;
        let id = mind.chosen.as_deref().map(game_choice::identity);
        if e.outcome == game_choice::Outcome::Reconsider
            && let Some(id) = id
        {
            failure(tx, e.person, id)?;
        }
        Ok(())
    })?;
    b.request::<Proceed>(|tx, r| {
        let (performed, next) = tx.call(plans::Advance {
            person: r.person,
            plan: r.plan,
        })?;
        if let Some(m) = tx.edit::<Minds>()?.people.get_mut(&r.person) {
            m.plan = next;
        }
        Ok(performed.into())
    })?;
    b.request::<Execute>(|tx, r| {
        tx.cancel_action(r.person)?;
        if !r.proposal.key.starts_with("food:") && r.proposal.key != "reserve" {
            tx.edit::<Minds>()?.people.get_mut(&r.person).unwrap().plan = None;
        }
        tx.call(game_choice::Run(r.proposal.request))
    })?;
    b.subscribe::<game_choice::Changed>("characters.new-evidence", |tx, e| {
        scheduling::wake(tx, e.person, false)
    })
}
