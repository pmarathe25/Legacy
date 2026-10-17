//! Claims have consequences only when particular witnesses notice and choose a response.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grievances {
    pub enabled: bool,
    pub noticed: Table<PersonId, Table<PersonId, ()>>,
    pub forgiven: u64,
}
impl Resource for Grievances {
    const KEY: &'static str = "grievances.noticed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "grievances.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Response {
    pub observer: PersonId,
    pub actor: PersonId,
    pub inventory: InventoryId,
    pub forgiven: bool,
}
impl Event for Response {
    const KEY: &'static str = "grievances.response";
}
fn notice(tx: &mut Transaction<'_>, e: &game_property::Interfered) -> Result<()> {
    if !tx.get::<Grievances>()?.enabled {
        return Ok(());
    }
    let claims = tx
        .get::<game_property::Property>()?
        .claims
        .get(&e.inventory)
        .cloned()
        .unwrap_or_default();
    for witness in &e.witnesses {
        if *witness == e.actor
            || !claims
                .iter()
                .any(|c| c.claimant == *witness || c.recognized_by.contains(witness))
        {
            continue;
        }
        let empathy = game_vitality::disposition(tx, *witness, "empathy");
        let actor = tx.get::<game_physiology::Bodies>()?.people.get(&e.actor);
        let necessary = ["eating", "feeding", "treatment"].contains(&e.purpose.as_str())
            && actor.is_some_and(|b| b.hunger >= 30 || b.health < 50);
        let forgiven = necessary && empathy >= 650;
        if forgiven {
            tx.edit::<Grievances>()?.forgiven += 1;
        } else {
            tx.call(game_relationships::Interpret {
                observer: *witness,
                actor: e.actor,
                shared: false,
                harmed: true,
                credible: true,
            })?;
            tx.edit::<Grievances>()?
                .noticed
                .entry(*witness)
                .or_default()
                .insert(e.actor, ());
        }
        tx.emit(
            Response {
                observer: *witness,
                actor: e.actor,
                inventory: e.inventory,
                forgiven,
            },
            vec![*witness, e.actor],
        )?;
    }
    Ok(())
}
fn respond(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let _measurement = game_diagnostics::span("grievance planning");
    if !tx.get::<Grievances>()?.enabled
        || Some(person) == tx.get::<game_lineage::Lineage>()?.current
        || tx.get::<Actions>()?.0.contains_key(&person)
    {
        return Ok(());
    }
    let Ok(area) = game_transport::area(tx.get()?, person) else {
        return Ok(());
    };
    let Some(known) = tx.get::<Grievances>()?.noticed.get(&person) else {
        return Ok(());
    };
    let aggression = game_vitality::disposition(tx, person, "aggression");
    let patience = game_vitality::disposition(tx, person, "patience");
    let target = known
        .keys()
        .copied()
        .filter(|p| {
            Some(*p) != tx.get::<game_lineage::Lineage>().unwrap().current
                && game_transport::area(tx.get().unwrap(), *p).ok() == Some(area)
                && game_physiology::capable(tx, *p).is_ok()
                && !tx.get::<Actions>().unwrap().0.contains_key(p)
        })
        .filter_map(|p| {
            let r = tx
                .get::<game_relationships::Social>()
                .ok()?
                .relationships
                .get(&person)?
                .get(&p)?;
            Some((
                i64::from(r.grievance) * 10 + aggression / 5 - patience / 5 - i64::from(r.fear) * 4,
                p,
            ))
        })
        .max();
    if let Some((score, subject)) = target
        && score > 150
        && tx
            .attempt(game_pressure::DemandSubmission {
                leader: person,
                subject,
            })
            .is_ok()
    {
        tx.edit::<Grievances>()?
            .noticed
            .get_mut(&person)
            .unwrap()
            .remove(&subject);
    }
    Ok(())
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("grievances", 1)?;
    b.resource::<Grievances>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Grievances>()?.enabled = true;
        Ok(())
    })?;
    b.subscribe::<game_property::Interfered>("grievances.witness", notice)?;
    b.subscribe::<game_agriculture::Encroached>("grievances.land", |tx, e| {
        if tx.get::<Grievances>()?.enabled && e.witnesses.contains(&e.steward) {
            let shared = e.purpose == "sowing";
            tx.call(game_relationships::Interpret {
                observer: e.steward,
                actor: e.person,
                shared,
                harmed: !shared,
                credible: true,
            })?;
            if !shared {
                tx.edit::<Grievances>()?
                    .noticed
                    .entry(e.steward)
                    .or_default()
                    .insert(e.person, ());
            }
        }
        Ok(())
    })?;
    b.subscribe::<game_characters::IdleReview>("disputes.responses", |tx, e| {
        respond(tx, e.person)
    })?;
    Ok(())
}
