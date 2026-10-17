//! Voluntary journeys connect places through remembered roads and actual visits.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trip {
    pub home: AreaId,
    pub destination: AreaId,
    pub returning: bool,
    pub leave_at: Option<SimTime>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wayfaring {
    pub enabled: bool,
    pub trips: Table<PersonId, Trip>,
    pub next: Table<PersonId, SimTime>,
    pub visits: Table<PersonId, Table<AreaId, SimTime>>,
    pub completed: u64,
}
impl Resource for Wayfaring {
    const KEY: &'static str = "wayfaring.journeys";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "wayfaring.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consider {
    pub person: PersonId,
}
impl Request for Consider {
    const KEY: &'static str = "wayfaring.consider";
    type Response = bool;
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Visited {
    pub person: PersonId,
    pub area: AreaId,
}
impl Event for Visited {
    const KEY: &'static str = "wayfaring.visited";
}
fn consider(tx: &mut Transaction<'_>, person: PersonId) -> Result<bool> {
    let _measurement = game_diagnostics::span("journey planning");
    if !tx.get::<Wayfaring>()?.enabled
        || tx.get::<Actions>()?.0.contains_key(&person)
        || tx.get::<game_lineage::Lineage>()?.current == Some(person)
        || game_physiology::capable(tx, person).is_err()
    {
        return Ok(false);
    }
    let Ok(area) = game_transport::area(tx.get()?, person) else {
        return Ok(false);
    };
    if !tx.get::<Wayfaring>()?.trips.contains_key(&person) {
        let foresight = game_vitality::ability(tx, person, "foresight");
        let risk = game_vitality::disposition(tx, person, "risk-tolerance");
        let local_source = tx
            .get::<game_provisioning::Reserves>()
            .ok()
            .and_then(|r| r.sources.get(&person))
            .is_some_and(|sources| {
                sources.iter().any(|(patch, experience)| {
                    experience.successes > experience.failures
                        && tx
                            .get::<game_knowledge::Knowledge>()
                            .ok()
                            .and_then(|k| k.0.get(&person))
                            .and_then(|facts| {
                                game_knowledge::known::<game_ecology::Patch>(
                                    facts,
                                    &game_knowledge::patch_key(*patch),
                                )
                            })
                            .is_some_and(|p| p.area == area && p.stock > 0)
                })
            });
        if foresight >= 1000 && risk < 850 && local_source {
            return Ok(false);
        }

        if tx.get::<game_livelihoods::Livelihoods>()?.has_plan(person)
            || tx
                .get::<Wayfaring>()?
                .next
                .get(&person)
                .is_some_and(|at| *at > tx.now())
            || game_vitality::disposition(tx, person, "risk-tolerance") < 650
        {
            return Ok(false);
        }
        let now = tx.now();
        tx.edit::<Wayfaring>()?
            .visits
            .entry(person)
            .or_default()
            .entry(area)
            .or_insert_with(|| now);
        let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
            return Ok(false);
        };
        let mut candidates = game_provisioning::frontiers(facts);
        if candidates.is_empty() {
            candidates = game_knowledge::with_prefix(facts, "roads:")
                .take(128)
                .filter_map(|(key, _)| key.strip_prefix("roads:")?.parse::<u64>().ok().map(AreaId))
                .filter(|a| {
                    *a != area
                        && tx
                            .get::<Wayfaring>()
                            .unwrap()
                            .visits
                            .get(&person)
                            .and_then(|v| v.get(a))
                            .is_none_or(|at| now.0.saturating_sub(at.0) >= 7 * DAY.0)
                })
                .collect();
        }
        let target = candidates
            .into_iter()
            .filter_map(|to| {
                game_knowledge::route(facts, area, to)
                    .ok()
                    .map(|r| (r.duration, to))
            })
            .min()
            .map(|(_, to)| to);
        let next = now.after(SimDuration(2 * DAY.0))?;
        tx.edit::<Wayfaring>()?.next.insert(person, next);
        let Some(destination) = target else {
            return Ok(false);
        };
        tx.edit::<Wayfaring>()?.trips.insert(
            person,
            Trip {
                home: area,
                destination,
                returning: false,
                leave_at: None,
            },
        );
    }
    let mut trip = tx.get::<Wayfaring>()?.trips[&person].clone();
    if area == trip.destination && !trip.returning {
        let Some(until) = trip.leave_at else {
            return Ok(false);
        };
        if tx.now() < until {
            return Ok(false);
        }
        trip.returning = true;
        tx.edit::<Wayfaring>()?.trips.insert(person, trip.clone());
    }
    let target = if trip.returning {
        trip.home
    } else {
        trip.destination
    };
    if area == target {
        tx.edit::<Wayfaring>()?.trips.remove(&person);
        let next = tx.now().after(SimDuration(2 * DAY.0))?;
        let s = tx.edit::<Wayfaring>()?;
        s.next.insert(person, next);
        s.completed = s.completed.saturating_add(1);
        return Ok(false);
    }
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
        return Ok(false);
    };
    let Ok(route) = game_knowledge::route(facts, area, target) else {
        tx.edit::<Wayfaring>()?.trips.remove(&person);
        return Ok(false);
    };
    if let Some(next) = route.areas.get(1) {
        return Ok(tx
            .attempt(game_transport::Move {
                person,
                destination: *next,
            })
            .is_ok());
    }
    Ok(false)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("wayfaring", 1)?;
    b.resource::<Wayfaring>()?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Wayfaring>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Consider>(|tx, r| consider(tx, r.person))?;
    b.subscribe::<game_characters::IdleReview>("journeys.explore", |tx, e| {
        tx.call(Consider { person: e.person })?;
        Ok(())
    })?;
    b.subscribe::<game_transport::Arrived>("wayfaring.arrived", |tx, e| {
        if !tx.get::<Wayfaring>()?.enabled {
            return Ok(());
        }
        tx.call(game_knowledge::ObserveSurroundings { person: e.person })?;
        let now = tx.now();
        let first = tx
            .edit::<Wayfaring>()?
            .visits
            .entry(e.person)
            .or_default()
            .insert(e.area, now)
            .is_none();
        if first {
            tx.call(game_relationships::LearnValue {
                person: e.person,
                concept: "discovery".into(),
                outcome: 1,
            })?;
            tx.emit(
                Visited {
                    person: e.person,
                    area: e.area,
                },
                vec![e.person],
            )?;
        }
        let until = now.after(SimDuration(4 * HOUR.0))?;
        if let Some(t) = tx.edit::<Wayfaring>()?.trips.get_mut(&e.person)
            && !t.returning
            && e.area == t.destination
        {
            t.leave_at = Some(until);
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("wayfaring.death", |tx, e| {
        let s = tx.edit::<Wayfaring>()?;
        s.trips.remove(&e.person);
        s.next.remove(&e.person);
        Ok(())
    })?;
    b.validate_save(|h| {
        for (p, t) in &h.get::<Wayfaring>()?.trips {
            require(
                h.get::<game_physiology::Bodies>()?
                    .people
                    .get(p)
                    .is_some_and(|b| b.alive)
                    && t.home != t.destination
                    && h.get::<game_knowledge::Knowledge>()?.0.contains_key(p),
                "invalid journey",
            )?;
        }
        Ok(())
    });
    Ok(())
}
