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
    #[serde(default)]
    pub sought: Table<PersonId, Table<PersonId, (SimTime, u64)>>,
    #[serde(default)]
    pub received: Table<PersonId, Table<PersonId, (SimTime, u64)>>,
    #[serde(default)]
    pub request_cursor: Table<PersonId, PersonId>,
    pub ongoing: Table<PersonId, PersonId>,
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
fn requested(tx: &Transaction<'_>, patient: PersonId, caregiver: PersonId) -> bool {
    tx.get::<Care>().is_ok_and(|c| {
        c.requests.get(&patient) == Some(&caregiver)
            || c.received
                .get(&caregiver)
                .is_some_and(|r| r.contains_key(&patient))
    })
}
fn injury(tx: &Transaction<'_>, person: PersonId) -> Result<u64> {
    let bodies = tx.get::<game_physiology::Bodies>()?;
    let body = bodies
        .people
        .get(&person)
        .ok_or_else(|| Error::Missing("patient".into()))?;
    Ok(body
        .maximum(&bodies.rules)
        .saturating_sub(body.health)
        .min(100))
}
#[derive(Clone, Serialize, Deserialize)]
struct Appeal(RequestAid);
impl Request for Appeal {
    const KEY: &'static str = "care.appeal";
    type Response = game_choice::Outcome;
}
fn release(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    if let Some(patient) = tx.edit::<Care>()?.helping.remove(&person) {
        tx.edit::<Care>()?.ongoing.remove(&patient);
    }
    Ok(())
}
fn known_supplies(
    tx: &Transaction<'_>,
    p: PersonId,
    good: GoodId,
    quantity: u64,
    area: AreaId,
) -> Vec<InventoryId> {
    tx.get::<game_knowledge::Knowledge>()
        .ok()
        .and_then(|k| k.0.get(&p))
        .map(|facts| {
            game_food_memory::with_stores(p, facts, tx.get().expect("objects"), |s| {
                s.all()
                    .filter_map(|store| {
                        let location = store.location.as_ref()?;
                        ((location.carrier == Some(p) || location.area == area)
                            && store.stock.as_ref()?.get(&good).copied().unwrap_or(0) >= quantity)
                            .then_some(store.id)
                    })
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect()
            })
        })
        .unwrap_or_default()
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
    let known = known_supplies(tx, person, supplies.supply_object, supplies.supply, area);
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
    let duty = social.opinion(person, "care");
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
            if tx.get::<Care>().unwrap().ongoing.contains_key(p) {
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
                    + if requested(tx, *p, person) { 350 } else { 0 }
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
            s.ongoing.insert(patient, person);
            s.helping.insert(person, patient);
            return Ok(true);
        }
    }
    Ok(false)
}
#[derive(Clone, Serialize, Deserialize)]
struct Treatment {
    caregiver: PersonId,
    patient: PersonId,
    supplies: InventoryId,
}
impl Request for Treatment {
    const KEY: &'static str = "care.treatment";
    type Response = game_choice::Outcome;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("care", 2)?;
    b.resource::<Care>()?;
    b.request::<Appeal>(|tx, r| {
        let need = injury(tx, r.0.patient)?;
        let delivered = tx.attempt(r.0.clone()).is_ok_and(|sent| sent);
        let now = tx.now();
        let care = tx.edit::<Care>()?;
        care.sought
            .entry(r.0.patient)
            .or_default()
            .insert(r.0.caregiver, (now, need));
        care.request_cursor.insert(r.0.patient, r.0.caregiver);
        Ok(if delivered {
            game_choice::Outcome::Performed
        } else {
            game_choice::Outcome::Reconsider
        })
    })?;
    b.request::<Treatment>(|tx, r| {
        require(
            !tx.get::<Care>()?.ongoing.contains_key(&r.patient),
            "patient already has a caregiver",
        )?;
        if tx
            .attempt(game_health::Treat {
                caregiver: r.caregiver,
                patient: r.patient,
                supplies: r.supplies,
            })
            .is_err()
        {
            let _ = tx.attempt(game_knowledge::ObserveStore {
                person: r.caregiver,
                inventory: r.supplies,
            });
            return Ok(game_choice::Outcome::Reconsider);
        }
        tx.edit::<Care>()?.ongoing.insert(r.patient, r.caregiver);
        tx.edit::<Care>()?.helping.insert(r.caregiver, r.patient);
        Ok(game_choice::Outcome::Performed)
    })?;
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
        let injury = injury(tx, r.patient)?;
        let now = tx.now();
        if tx
            .get::<Care>()?
            .received
            .get(&r.caregiver)
            .and_then(|row| row.get(&r.patient))
            == Some(&(now, injury))
        {
            return Ok(false);
        }
        let spoken = game_vitality::ability(tx, r.patient, "vocalization") > 0
            && game_vitality::ability(tx, r.caregiver, "hearing") > 0;
        let shown = game_vitality::ability(tx, r.patient, "gesturing") > 0
            && game_vitality::ability(tx, r.caregiver, "interpretation") > 0;
        require(spoken || shown, "no available channel for requesting aid")?;
        let care = tx.edit::<Care>()?;
        care.received
            .entry(r.caregiver)
            .or_default()
            .insert(r.patient, (now, injury));
        care.requests.insert(r.patient, r.caregiver);
        care.sought
            .entry(r.patient)
            .or_default()
            .insert(r.caregiver, (now, injury));
        if tx.get::<game_characters::Minds>().is_ok_and(|m| m.running) {
            tx.notify(game_choice::Changed {
                person: r.caregiver,
            })?;
        } else {
            consider(tx, r.caregiver)?;
        }
        Ok(true)
    })?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    game_choice::provider(b, "care.help", |tx, e| {
        if game_choice::collecting(tx, e.person) {
            if !tx.get::<Care>()?.enabled {
                return Ok(());
            }
            let p = e.person;
            let Ok(area) = game_transport::area(tx.get()?, p) else {
                return Ok(());
            };
            let body = &tx.get::<game_physiology::Bodies>()?.people[&p];
            let injury = body
                .maximum(&tx.get::<game_physiology::Bodies>()?.rules)
                .saturating_sub(body.health)
                .min(100);
            let helpers: Vec<_> = tx
                .get::<game_social_life::SocialLife>()?
                .residents
                .get(&area)
                .into_iter()
                .flat_map(|r| {
                    r.scan_after(
                        tx.get::<Care>()
                            .unwrap()
                            .request_cursor
                            .get(&p)
                            .or(Some(&p)),
                    )
                    .chain(r.iter())
                    .take(r.len().min(16))
                })
                .map(|(q, _)| *q)
                .filter(|q| {
                    *q != p && Some(*q) != tx.get::<game_lineage::Lineage>().unwrap().current
                })
                .take(8)
                .collect();
            for helper in helpers {
                let previous = tx
                    .get::<Care>()?
                    .sought
                    .get(&p)
                    .and_then(|r| r.get(&helper));
                let need = previous.map_or(injury, |(at, old)| {
                    injury.saturating_sub(*old)
                        + injury * ((tx.now().0.saturating_sub(at.0) / HOUR.0).min(24)) / 24
                });
                if need > 0
                    && (game_vitality::ability(tx, p, "vocalization") > 0
                        || game_vitality::ability(tx, p, "gesturing") > 0)
                {
                    let regard = tx
                        .get::<game_relationships::Social>()?
                        .relationships
                        .get(&p)
                        .and_then(|r| r.get(&helper))
                        .map_or(0, |r| {
                            i64::from(
                                r.affection + r.trust + r.respect + r.admiration
                                    - r.fear
                                    - r.grievance,
                            )
                        })
                        .clamp(-75, 150);
                    let need = need as i64 * (100 + regard) / 100;
                    game_choice::offer(
                        tx,
                        p,
                        format!("aid:request:{}", helper.0),
                        game_choice::motive(need.min(1000), 0, 0, 2, 0),
                        game_choice::Undertake {
                            person: p,
                            kind: "request-aid".into(),
                            duration: MINUTE,
                            request: Pending::immediate(Appeal(RequestAid {
                                patient: p,
                                caregiver: helper,
                            })),
                        },
                    )?;
                }
            }
            let rules = tx.get::<game_health::TreatmentRules>()?;
            let supplies = rules.supply_object;
            let quantity = rules.supply;
            let inventories = known_supplies(tx, p, supplies, quantity, area);
            let peers: Vec<_> = std::iter::once(p)
                .chain(
                    tx.get::<game_social_life::SocialLife>()?
                        .residents
                        .get(&area)
                        .into_iter()
                        .flat_map(|r| r.keys())
                        .copied()
                        .filter(|q| *q != p),
                )
                .take(32)
                .collect();
            for patient in peers {
                let body = &tx.get::<game_physiology::Bodies>()?.people[&patient];
                let injury = body
                    .maximum(&tx.get::<game_physiology::Bodies>()?.rules)
                    .saturating_sub(body.health)
                    .min(100) as i64;
                if !body.alive || injury == 0 || tx.get::<Care>()?.ongoing.contains_key(&patient) {
                    continue;
                }
                let tie = tx
                    .get::<game_relationships::Social>()?
                    .relationships
                    .get(&p)
                    .and_then(|r| r.get(&patient))
                    .cloned()
                    .unwrap_or_default();
                let duty = tx.get::<game_relationships::Social>()?.opinion(p, "care");
                let mut concern = if patient == p {
                    injury * 3
                } else {
                    injury
                        * game_vitality::disposition(tx, p, "empathy")
                        * game_vitality::disposition(tx, p, "altruism")
                        / 500000
                        + i64::from(tie.affection - tie.grievance + duty * 2)
                };
                if patient != p && requested(tx, patient, p) && concern > 0 {
                    concern += concern / 2;
                }
                if inventories.is_empty()
                    && concern > 0
                    && tx.registered::<game_livelihoods::Supply>()
                {
                    let object = tx
                        .get::<game_objects::Objects>()?
                        .items
                        .iter()
                        .find(|(_, o)| o.id == supplies)
                        .map(|(key, _)| key.clone());
                    if let Some(object) = object {
                        game_choice::offer(
                            tx,
                            p,
                            format!("care:supplies:{}:{}", supplies.0, patient.0),
                            game_choice::motive(concern, 0, 0, 10, 0),
                            game_livelihoods::Supply {
                                person: p,
                                goal: game_livelihoods::Goal {
                                    object,
                                    quantity,
                                    priority: 100,
                                    affinity: "altruism".into(),
                                    concept: "care".into(),
                                },
                            },
                        )?;
                    }
                }
                for inventory in &inventories {
                    game_choice::offer(
                        tx,
                        p,
                        format!("care:treatment:{}:{}", patient.0, inventory.0),
                        game_choice::motive(concern, 0, 0, 5, 0),
                        Treatment {
                            caregiver: p,
                            patient,
                            supplies: *inventory,
                        },
                    )?;
                }
            }
            return Ok(());
        }
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
            s.ongoing.len() == s.helping.len()
                && s.helping.iter().all(|(carer, patient)| {
                    s.ongoing.get(patient) == Some(carer)
                        && h.get::<Actions>()
                            .is_ok_and(|a| a.0.get(carer).is_some_and(|w| w.kind == "treat"))
                }),
            "invalid care assignments",
        )
    });
    Ok(())
}
