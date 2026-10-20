use game_core::*;
use game_households::Households;
use game_sim::*;
use serde::{Deserialize, Serialize};
/// A prospective parent's own attachment, interests, condition, and current care burden.
/// Independent adult children do not consume a lifetime reproductive allowance.
pub fn desire(tx: &Transaction<'_>, person: PersonId, partner: PersonId) -> Result<i64> {
    let bodies = tx.get::<game_physiology::Bodies>()?;
    let body = &bodies.people[&person];
    if tx
        .get::<game_lifecycle::Population>()?
        .pregnancies
        .values()
        .any(|p| p.mother == person)
    {
        return Ok(i64::MIN / 4);
    }
    let social = tx.get::<game_relationships::Social>()?;
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
        .chain(households.beneficiaries_of(person))
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
    let risk = social
        .relationships
        .get(&person)
        .and_then(|row| row.get(&partner))
        .map_or(0, |r| i64::from(r.fear + r.grievance));
    let motive = game_family_life::attraction(tx, person, partner) - risk
        + i64::from(interest) * 4
        + game_vitality::disposition(tx, person, "empathy") / 20;
    let cost = 40
        + body.hunger.min(1000) as i64
        + body.fatigue.min(1000) as i64
        + i64::from(stress) / 8
        + burden.min(i64::MAX as u64) as i64;
    Ok(motive + drive(tx, person) - cost)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
impl Request for Consider {
    const KEY: &'static str = "parenthood.consider";
    type Response = bool;
}
/// Expressed physiological incentive, not a mandate or a demographic target.
pub fn drive(tx: &Transaction<'_>, person: PersonId) -> i64 {
    let Ok(bodies) = tx.get::<game_physiology::Bodies>() else {
        return 0;
    };
    let Some(body) = bodies.people.get(&person) else {
        return 0;
    };
    let condition = body.health.saturating_mul(1000) / body.maximum(&bodies.rules).max(1);
    game_vitality::disposition(tx, person, "reproductive-drive") * condition.min(1000) as i64 / 3000
}
fn partners(tx: &Transaction<'_>, person: PersonId) -> Vec<PersonId> {
    let mut partners = std::collections::BTreeSet::new();
    if let Some(p) = tx
        .get::<Households>()
        .ok()
        .and_then(|h| h.partners.get(&person))
    {
        partners.insert(*p);
    }
    if let Some(row) = tx
        .get::<game_family_life::Bonds>()
        .ok()
        .and_then(|b| b.partners.get(&person))
    {
        partners.extend(
            row.iter()
                .filter(|(_, k)| **k == game_family_life::Kind::CoParent)
                .map(|(p, _)| *p),
        );
    }
    partners.into_iter().collect()
}
fn roles(
    tx: &Transaction<'_>,
    person: PersonId,
    partner: PersonId,
) -> Result<Option<(PersonId, PersonId)>> {
    let bodies = tx.get::<game_physiology::Bodies>()?;
    if !bodies.people.contains_key(&person) || !bodies.people.contains_key(&partner) {
        return Ok(None);
    }
    if Some(person) == tx.get::<game_lineage::Lineage>()?.current
        || Some(partner) == tx.get::<game_lineage::Lineage>()?.current
        || game_transport::area(tx.get()?, person).ok().is_none()
        || game_transport::area(tx.get()?, person).ok()
            != game_transport::area(tx.get()?, partner).ok()
    {
        return Ok(None);
    }
    for (mother, father) in [(person, partner), (partner, person)] {
        if !tx
            .get::<game_lifecycle::Population>()?
            .pregnancies
            .values()
            .any(|p| p.mother == mother)
            && game_lifecycle::reproductive(tx, mother, game_lifecycle::Role::Gestational)?
            && game_lifecycle::reproductive(tx, father, game_lifecycle::Role::Gamete)?
        {
            return Ok(Some((mother, father)));
        }
    }
    Ok(None)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ConsiderWith {
    pub person: PersonId,
    pub partner: PersonId,
}
impl Request for ConsiderWith {
    const KEY: &'static str = "parenthood.consider-with";
    type Response = bool;
}
fn consider_with(tx: &mut Transaction<'_>, person: PersonId, partner: PersonId) -> Result<bool> {
    if !tx.get::<Households>()?.enabled || !partners(tx, person).contains(&partner) {
        return Ok(false);
    }
    let Some((mother, father)) = roles(tx, person, partner)? else {
        return Ok(false);
    };
    if desire(tx, person, partner)? <= 0 || desire(tx, partner, person)? <= 0 {
        return Ok(false);
    }
    Ok(tx
        .attempt(game_lifecycle::Conceive {
            mother,
            father,
            attendant: person,
        })
        .is_ok())
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    if !tx.get::<Households>()?.enabled {
        return Ok(false);
    }
    for partner in partners(tx, person) {
        if consider_with(tx, person, partner)? {
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("parenthood", 1)?;
    b.request::<ConsiderWith>(|tx, r| consider_with(tx, r.person, r.partner))?;
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
            for partner in partners(tx, e.person) {
                if roles(tx, e.person, partner)?.is_none() {
                    continue;
                }
                let value = desire(tx, e.person, partner)?;
                if value <= 0 {
                    continue;
                }
                game_choice::offer(
                    tx,
                    e.person,
                    format!("parenthood:{}", partner.0),
                    game_choice::motive(drive(tx, e.person), 0, value / 4, 5, 0),
                    game_choice::Undertake {
                        person: e.person,
                        kind: "parenthood".into(),
                        duration: HOUR,
                        request: Pending::immediate(ConsiderWith {
                            person: e.person,
                            partner,
                        }),
                    },
                )?;
            }
        } else {
            consider(tx, e.person)?;
        }
        Ok(())
    })?;
    Ok(())
}
