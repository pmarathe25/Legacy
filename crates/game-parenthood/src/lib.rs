use game_core::*;
use game_households::Households;
use game_sim::*;
use serde::{Deserialize, Serialize};
/// A prospective parent's own attachment, interests, condition, and current care burden.
/// Independent adult children do not consume a lifetime reproductive allowance.
fn child(tx: &Transaction<'_>, person: PersonId, partner: PersonId) -> Result<i64> {
    let bodies = tx.get::<game_physiology::Bodies>()?;
    let body = &bodies.people[&person];
    if tx
        .get::<game_lifecycle::Population>()?
        .pregnancies
        .values()
        .any(|p| p.mother == person || p.father == person)
    {
        return Ok(i64::MIN / 4);
    }
    let social = tx.get::<game_relationships::Social>()?;
    let tie = social
        .relationships
        .get(&person)
        .and_then(|r| r.get(&partner))
        .cloned()
        .unwrap_or_default();
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
    Ok(motive - cost)
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
    let partner = tx
        .get::<Households>()?
        .partners
        .get(&person)
        .copied()
        .or_else(|| {
            tx.get::<game_family_life::Bonds>()
                .ok()?
                .partners
                .get(&person)?
                .iter()
                .find(|(_, k)| **k == game_family_life::Kind::CoParent)
                .map(|(p, _)| *p)
        });
    let Some(partner) = partner else {
        return Ok(false);
    };
    if Some(partner) == tx.get::<game_lineage::Lineage>()?.current
        || child(tx, person, partner)? <= 0
        || child(tx, partner, person)? <= 0
    {
        return Ok(false);
    }
    // Each physical role is tested separately, independently of ID ordering or affection.
    for (mother, father) in [(person, partner), (partner, person)] {
        if game_lifecycle::reproductive(tx, mother, game_lifecycle::Role::Gestational)?
            && game_lifecycle::reproductive(tx, father, game_lifecycle::Role::Gamete)?
        {
            return Ok(tx
                .attempt(game_lifecycle::Conceive {
                    mother,
                    father,
                    caregiver: person,
                })
                .is_ok());
        }
    }
    Ok(false)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("parenthood", 1)?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    b.subscribe::<game_households::Formed>("parenthood.intentions", |tx, e| {
        if tx.get::<game_characters::Minds>()?.running {
            for person in [e.first, e.second] {
                tx.notify(game_choice::Changed { person })?;
            }
        } else {
            consider(tx, e.first.min(e.second))?;
        }
        Ok(())
    })?;
    game_choice::provider(b, "a.parenthood.consider", |tx, e| {
        if game_choice::collecting(tx, e.person) {
            if !tx.get::<Households>()?.enabled {
                return Ok(());
            }
            let partner = tx
                .get::<Households>()?
                .partners
                .get(&e.person)
                .copied()
                .or_else(|| {
                    tx.get::<game_family_life::Bonds>()
                        .ok()?
                        .partners
                        .get(&e.person)?
                        .iter()
                        .find(|(_, k)| **k == game_family_life::Kind::CoParent)
                        .map(|(p, _)| *p)
                });
            if let Some(partner) = partner {
                let pop = tx.get::<game_lifecycle::Population>()?;
                let body = &tx.get::<game_physiology::Bodies>()?.people[&e.person];
                if game_lifecycle::fertile(body, pop.adult_age, tx.now()) {
                    let value = child(tx, e.person, partner)? / 4;
                    game_choice::offer(
                        tx,
                        e.person,
                        format!("parenthood:{}", partner.0),
                        game_choice::motive(0, 0, value, 5, 0),
                        game_choice::Undertake {
                            person: e.person,
                            kind: "parenthood".into(),
                            duration: HOUR,
                            request: Pending::immediate(Consider { person: e.person }),
                        },
                    )?;
                }
            }
        } else {
            consider(tx, e.person)?;
        }
        Ok(())
    })?;
    Ok(())
}
