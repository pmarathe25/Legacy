//! Local, voluntary care composes personal concern with the existing treatment owner.
use game_core::*;
pub use game_nurture::Assist;
mod fostering;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Care {
    pub enabled: bool,
    pub requests: Table<PersonId, PersonId>,
    pub assigned: Table<PersonId, PersonId>,
    pub helping: Table<PersonId, PersonId>,
    pub treated: Table<PersonId, u64>,
    pub cursor: Table<PersonId, PersonId>,
}
impl Resource for Care {
    const KEY: &'static str = "care.neighbors";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "care.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
impl Request for Consider {
    const KEY: &'static str = "care.consider";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RequestAid {
    pub patient: PersonId,
    pub caregiver: PersonId,
}
impl Request for RequestAid {
    const KEY: &'static str = "care.request-aid";
    type Response = bool;
}
fn release(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if let Some(patient) = tx.edit::<Care>()?.helping.remove(&person) {
        tx.edit::<Care>()?.assigned.remove(&patient);
    }
    Ok(())
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    let _measurement = game_diagnostics::span("care planning");
    if !tx.get::<Care>()?.enabled
        || tx.get::<Actions>()?.0.contains_key(&person)
        || tx.get::<game_lineage::Lineage>()?.current == Some(person)
        || game_physiology::capable(tx, person).is_err()
    {
        return Ok(false);
    }
    let Ok(area) = game_transport::area(tx.get()?, person) else {
        return Ok(false);
    };
    fostering::consider(tx, person, area)?;
    let supplies = tx.get::<game_health::TreatmentRules>()?;
    let cargo = tx.get::<game_cargo::Cargo>()?;
    let positions = tx.get::<game_transport::Positions>()?;
    let known = tx
        .get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&person))
        .map(|facts| {
            game_food_memory::with_stores(
                person,
                facts,
                tx.get::<game_objects::Objects>().unwrap(),
                |s| {
                    s.among(
                        cargo
                            .stores_with(supplies.supply_object, supplies.supply)
                            .into_iter()
                            .filter(|id| cargo.0[id].area(positions) == Some(area)),
                    )
                },
            )
        })
        .unwrap_or_default();
    let claims = game_property::ClaimCosts::new(tx, person);
    let inventory = known
        .into_iter()
        .chain(cargo.carried(person))
        .filter(|id| {
            cargo.0.get(id).is_some_and(|i| {
                i.area(positions) == Some(area)
                    && i.amount(supplies.supply_object) >= supplies.supply
            })
        })
        .min_by_key(|id| claims.cost(*id));
    let Some(supplies) = inventory else {
        return Ok(false);
    };
    let empathy = game_vitality::disposition(tx, person, "empathy");
    let altruism = game_vitality::disposition(tx, person, "altruism");
    let social = tx.get::<game_relationships::Social>()?;
    let duty = social
        .values
        .get(&person)
        .and_then(|v| v.get("care"))
        .copied()
        .unwrap_or(0);
    // Wounds are locally observable; other people's beliefs, needs, and private inventories are not read.
    let peers: Vec<_> = tx
        .get::<game_social_life::SocialLife>()?
        .residents
        .get(&area)
        .into_iter()
        .flat_map(|rows| {
            rows.scan_after(
                tx.get::<Care>()
                    .unwrap()
                    .cursor
                    .get(&person)
                    .or(Some(&person)),
            )
            .chain(rows.iter())
            .take(rows.len().min(64))
            .map(|(p, _)| *p)
        })
        .collect();
    let mut patients: Vec<_> = std::iter::once(&person)
        .chain(peers.iter().filter(|p| **p != person))
        .filter_map(|p| {
            if tx.get::<Care>().unwrap().assigned.contains_key(p) {
                return None;
            }
            let body = tx.get::<game_physiology::Bodies>().ok()?.people.get(p)?;
            let max = body.maximum(&tx.get::<game_physiology::Bodies>().ok()?.rules);
            if !body.alive || body.health >= max {
                return None;
            }
            let tie = social
                .relationships
                .get(&person)
                .and_then(|s| s.get(p))
                .cloned()
                .unwrap_or_default();
            let concern = if *p == person {
                500 + (max - body.health).min(100) as i64 * 20
            } else {
                empathy * altruism / 500 + i64::from(duty) * 8 + i64::from(tie.affection) * 8
                    - i64::from(tie.grievance) * 8
                    + if tx.get::<Care>().unwrap().requests.get(p) == Some(&person) {
                        350
                    } else {
                        0
                    }
            };
            let concern = concern - game_property::reluctance(tx, person, supplies) * 5;
            (concern >= 600).then_some((
                std::cmp::Reverse((max - body.health).saturating_add(concern.max(0) as u64)),
                *p,
            ))
        })
        .collect();
    if let Some(last) = peers.last() {
        tx.edit::<Care>()?.cursor.insert(person, *last);
    }
    patients.sort();
    for (_, patient) in patients {
        if tx
            .attempt(game_health::Treat {
                caregiver: person,
                patient,
                supplies,
            })
            .is_ok()
        {
            let s = tx.edit::<Care>()?;
            s.assigned.insert(patient, person);
            s.helping.insert(person, patient);
            return Ok(true);
        }
    }
    Ok(false)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("care", 1)?;
    b.resource::<Care>()?;
    game_nurture::register(b)?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Care>()?.enabled = true;
        Ok(())
    })?;
    b.request::<RequestAid>(|tx, r| {
        game_physiology::capable(tx, r.patient)?;
        require(
            game_transport::area(tx.get()?, r.patient)?
                == game_transport::area(tx.get()?, r.caregiver)?,
            "aid requires contact",
        )?;
        require(
            Some(r.caregiver) != tx.get::<game_lineage::Lineage>()?.current,
            "player decides aid",
        )?;
        tx.edit::<Care>()?.requests.insert(r.patient, r.caregiver);
        consider(tx, r.caregiver)
    })?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    b.subscribe::<game_characters::IdleReview>("care.help", |tx, e| {
        tx.call(Consider { person: e.person })?;
        Ok(())
    })?;
    b.subscribe::<ActionFinished>("care.release", |tx, e| release(tx, e.person))?;
    b.subscribe::<game_physiology::Died>("care.death", |tx, e| release(tx, e.person))?;
    b.subscribe::<game_health::Treated>("care.gratitude", |tx, e| {
        if !e.successful {
            return Ok(());
        }
        let count = tx.edit::<Care>()?.treated.entry(e.caregiver).or_default();
        *count = count.saturating_add(1);
        if e.caregiver != e.patient {
            tx.call(game_relationships::Converse {
                observer: e.patient,
                other: e.caregiver,
                warmth: 3,
            })?;
        }
        tx.call(game_relationships::LearnValue {
            person: e.caregiver,
            concept: "care".into(),
            outcome: 1,
        })?;
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Care>()?;
        require(
            s.assigned.len() == s.helping.len()
                && s.helping.iter().all(|(carer, patient)| {
                    s.assigned.get(patient) == Some(carer)
                        && h.get::<Actions>()
                            .is_ok_and(|a| a.0.get(carer).is_some_and(|w| w.kind == "treat"))
                }),
            "invalid care assignments",
        )
    });
    Ok(())
}
