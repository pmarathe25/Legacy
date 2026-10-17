use super::*;
fn relation(
    tx: &Transaction<'_>,
    person: PersonId,
    other: PersonId,
) -> game_relationships::Relationship {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&person)?.get(&other))
        .cloned()
        .unwrap_or_default()
}
/// Anticipation uses only co-located visible build, one's own condition and foresight,
/// and personally experienced fear. It does not inspect another person's genes or strength.
fn fear(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> Result<i64> {
    let remembered = i64::from(relation(tx, person, other).fear);
    if game_transport::area(tx.get()?, person)
        .ok()
        .is_none_or(|a| game_transport::area(tx.get().unwrap(), other).ok() != Some(a))
    {
        return Ok(remembered);
    }
    let own = game_vitality::ability(tx, person, "size").max(100);
    let apparent = (game_vitality::ability(tx, other, "size") / 100 * 100).max(100);
    let ratio = apparent.saturating_mul(100) / own;
    let bodies = tx.get::<game_physiology::Bodies>()?;
    let body = &bodies.people[&person];
    let wounds = 100 - (body.health.saturating_mul(100) / body.maximum(&bodies.rules)).min(100);
    let vulnerability = wounds * ratio.min(100) / 100;
    let build = (ratio.saturating_sub(100) / 2).min(100);
    let foresight = (600 + game_vitality::ability(tx, person, "foresight") / 2).min(2000);
    Ok(remembered + (build.max(vulnerability) * foresight / 1000).min(100) as i64)
}
pub(super) fn afraid(tx: &Transaction<'_>, person: PersonId, other: PersonId) -> Result<bool> {
    yielding(tx, person, other, &Purpose::Allegiance)
}
pub(super) fn yielding(
    tx: &Transaction<'_>,
    person: PersonId,
    other: PersonId,
    purpose: &Purpose,
) -> Result<bool> {
    let r = relation(tx, person, other);
    let social = tx.get::<game_relationships::Social>()?;
    let independence = social
        .values
        .get(&person)
        .and_then(|v| v.get("independence"))
        .copied()
        .unwrap_or(0);
    let authority = social
        .beliefs
        .get(&person)
        .map_or(0, |b| b.authority.clamp(-10, 10));
    let conviction = match purpose {
        Purpose::Allegiance => 0,
        Purpose::Endorse { concept, stance } => {
            let difference = (i64::from(social.opinion(person, concept)) - i64::from(*stance))
                .abs()
                .min(200);
            difference * disposition(tx, person, "conviction") / 2000
        }
    };
    let resistance = conviction
        + 20
        + disposition(tx, person, "risk-tolerance") / 20
        + i64::from(independence)
        + i64::from(r.grievance) / 2
        + (disposition(tx, person, "aggression") - 500) / 20
        - i64::from(authority);
    Ok(fear(tx, person, other)? >= resistance.max(10))
}
pub(super) fn escalate(tx: &Transaction<'_>, leader: PersonId, subject: PersonId) -> Result<bool> {
    let r = relation(tx, leader, subject);
    let body = &tx.get::<game_physiology::Bodies>()?.people[&leader];
    let motive = disposition(tx, leader, "aggression") / 4
        + disposition(tx, leader, "ambition") / 10
        + i64::from(r.grievance) * 3;
    let cost = disposition(tx, leader, "patience") / 5
        + i64::from(r.affection.max(0)) * 2
        + fear(tx, leader, subject)? * 2
        + body.hunger.min(1000) as i64 / 2
        + body.fatigue.min(1000) as i64 / 2;
    Ok(motive > cost)
}
pub(super) fn escape(
    tx: &Transaction<'_>,
    person: PersonId,
    other: PersonId,
) -> Result<Option<AreaId>> {
    let area = game_transport::area(tx.get()?, person)?;
    let roads = tx
        .get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&person))
        .and_then(|f| f.get(game_knowledge::roads_key(area).as_str()));
    let Some(roads) = roads else {
        return Ok(None);
    };
    let game_knowledge::FactValue::Roads(roads) = roads.value.as_ref() else {
        return Ok(None);
    };
    let motive = fear(tx, person, other)? * (1000 - disposition(tx, person, "risk-tolerance"))
        / 250
        + game_vitality::ability(tx, person, "foresight").min(16000) as i64 / 50;
    let cost = 50
        + disposition(tx, person, "aggression") / 10
        + i64::from(relation(tx, person, other).grievance);
    Ok(roads
        .links
        .iter()
        .filter(|(to, _)| **to != area)
        .filter(|(_, time)| {
            motive
                > cost
                    + (game_transport::travel_time(tx, person, **time).0 / MINUTE.0).min(10000)
                        as i64
        })
        .min_by_key(|(to, time)| (**time, **to))
        .map(|(to, _)| *to))
}
