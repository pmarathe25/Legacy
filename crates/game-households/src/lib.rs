//! Voluntary families, dependent care, and age-appropriate enrollment.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod care;
pub use care::{CareChanged, CommitCare};
pub use game_household_state::{
    ClearNewFoodUrgency, ClearObservedFoodAction, ConfigureFoodRefusals, ExpireFoodRefusals,
    FoodRefusal, Households, RefusalGroup, RememberFoodRefusal, SetRefusalRetry,
};
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
    let children: Vec<_> = tx
        .get::<Households>()?
        .wards
        .get(&person)
        .into_iter()
        .flat_map(|s| s.keys().copied())
        .collect();
    for child in children {
        let body = tx.get::<game_physiology::Bodies>()?.people[&child];
        if !body.alive {
            tx.edit::<Households>()?.set_care(person, child, false);
            support(tx, person)?;
            continue;
        }
        if game_physiology::capable(tx, child).is_ok() {
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
        }
        if game_physiology::capable(tx, person).is_err() {
            continue;
        }
        let Ok(area) = game_transport::area(tx.get()?, person) else {
            continue;
        };
        if game_transport::area(tx.get()?, child).ok() != Some(area) {
            continue;
        }
        tx.edit::<Households>()?.nursery.insert(child, area);
    }
    if tx.registered::<TendFood>() {
        if tx.get::<game_characters::Minds>()?.running
            && tx
                .get::<game_characters::Minds>()?
                .people
                .contains_key(&person)
        {
            return tx.notify(game_choice::Changed { person });
        }
        tx.call(TendFood { caregiver: person })?;
    }
    Ok(())
}
pub use game_provisioning::{FoodCare, TendFood};
#[derive(Clone, Serialize, Deserialize)]
pub struct RememberFoodCare {
    pub caregiver: PersonId,
    pub intent: Option<FoodCare>,
}
impl Request for RememberFoodCare {
    const KEY: &'static str = "households.food-intent";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tend {
    pub person: PersonId,
}
impl Request for Tend {
    const KEY: &'static str = "households.tend";
    type Response = ();
}
/// Freely offer care; another person's existing commitment does not exclude help.
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
#[derive(Clone, Serialize, Deserialize)]
pub struct Separate {
    pub person: PersonId,
    pub other: PersonId,
}
impl Request for Separate {
    const KEY: &'static str = "households.separate";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Independent {
    pub person: PersonId,
}
impl Request for Independent {
    const KEY: &'static str = "households.independent";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("households", 2)?;
    b.resource::<Households>()?;
    game_household_state::register_refusals(b)?;
    b.request::<Separate>(|tx, r| {
        if tx.get::<Households>()?.partners.get(&r.person) == Some(&r.other) {
            tx.edit::<Households>()?.partners.remove(&r.person);
            tx.edit::<Households>()?.partners.remove(&r.other);
        }
        Ok(())
    })?;
    b.request::<Independent>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            game_vitality::can_walk(tx, r.person),
            "cannot travel independently",
        )?;
        // A personal declaration grants no power to rewrite another person's intentions.
        tx.edit::<Households>()?.self_reliant.insert(r.person, ());
        Ok(())
    })?;
    b.request::<RememberFoodCare>(|tx, r| {
        if let Some(intent) = &r.intent {
            require(
                intent.valid_for(r.caregiver) && intent.observed_at <= tx.now(),
                "invalid care intention",
            )?;
            let continuing = tx
                .get::<Households>()?
                .food_care
                .get(&r.caregiver)
                .is_some_and(|old| old.beneficiary == intent.beneficiary);
            if !continuing {
                game_physiology::capable(tx, r.caregiver)?;
                require(
                    game_transport::area(tx.get()?, r.caregiver)
                        .ok()
                        .is_some_and(|area| {
                            game_transport::area(tx.get().unwrap(), intent.beneficiary).ok()
                                == Some(area)
                        })
                        || game_signals::perception(tx, r.caregiver, intent.beneficiary)
                            .is_some_and(|p| p.area == Some(intent.home))
                            && game_signals::estimated_hunger(tx, r.caregiver, intent.beneficiary)
                                .is_some(),
                    "care need has no personally observed location",
                )?;
            }
        }
        if let Some(intent) = r.intent {
            tx.edit::<Households>()?
                .food_care
                .insert(r.caregiver, intent);
        } else {
            tx.edit::<Households>()?.food_care.remove(&r.caregiver);
        }
        Ok(())
    })?;
    care::register(b)?;
    b.request::<Foster>(|tx, r| {
        tx.call(CommitCare {
            caregiver: r.caregiver,
            beneficiary: r.child,
            accept: true,
        })?;
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
        if !tx.get::<game_characters::Minds>()?.running
            && tx.get::<Households>()?.enabled
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
    game_choice::provider(b, "a.households.care", |tx, e| {
        if !game_choice::collecting(tx, e.person)
            && tx.get::<Households>()?.enabled
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
        s.nursery.insert(e.person, e.area);
        for p in [f.mother, f.father] {
            s.children.entry(p).or_default().push(e.person);
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
    b.subscribe::<game_physiology::Died>("households.bereavement", |tx, e| {
        game_household_state::forget_person(tx, e.person)?;
        let carers: Vec<_> = tx.get::<Households>()?.carers_of(e.person).collect();
        let beneficiaries: Vec<_> = tx.get::<Households>()?.beneficiaries_of(e.person).collect();
        for carer in carers {
            tx.call(CommitCare {
                caregiver: carer,
                beneficiary: e.person,
                accept: false,
            })?;
        }
        for beneficiary in beneficiaries {
            tx.call(CommitCare {
                caregiver: e.person,
                beneficiary,
                accept: false,
            })?;
        }
        if let Some(partner) = tx.edit::<Households>()?.partners.remove(&e.person) {
            tx.edit::<Households>()?.partners.remove(&partner);
        }
        Ok(())
    })?;
    b.validate_save(|h| {
        let s = h.get::<Households>()?;
        require(s.wards_valid(), "invalid dependent index")?;
        require(
            s.partners
                .iter()
                .all(|(p, o)| p != o && s.partners.get(o) == Some(p)),
            "asymmetric partnership",
        )?;
        require(
            s.caregivers.iter().all(|(person, carers)| {
                !carers.is_empty() && carers.keys().all(|carer| person != carer)
            }),
            "invalid dependent",
        )
    });
    Ok(())
}
