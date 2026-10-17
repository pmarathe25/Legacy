use super::*;
/// Locally observed need can motivate voluntary care. No distant orphan registry is consulted.
pub(super) fn consider(tx: &mut Transaction<'_>, person: PersonId, area: AreaId) -> Result<()> {
    let Ok(households) = tx.get::<game_households::Households>() else {
        return Ok(());
    };
    if !households.enabled {
        return Ok(());
    }
    let bodies = tx.get::<game_physiology::Bodies>()?;
    let body = &bodies.people[&person];
    if tx.now().0.saturating_sub(body.born.0) < 18 * 365 * DAY.0 {
        return Ok(());
    }
    let empathy = game_vitality::disposition(tx, person, "empathy");
    let social = tx.get::<game_relationships::Social>()?;
    let existing = households.dependents_of(person).count() as i64;
    let children: Vec<_> = tx
        .get::<game_social_life::SocialLife>()?
        .residents
        .get(&area)
        .into_iter()
        .flat_map(|r| r.keys())
        .filter_map(|child| {
            let previous = households.dependents.get(child)?;
            if game_transport::area(tx.get().ok()?, *previous).ok() != Some(area)
                || bodies.people.get(previous)?.alive
                || !bodies.people.get(child)?.alive
            {
                return None;
            }
            let tie = social.relationships.get(&person).and_then(|r| r.get(child));
            let concern = empathy
                + tie.map_or(0, |r| {
                    i64::from(r.affection) * 8 - i64::from(r.grievance) * 8
                });
            let cost =
                600 + existing * 150 + body.hunger.min(1000) as i64 + body.fatigue.min(1000) as i64;
            (concern > cost).then_some(*child)
        })
        .take(1)
        .collect();
    for child in children {
        tx.call(game_households::Foster {
            caregiver: person,
            child,
        })?;
    }
    Ok(())
}
