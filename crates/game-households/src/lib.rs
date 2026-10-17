//! Voluntary families, dependent care, and age-appropriate enrollment.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod index;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "index::Saved")]
pub struct Households {
    pub enabled: bool,
    pub nursery: Table<PersonId, AreaId>,
    pub partners: Table<PersonId, PersonId>,
    pub children: Table<PersonId, Vec<PersonId>>,
    pub next: Table<PersonId, SimTime>,
    pub dependents: Table<PersonId, PersonId>,
    pub review: Option<ScheduleId>,
    #[serde(skip)]
    wards: Table<PersonId, Table<PersonId, ()>>,
}
impl Resource for Households {
    const KEY: &'static str = "households.kin";
}
#[derive(Clone, Serialize, Deserialize)]
struct ReviewDependents {
    after: Option<PersonId>,
}
impl Request for ReviewDependents {
    const KEY: &'static str = "households.review-dependents";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "households.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Form {
    pub first: PersonId,
    pub second: PersonId,
}
impl Request for Form {
    const KEY: &'static str = "households.form";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Formed {
    pub first: PersonId,
    pub second: PersonId,
}
impl Event for Formed {
    const KEY: &'static str = "households.formed";
}
fn willing(tx: &Transaction<'_>, p: PersonId, other: PersonId) -> Result<bool> {
    let population = tx.get::<game_lifecycle::Population>()?;
    let parents = |id| population.families.get(&id).map(|f| [f.mother, f.father]);
    if parents(p).is_some_and(|parents| parents.contains(&other))
        || parents(other).is_some_and(|parents| parents.contains(&p))
        || parents(p)
            .zip(parents(other))
            .is_some_and(|(a, b)| a.iter().any(|p| b.contains(p)))
    {
        return Ok(false);
    }
    let b = &tx.get::<game_physiology::Bodies>()?.people[&p];
    let age = tx.now().0.saturating_sub(b.born.0);
    let tie = tx
        .get::<game_relationships::Social>()?
        .relationships
        .get(&p)
        .and_then(|r| r.get(&other));
    let s = tx.get::<Households>()?;
    Ok(b.alive
        && b.kind == game_physiology::Kind::Mortal
        && age >= tx.get::<game_lifecycle::Population>()?.adult_age.0
        && s.partners.get(&p).is_none_or(|partner| *partner == other)
        && tie
            .is_some_and(|r| r.affection >= 30 && r.trust >= 6 && r.fear < 10 && r.grievance < 10))
}
fn support(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let dependents = tx
        .get::<Households>()?
        .wards
        .get(&person)
        .map_or(0, |s| s.len()) as u32;
    tx.call(game_provisioning::Support { person, dependents })
}
fn tend(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let _measurement = game_diagnostics::span("dependent care");
    let mut children: Vec<_> = tx
        .get::<Households>()?
        .wards
        .get(&person)
        .into_iter()
        .flat_map(|s| s.keys().copied())
        .collect();
    if let Ok(area) = game_transport::area(tx.get()?, person) {
        // A departure must not hide nearby dependents later in the iteration.
        children.sort_by_key(|child| {
            game_transport::area(tx.get().unwrap(), *child).ok() != Some(area)
        });
    }
    for child in children {
        let body = tx.get::<game_physiology::Bodies>()?.people[&child];
        if !body.alive {
            tx.edit::<Households>()?.assign(child, None);
            support(tx, person)?;
            continue;
        }
        if tx.now().0.saturating_sub(body.born.0) >= 6 * 365 * DAY.0 {
            if game_transport::area(tx.get()?, child).is_err() {
                continue;
            }
            if !tx
                .get::<game_characters::Minds>()?
                .people
                .contains_key(&child)
            {
                tx.call(game_characters::Register {
                    person: child,
                    mind: Default::default(),
                })?;
            }
            if tx.now().0.saturating_sub(body.born.0) >= 18 * 365 * DAY.0 {
                tx.edit::<Households>()?.assign(child, None);
                support(tx, person)?;
                continue;
            }
        }
        if game_physiology::capable(tx, person).is_err() {
            continue;
        }
        let Ok(area) = game_transport::area(tx.get()?, person) else {
            continue;
        };
        if game_transport::area(tx.get()?, child).ok() != Some(area) {
            if let Some(target) = tx.get::<Households>()?.nursery.get(&child).copied()
                && !tx.get::<Actions>()?.0.contains_key(&person)
                && tx
                    .get::<game_characters::Minds>()?
                    .people
                    .get(&person)
                    .is_none_or(|m| m.plan.is_none())
                && let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person)
                && let Ok(route) = game_knowledge::route(facts, area, target)
                && let Some(next) = route.areas.get(1)
            {
                let _ = tx.attempt(game_transport::Move {
                    person,
                    destination: *next,
                });
            }
            continue;
        }
        tx.edit::<Households>()?.nursery.insert(child, area);
        if body.fatigue > tx.get::<game_characters::Minds>()?.rules.rest_fatigue
            && !tx.get::<Actions>()?.0.contains_key(&child)
        {
            let _ = tx.attempt(game_physiology::Rest { person: child });
        }
        if body.hunger < tx.get::<game_characters::Minds>()?.rules.meal_hunger {
            continue;
        }
        let empathy = game_vitality::disposition(tx, person, "empathy");
        let mut stores: Vec<_> = {
            let claims = game_property::ClaimCosts::new(tx, person);
            let objects = tx.get::<game_objects::Objects>()?;
            let cargo = tx.get::<game_cargo::Cargo>()?;
            let positions = tx.get::<game_transport::Positions>()?;
            let known = if let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) {
                game_food_memory::with_stores(person, facts, objects, |memory| {
                    memory.among(
                        cargo
                            .food_stores(objects)
                            .into_iter()
                            .filter(|id| cargo.0[id].area(positions) == Some(area)),
                    )
                })
            } else {
                vec![]
            };
            known
                .into_iter()
                .chain(cargo.carried(person))
                .filter(|id| {
                    cargo.0.get(id).is_some_and(|i| {
                        i.area(positions) == Some(area) && objects.has_meal(&i.goods)
                    }) && claims.cost(*id) < body.hunger as i64 * 2 + empathy / 20
                })
                .collect()
        };
        // Compare the perceived consequences, then prefer one's own supplies.
        let claims = game_property::ClaimCosts::new(tx, person);
        let cargo = tx.get::<game_cargo::Cargo>()?;
        stores.sort_by_cached_key(|id| {
            (
                claims.cost(*id),
                !matches!(cargo.0[id].custody, game_cargo::Custody::Carried(p) if p == person),
                *id,
            )
        });
        stores.dedup();
        for inventory in stores {
            let result = tx.attempt(game_physiology::Feed {
                caregiver: person,
                patient: child,
                inventory,
            });
            game_diagnostics::failure(&result);
            if result.is_ok() {
                break;
            }
        }
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tend {
    pub person: PersonId,
}
impl Request for Tend {
    const KEY: &'static str = "households.tend";
    type Response = ();
}
/// Accept responsibility for a local dependent whose previous caregiver has died.
#[derive(Clone, Serialize, Deserialize)]
pub struct Foster {
    pub caregiver: PersonId,
    pub child: PersonId,
}
impl Request for Foster {
    const KEY: &'static str = "households.foster";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Fostered {
    pub caregiver: PersonId,
    pub child: PersonId,
}
impl Event for Fostered {
    const KEY: &'static str = "households.fostered";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("households", 1)?;
    b.resource::<Households>()?;
    b.request::<Foster>(|tx, r| {
        require(r.caregiver != r.child, "cannot care for oneself")?;
        game_physiology::capable(tx, r.caregiver)?;
        game_physiology::capable(tx, r.child)?;
        let bodies = tx.get::<game_physiology::Bodies>()?;
        require(
            tx.now()
                .0
                .saturating_sub(bodies.people[&r.caregiver].born.0)
                >= 18 * 365 * DAY.0,
            "caregiver is immature",
        )?;
        require(
            tx.get::<Households>()?
                .dependents
                .get(&r.child)
                .is_some_and(|p| bodies.people.get(p).is_some_and(|b| !b.alive)),
            "dependent already has a caregiver",
        )?;
        let area = game_transport::area(tx.get()?, r.caregiver)?;
        require(
            game_transport::area(tx.get()?, r.child)? == area,
            "dependent is elsewhere",
        )?;
        require(
            tx.now().0.saturating_sub(bodies.people[&r.child].born.0) < 18 * 365 * DAY.0,
            "dependent is already an adult",
        )?;
        let old = tx.edit::<Households>()?.assign(r.child, Some(r.caregiver));
        if let Some(old) = old {
            support(tx, old)?;
        }
        tx.edit::<Households>()?.nursery.insert(r.child, area);
        support(tx, r.caregiver)?;
        tx.emit(
            Fostered {
                caregiver: r.caregiver,
                child: r.child,
            },
            vec![r.caregiver, r.child],
        )
    })?;
    b.request::<Tend>(|tx, r| tend(tx, r.person))?;
    b.request::<ReviewDependents>(|tx, r| {
        let batch: Vec<_> = tx
            .get::<Households>()?
            .wards
            .scan_after(r.after.as_ref())
            .take(33)
            .map(|(p, _)| *p)
            .collect();
        for person in batch.iter().take(32).copied() {
            if Some(person) != tx.get::<game_lineage::Lineage>()?.current {
                tend(tx, person)?;
            }
        }
        let (at, after) = if batch.len() > 32 {
            (tx.now(), batch.get(31).copied())
        } else {
            (tx.now().after(SimDuration(12 * HOUR.0))?, None)
        };
        let job = tx.schedule(at, 40, ReviewDependents { after })?;
        tx.edit::<Households>()?.review = Some(job);
        Ok(())
    })?;
    b.request::<Enable>(|tx, _| {
        if !tx.get::<Households>()?.enabled {
            let job = tx.schedule(
                tx.now().after(SimDuration(12 * HOUR.0))?,
                40,
                ReviewDependents { after: None },
            )?;
            tx.edit::<Households>()?.review = Some(job);
        }
        tx.edit::<Households>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Form>(|tx, r| {
        require(r.first != r.second, "distinct partners required")?;
        if !willing(tx, r.first, r.second)? || !willing(tx, r.second, r.first)? {
            return Ok(false);
        }
        require(
            game_transport::area(tx.get()?, r.first)? == game_transport::area(tx.get()?, r.second)?,
            "partners elsewhere",
        )?;
        if tx.get::<Households>()?.partners.get(&r.first) == Some(&r.second) {
            return Ok(false);
        }
        let s = tx.edit::<Households>()?;
        s.partners.insert(r.first, r.second);
        s.partners.insert(r.second, r.first);
        tx.emit(
            Formed {
                first: r.first,
                second: r.second,
            },
            vec![r.first, r.second],
        )?;
        Ok(true)
    })?;
    b.subscribe::<game_social_life::Encounter>("households.affection", |tx, e| {
        if tx.get::<Households>()?.enabled
            && ![Some(e.first), Some(e.second)]
                .contains(&tx.get::<game_lineage::Lineage>()?.current)
        {
            tx.attempt(Form {
                first: e.first,
                second: e.second,
            })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_characters::IdleReview>("a.households.care", |tx, e| {
        if tx.get::<Households>()?.enabled
            && Some(e.person) != tx.get::<game_lineage::Lineage>()?.current
        {
            tend(tx, e.person)?;
        }
        Ok(())
    })?;
    b.subscribe::<game_lifecycle::Born>("b.households.enroll", |tx, e| {
        if !tx.get::<Households>()?.enabled {
            return Ok(());
        }
        let f = tx.get::<game_lifecycle::Population>()?.families[&e.person].clone();
        let s = tx.edit::<Households>()?;
        s.assign(e.person, Some(f.caregiver));
        s.nursery.insert(e.person, e.area);
        for p in [f.mother, f.father] {
            s.children.entry(p).or_default().push(e.person);
        }
        support(tx, f.caregiver)?;
        let t = tx.get::<game_traits::Traits>()?;
        let capacity =
            (500 + t.ability(e.person, "size") + t.ability(e.person, "strength") / 2).max(1200);
        let inventory = tx.call(game_cargo::Create {
            custody: game_cargo::Custody::Carried(e.person),
            capacity,
        })?;
        tx.call(game_property::ClaimInventory {
            claimant: e.person,
            inventory,
            recognized_by: std::collections::BTreeSet::from([e.person]),
        })?;
        tx.call(game_property::Permit {
            issuer: e.person,
            person: e.person,
            inventory,
            allow: true,
        })?;
        tx.call(game_knowledge::ObserveStore {
            person: e.person,
            inventory,
        })?;
        if tx.registered::<game_names::NameChild>() {
            tx.call(game_names::NameChild { child: e.person })?;
        } else {
            let name = tx
                .get::<game_social_life::SocialLife>()?
                .people
                .get(&f.caregiver)
                .map_or("Kin", |p| p.name.as_str())
                .to_owned();
            tx.call(game_social_life::Register {
                person: e.person,
                name,
            })?;
        }
        tx.call(game_relationships::Initialize {
            person: e.person,
            beliefs: Default::default(),
            values: Default::default(),
        })?;
        for p in [f.mother, f.father] {
            tx.call(game_relationships::Converse {
                observer: p,
                other: e.person,
                warmth: 10,
            })?;
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("households.locations", |tx, e| {
        let children: Vec<_> = tx
            .get::<Households>()?
            .wards
            .get(&e.person)
            .into_iter()
            .flat_map(|s| s.keys().copied())
            .collect();
        for child in children {
            if game_transport::area(tx.get()?, child).ok() == Some(e.area) {
                tx.edit::<Households>()?.nursery.insert(child, e.area);
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_transport::Departed>("households.carry", |tx, r| {
        let children: Vec<_> = tx
            .get::<Households>()?
            .wards
            .get(&r.person)
            .into_iter()
            .flat_map(|s| s.keys().copied())
            .collect();
        for child in children {
            if tx
                .get::<game_characters::Minds>()?
                .people
                .contains_key(&child)
            {
                continue;
            }
            if tx
                .get::<Actions>()?
                .0
                .get(&child)
                .is_some_and(|a| a.kind == "socialize" || a.kind == "rest")
            {
                tx.cancel_action(child)?;
            }
            if game_physiology::capable(tx, child).is_ok()
                && !tx.get::<Actions>()?.0.contains_key(&child)
            {
                let _ = tx.attempt(game_transport::Carry {
                    carrier: r.person,
                    passenger: child,
                    destination: r.destination,
                });
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("households.bereavement", |tx, e| {
        if let Some(carer) = tx.edit::<Households>()?.assign(e.person, None) {
            support(tx, carer)?;
        }
        if let Some(partner) = tx.edit::<Households>()?.partners.remove(&e.person) {
            tx.edit::<Households>()?.partners.remove(&partner);
            let children: Vec<_> = tx
                .get::<Households>()?
                .wards
                .get(&e.person)
                .into_iter()
                .flat_map(|s| s.keys().copied())
                .collect();
            for child in children {
                tx.edit::<Households>()?.assign(child, Some(partner));
            }
            support(tx, partner)?;
            support(tx, e.person)?;
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Households>()?;
        require(
            s.wards == index::build(&s.dependents),
            "invalid dependent index",
        )?;
        require(
            s.partners
                .iter()
                .all(|(p, o)| p != o && s.partners.get(o) == Some(p)),
            "asymmetric partnership",
        )?;
        require(
            s.dependents.iter().all(|(c, p)| {
                c != p
                    && h.get::<game_lifecycle::Population>()
                        .is_ok_and(|s| s.families.contains_key(c))
            }),
            "invalid dependent",
        )
    });
    Ok(())
}
