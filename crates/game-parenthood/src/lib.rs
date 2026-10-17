use game_core::*;
use game_households::Households;
use game_sim::*;
use serde::{Deserialize, Serialize};
/// A prospective parent's own attachment, interests, condition, and current care burden.
/// Independent adult children do not consume a lifetime reproductive allowance.
fn child(tx: &Transaction<'_>, person: PersonId, partner: PersonId) -> Result<bool> {
    let bodies = tx.get::<game_physiology::Bodies>()?;
    let body = &bodies.people[&person];
    if tx
        .get::<game_lifecycle::Population>()?
        .pregnancies
        .values()
        .any(|p| p.mother == person || p.father == person)
    {
        return Ok(false);
    }
    let social = tx.get::<game_relationships::Social>()?;
    let tie = &social.relationships[&person][&partner];
    let interest = social
        .values
        .get(&person)
        .and_then(|v| v.get("kinship"))
        .copied()
        .unwrap_or(0);
    let households = tx.get::<Households>()?;
    let children: std::collections::BTreeSet<_> = households
        .children
        .get(&person)
        .into_iter()
        .flatten()
        .copied()
        .chain(households.dependents_of(person))
        .collect();
    let burden: u64 = children
        .iter()
        .filter_map(|id| bodies.people.get(id))
        .filter(|b| b.alive)
        .map(|b| {
            (18 * 365 * DAY.0).saturating_sub(tx.now().0.saturating_sub(b.born.0)) / (365 * DAY.0)
                * 5
        })
        .sum();
    let stress = tx
        .get::<game_vitality::Vitality>()
        .ok()
        .and_then(|v| v.people.get(&person))
        .map_or(0, |l| l.condition(tx.now()).0);
    let motive = i64::from(tie.affection) * 2 + i64::from(tie.trust)
        - i64::from(tie.fear) * 4
        - i64::from(tie.grievance) * 4
        + i64::from(interest) * 4
        + game_vitality::disposition(tx, person, "empathy") / 20;
    let cost = 40
        + body.hunger.min(1000) as i64
        + body.fatigue.min(1000) as i64
        + i64::from(stress) / 8
        + burden.min(i64::MAX as u64) as i64;
    Ok(motive > cost)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
impl Request for Consider {
    const KEY: &'static str = "parenthood.consider";
    type Response = bool;
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    if !tx.get::<Households>()?.enabled
        || Some(person) == tx.get::<game_lineage::Lineage>()?.current
    {
        return Ok(false);
    }
    let Some(partner) = tx.get::<Households>()?.partners.get(&person).copied() else {
        return Ok(false);
    };
    if person > partner || Some(partner) == tx.get::<game_lineage::Lineage>()?.current {
        return Ok(false);
    }
    let population = tx.get::<game_lifecycle::Population>()?;
    let bodies = tx.get::<game_physiology::Bodies>()?;
    if ![person, partner]
        .iter()
        .all(|p| game_lifecycle::fertile(&bodies.people[p], population.adult_age, tx.now()))
        || !child(tx, person, partner)?
        || !child(tx, partner, person)?
    {
        return Ok(false);
    }
    Ok(tx
        .attempt(game_lifecycle::Conceive {
            mother: person,
            father: partner,
            caregiver: person,
        })
        .is_ok())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("parenthood", 1)?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    b.subscribe::<game_households::Formed>("parenthood.intentions", |tx, e| {
        consider(tx, e.first.min(e.second))?;
        Ok(())
    })?;
    b.subscribe::<game_characters::IdleReview>("a.parenthood.consider", |tx, e| {
        consider(tx, e.person)?;
        Ok(())
    })?;
    Ok(())
}
