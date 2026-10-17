//! Actual experiences affect individual condition through the vitality owner.
use game_core::*;
use game_sim::*;
fn experience(
    tx: &mut Transaction<'_>,
    person: PersonId,
    stress: i32,
    satisfaction: i32,
) -> Result<()> {
    tx.call(game_vitality::Experience {
        person,
        stress,
        satisfaction,
    })
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("wellbeing", 1)?;
    b.subscribe::<game_physiology::Damaged>("wellbeing.pain", |tx, e| {
        experience(tx, e.person, e.amount.min(1000) as i32 * 5, 0)
    })?;
    b.subscribe::<game_physiology::Fed>("wellbeing.nourished", |tx, e| {
        experience(tx, e.patient, -5, 2)
    })?;
    b.subscribe::<game_physiology::Rested>("wellbeing.rested", |tx, e| {
        experience(tx, e.person, -15, 3)
    })?;
    b.subscribe::<game_pastimes::Experienced>("wellbeing.leisure", |tx, e| {
        if e.completed {
            experience(tx, e.person, -10, 8)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_social_life::Encounter>("wellbeing.company", |tx, e| {
        for (person, other) in [(e.first, e.second), (e.second, e.first)] {
            let affection = tx
                .get::<game_relationships::Social>()?
                .relationships
                .get(&person)
                .and_then(|r| r.get(&other))
                .map_or(0, |r| r.affection);
            experience(tx, person, -affection.signum() * 5, affection.signum() * 3)?;
            let child = tx
                .get::<game_vitality::Vitality>()?
                .people
                .get(&person)
                .is_some_and(|life| tx.now().0.saturating_sub(life.born.0) < 18 * 365 * DAY.0);
            if child && affection > 10 && game_vitality::disposition(tx, person, "empathy") > 200 {
                // Actual conversation supplies attitudes; parentage alone supplies none.
                let social = tx.get::<game_relationships::Social>()?;
                let mut own = social.beliefs.get(&person).cloned().unwrap_or_default();
                let spoken = social.beliefs.get(&other).cloned().unwrap_or_default();
                own.sharing += (i64::from(spoken.sharing) - i64::from(own.sharing)).signum() as i32;
                own.authority +=
                    (i64::from(spoken.authority) - i64::from(own.authority)).signum() as i32;
                own.honesty += (i64::from(spoken.honesty) - i64::from(own.honesty)).signum() as i32;
                tx.call(game_relationships::DefineBeliefs {
                    person,
                    beliefs: own,
                })?;
            }
        }
        Ok(())
    })?;
    Ok(())
}
