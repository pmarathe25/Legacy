//! Continuation and failure scoring remain part of individual deliberation.
use crate::{Mind, Rules};
use game_choice::{Proposal, Wait, motive};
use game_core::*;
use game_sim::{Action, Transaction};

pub fn failure_risk(
    tx: &Transaction<'_>,
    person: PersonId,
    mind: &Mind,
    rules: &Rules,
    offers: &mut [Proposal],
) {
    let caution = 1500 - game_vitality::disposition(tx, person, "risk-tolerance");
    let learning = game_vitality::ability(tx, person, "learning");
    for o in offers {
        if o.continuation.is_none()
            && let Some(until) = mind.retry_after.get(&o.motive.id)
        {
            let remaining = until.0.saturating_sub(tx.now().0).min(rules.retry.0);
            let cost = game_utility::failure_cost(remaining, rules.retry.0, learning, caution);
            o.motive.risk = o.motive.risk.saturating_add(cost);
        }
    }
}

pub fn continuation(
    tx: &Transaction<'_>,
    person: PersonId,
    mind: &Mind,
    rules: &Rules,
    acting: Option<&Action>,
    offers: &mut Vec<Proposal>,
) -> Result<Option<u64>> {
    let Some(action) = acting else {
        return Ok(None);
    };
    let retained = action.id != 0 && mind.engagement == Some(action.id);
    let active = offers.iter().position(|o| {
        o.continuation == Some(action.id) || retained && mind.chosen.as_ref() == Some(&o.key)
    });
    let index = if let Some(index) = active {
        index
    } else {
        let fatigue = tx.get::<game_physiology::Bodies>()?.people[&person].fatigue;
        let need = if action.kind == "rest" {
            fatigue.saturating_sub(rules.rest_fatigue).min(10000) as i64
        } else {
            0
        };
        let expected = mind
            .expectation
            .filter(|_| retained && action.kind != "rest")
            .unwrap_or(motive(need, 0, 0, 0, 0));
        let key = mind
            .chosen
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
    Ok(Some(offers[index].motive.id))
}
