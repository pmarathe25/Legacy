//! Local exposure and perception. Spatial, bodily and tactical facts are
//! transient inputs; only the health owner's observations are persisted.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HealthActivityFacts {
    pub combat: bool,
    pub cultivation: bool,
    pub gathering: bool,
    pub construction: bool,
    pub play: bool,
}
impl HealthActivityFacts {
    fn outside_shelter(self) -> bool {
        self.combat || self.cultivation || self.gathering || self.construction || self.play
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TacticalIdentity {
    pub encounter: EncounterId,
    pub anchor: PersonId,
}
/// `place` and `physical_area` describe the resolved spatial point. An actor on
/// a road has no physical area even if its weather cell is the departure area.
#[derive(Clone, Debug)]
pub struct LocalHealthActor<K> {
    pub actor: PersonId,
    pub place: Option<K>,
    pub physical_area: Option<AreaId>,
    pub carrier: Option<PersonId>,
    pub tactical: Option<TacticalIdentity>,
    pub alive: bool,
    pub moving: bool,
    pub activity: HealthActivityFacts,
    pub health: u64,
    pub max_health: u64,
    pub temperature_c: Option<i32>,
}
/// An actual interaction supplied by another feature, not a relationship,
/// intention, conversation offer, or another person's private knowledge.
#[derive(Clone, Copy, Debug)]
pub struct HealthInteraction {
    pub first: PersonId,
    pub second: PersonId,
}
/// Geometry reports reach and visibility; health decides their relevance.
#[derive(Clone, Copy, Debug)]
pub struct HealthGeometry {
    pub first: TacticalIdentity,
    pub second: TacticalIdentity,
    pub contact_clear: bool,
    pub sight_clear: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Cohort<K> {
    place: K,
    shelter: Option<ShelterId>,
    tactical: Option<TacticalIdentity>,
}
struct LocalIndex<'a, K> {
    actors: BTreeMap<PersonId, &'a LocalHealthActor<K>>,
    cohorts: BTreeMap<PersonId, Cohort<K>>,
    geometry: BTreeMap<(TacticalIdentity, TacticalIdentity), (bool, bool)>,
}
fn ordered<T: Ord + Copy>(a: T, b: T) -> (T, T) {
    if a <= b { (a, b) } else { (b, a) }
}
fn invalid(message: &str) -> Error {
    Error::InvalidSave(message.into())
}
impl<'a, K: Clone + Ord> LocalIndex<'a, K> {
    fn build(
        store: &Store,
        actors: &'a [LocalHealthActor<K>],
        geometry: &[HealthGeometry],
    ) -> Result<Self, Error> {
        let mut facts = BTreeMap::new();
        for actor in actors {
            if actor.actor.0 == 0
                || actor.max_health == 0
                || actor.health > actor.max_health
                || actor.physical_area.is_some_and(|area| area.0 == 0)
                || actor
                    .tactical
                    .is_some_and(|point| point.encounter.0 == 0 || point.anchor.0 == 0)
                || facts.insert(actor.actor, actor).is_some()
            {
                return Err(invalid("invalid or repeated local health actor"));
            }
        }
        let mut clear = BTreeMap::new();
        for hint in geometry {
            let key = ordered(hint.first, hint.second);
            let value = (hint.contact_clear, hint.sight_clear);
            if (key.0.encounter != key.1.encounter && (value.0 || value.1))
                || key.0.encounter.0 == 0
                || key.1.encounter.0 == 0
                || key.0.anchor.0 == 0
                || key.1.anchor.0 == 0
                || clear.insert(key, value).is_some_and(|old| old != value)
            {
                return Err(invalid("inconsistent local health geometry"));
            }
        }
        // Shelter inheritance follows the physical carrier chain. No access or
        // occupancy is granted to a held body merely by being sheltered.
        let mut shelters = BTreeMap::<PersonId, Option<ShelterId>>::new();
        let mut tactical = BTreeMap::<PersonId, Option<TacticalIdentity>>::new();
        for id in facts.keys() {
            let mut current = *id;
            let mut chain = Vec::new();
            let mut seen = BTreeSet::new();
            let mut effective = loop {
                if let Some(shelter) = shelters.get(&current) {
                    break *shelter;
                }
                if !seen.insert(current) {
                    return Err(invalid("cyclic local health carrying"));
                }
                let actor = facts
                    .get(&current)
                    .ok_or_else(|| invalid("missing local health carrier"))?;
                if let Some(carrier) = actor.carrier {
                    chain.push(current);
                    current = carrier;
                } else {
                    let effective = occupancy(store, current)
                        .and_then(|row| shelter(store, row.shelter))
                        .filter(|shelter| {
                            actor.alive
                                && !actor.activity.outside_shelter()
                                && actor.physical_area == Some(shelter.area)
                        })
                        .map(|shelter| shelter.id);
                    shelters.insert(current, effective);
                    tactical.insert(current, actor.tactical);
                    break effective;
                }
            };
            let anchor_tactical = tactical[&current];
            for carried in chain.into_iter().rev() {
                if !facts[&carried].alive {
                    effective = None;
                }
                shelters.insert(carried, effective);
                tactical.insert(carried, anchor_tactical);
            }
        }
        let cohorts = facts
            .iter()
            .filter_map(|(id, actor)| {
                Some((
                    *id,
                    Cohort {
                        place: actor.place.clone()?,
                        shelter: shelters[id],
                        tactical: tactical[id],
                    },
                ))
            })
            .collect();
        Ok(Self {
            actors: facts,
            cohorts,
            geometry: clear,
        })
    }
    fn clear(
        &self,
        a: Option<TacticalIdentity>,
        b: Option<TacticalIdentity>,
        contact: bool,
    ) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) if a == b => true,
            (Some(a), Some(b)) if a.encounter == b.encounter => self
                .geometry
                .get(&ordered(a, b))
                .is_some_and(|(reach, sight)| if contact { *reach } else { *sight }),
            _ => false,
        }
    }
    fn contact(&self, a: PersonId, b: PersonId, treatment: bool) -> bool {
        if a == b {
            return false;
        }
        let (Some(a), Some(b)) = (self.cohorts.get(&a), self.cohorts.get(&b)) else {
            return false;
        };
        a.place == b.place
            && self.clear(a.tactical, b.tactical, true)
            && (treatment || a.shelter == b.shelter)
    }
    fn visible(&self, a: &Cohort<K>, b: &Cohort<K>) -> bool {
        a.place == b.place && a.shelter == b.shelter && self.clear(a.tactical, b.tactical, false)
    }
}
fn infectious(store: &Store, actor: PersonId, at: SimTime) -> [u64; 2] {
    [IllnessKind::Enteric, IllnessKind::Respiratory].map(|kind| {
        u64::from(
            clinical(store, actor)
                .and_then(|row| row.illnesses.get(&kind))
                .is_some_and(|illness| illness.symptomatic && illness.recovers > at),
        )
    })
}
fn symptoms(
    store: &Store,
    rules: &Rules,
    actor: &LocalHealthActor<impl Clone>,
    at: SimTime,
) -> SymptomsView {
    let state = clinical(store, actor.actor);
    let [enteric, respiratory] = infectious(store, actor.actor, at);
    SymptomsView {
        person: actor.actor,
        fever: enteric > 0 || respiratory > 0,
        nausea: enteric > 0,
        cough: respiratory > 0,
        shivering: state.is_some_and(|state| state.cold >= rules.cold_harm / 2),
        wounded: actor.health < actor.max_health / 2,
        observed_at: at,
    }
}
impl Runtime {
    pub fn local_exposures<K: Clone + Ord>(
        &self,
        store: &Store,
        at: SimTime,
        actors: &[LocalHealthActor<K>],
        interactions: &[HealthInteraction],
        geometry: &[HealthGeometry],
    ) -> Result<BTreeMap<PersonId, ExposureInput>, Error> {
        let index = LocalIndex::build(store, actors, geometry)?;
        let infectious: BTreeMap<_, _> = index
            .actors
            .keys()
            .map(|actor| (*actor, infectious(store, *actor, at)))
            .collect();
        let mut shared = BTreeMap::<ShelterId, [u64; 2]>::new();
        for (actor, cohort) in &index.cohorts {
            if let Some(shelter) = cohort.shelter {
                let total = shared.entry(shelter).or_default();
                for (sum, flag) in total.iter_mut().zip(infectious[actor]) {
                    *sum = sum.checked_add(flag).ok_or(Error::Overflow)?;
                }
            }
        }
        let mut exposures: BTreeMap<_, _> = index
            .actors
            .iter()
            .map(|(actor, facts)| {
                let effective = index.cohorts.get(actor).and_then(|row| row.shelter);
                let mut contacts = effective.map_or([0; 2], |shelter| shared[&shelter]);
                if effective.is_some() {
                    for (count, own) in contacts.iter_mut().zip(infectious[actor]) {
                        *count -= own;
                    }
                }
                (
                    *actor,
                    ExposureInput {
                        temperature_c: facts.temperature_c,
                        insulation: effective
                            .and_then(|id| shelter(store, id))
                            .map_or(0, |row| row.insulation),
                        infectious_contacts: contacts,
                    },
                )
            })
            .collect();
        let mut pairs = BTreeSet::new();
        let mut add_pair = |a, b, treatment| {
            if index.contact(a, b, treatment) {
                let shared_shelter = index.cohorts[&a].shelter;
                if shared_shelter.is_none() || shared_shelter != index.cohorts[&b].shelter {
                    pairs.insert(ordered(a, b));
                }
            }
        };
        for (_, job) in jobs(store) {
            if let Work::Treat { patient, .. } = job.work {
                add_pair(job.actor, patient, true);
            }
        }
        for hint in interactions {
            add_pair(hint.first, hint.second, false);
        }
        for actor in actors {
            if let Some(carrier) = actor.carrier {
                add_pair(actor.actor, carrier, true);
            }
        }
        for (a, b) in pairs {
            for (receiver, source) in [(a, b), (b, a)] {
                for (sum, flag) in exposures
                    .get_mut(&receiver)
                    .expect("local pair actor")
                    .infectious_contacts
                    .iter_mut()
                    .zip(infectious[&source])
                {
                    *sum = sum.checked_add(flag).ok_or(Error::Overflow)?;
                }
            }
        }
        Ok(exposures)
    }
}

#[derive(Clone)]
struct ObservationDelta {
    observer: PersonId,
    before_people: BTreeMap<PersonId, SymptomsView>,
    before_shelters: BTreeMap<ShelterId, ShelterView>,
    people: Vec<(PersonId, SymptomsView)>,
    shelters: Vec<(ShelterId, ShelterView)>,
}
/// Prepared observation changes only known symptoms and shelters. Water
/// knowledge accumulated by other health operations is retained at commit.
pub struct PreparedLocalObservation {
    rules: Rules,
    clinical: Vec<(PersonId, Option<Clinical>)>,
    occupancy: Vec<(PersonId, Option<Occupancy>)>,
    shelters: BTreeMap<ShelterId, Option<Shelter>>,
    visible_shelters: Vec<ShelterId>,
    observed_areas: BTreeSet<AreaId>,
    deltas: Vec<ObservationDelta>,
}
impl PreparedLocalObservation {
    pub fn observers(&self) -> usize {
        self.deltas.len()
    }
    pub fn commit(self, runtime: &Runtime, store: &mut Store) -> Result<(), Error> {
        if runtime.rules != self.rules
            || self
                .clinical
                .iter()
                .any(|(actor, before)| clinical(store, *actor) != before.as_ref())
            || self
                .occupancy
                .iter()
                .any(|(actor, before)| occupancy(store, *actor) != before.as_ref())
            || shelters(store)
                .filter(|(_, shelter)| self.observed_areas.contains(&shelter.area))
                .map(|(id, _)| id)
                .ne(self.visible_shelters.iter().copied())
            || self
                .shelters
                .iter()
                .any(|(id, before)| shelter(store, *id) != before.as_ref())
        {
            return Err(invalid("stale local health observation"));
        }
        let mut batch = Batch::new();
        for delta in self.deltas {
            let mut current = knowledge(store, delta.observer)
                .cloned()
                .unwrap_or_default();
            if current.people != delta.before_people || current.shelters != delta.before_shelters {
                return Err(invalid("stale personal health observation"));
            }
            current.people.extend(delta.people);
            current.shelters.extend(delta.shelters);
            current.validate()?;
            batch.insert(StableId(delta.observer.0), current);
        }
        store.apply_batch(batch)
    }
}
impl Runtime {
    pub fn prepare_local_observations<K: Clone + Ord>(
        &self,
        store: &Store,
        at: SimTime,
        actors: &[LocalHealthActor<K>],
        geometry: &[HealthGeometry],
    ) -> Result<PreparedLocalObservation, Error> {
        let index = LocalIndex::build(store, actors, geometry)?;
        let mut observed_areas = BTreeSet::new();
        let mut occupants = BTreeMap::<(ShelterId, Cohort<K>), BTreeSet<PersonId>>::new();
        for (actor, cohort) in &index.cohorts {
            let shelter = if index.actors[actor].carrier.is_some() {
                cohort.shelter
            } else {
                occupancy(store, *actor).map(|row| row.shelter)
            };
            if let Some(shelter) = shelter {
                occupants
                    .entry((shelter, cohort.clone()))
                    .or_default()
                    .insert(*actor);
            }
        }
        let mut visible = BTreeMap::<Cohort<K>, Vec<(PersonId, SymptomsView)>>::new();
        for cohort in index.cohorts.values() {
            if visible.contains_key(cohort) {
                continue;
            }
            let people = index
                .cohorts
                .iter()
                .filter(|(_, subject)| index.visible(cohort, subject))
                .take(64)
                .map(|(actor, _)| {
                    (
                        *actor,
                        symptoms(store, &self.rules, index.actors[actor], at),
                    )
                })
                .collect();
            visible.insert(cohort.clone(), people);
        }
        let mut deltas = Vec::new();
        for (observer, facts) in &index.actors {
            let Some(cohort) = index
                .cohorts
                .get(observer)
                .filter(|_| facts.alive && !facts.moving)
            else {
                continue;
            };
            if !store.contains(StableId(observer.0)) {
                return Err(invalid("missing local health observer entity"));
            }
            let area = facts.physical_area.filter(|_| cohort.tactical.is_none());
            if let Some(area) = area {
                observed_areas.insert(area);
            }
            let shelters = area
                .into_iter()
                .flat_map(|area| shelters(store).filter(move |(_, row)| row.area == area))
                .map(|(id, row)| {
                    (
                        id,
                        ShelterView {
                            id,
                            area: row.area,
                            capacity: row.capacity,
                            insulation: row.insulation,
                            custodian: row.custodian,
                            accessible: row.access.contains(observer),
                            known_occupants: occupants
                                .get(&(id, cohort.clone()))
                                .cloned()
                                .unwrap_or_default(),
                            observed_at: at,
                        },
                    )
                })
                .collect();
            let before = knowledge(store, *observer);
            deltas.push(ObservationDelta {
                observer: *observer,
                before_people: before.map_or_else(BTreeMap::new, |row| row.people.clone()),
                before_shelters: before.map_or_else(BTreeMap::new, |row| row.shelters.clone()),
                people: visible[cohort].clone(),
                shelters,
            });
        }
        let visible_shelters: Vec<_> = shelters(store)
            .filter(|(_, row)| observed_areas.contains(&row.area))
            .map(|(id, _)| id)
            .collect();
        let mut consulted: BTreeSet<_> = visible_shelters.iter().copied().collect();
        for actor in index.actors.keys() {
            if let Some(row) = occupancy(store, *actor) {
                consulted.insert(row.shelter);
            }
        }
        Ok(PreparedLocalObservation {
            rules: self.rules.clone(),
            clinical: index
                .actors
                .keys()
                .map(|id| (*id, clinical(store, *id).cloned()))
                .collect(),
            occupancy: index
                .actors
                .keys()
                .map(|id| (*id, occupancy(store, *id).cloned()))
                .collect(),
            shelters: consulted
                .into_iter()
                .map(|id| (id, shelter(store, id).cloned()))
                .collect(),
            visible_shelters,
            observed_areas,
            deltas,
        })
    }
}

#[cfg(test)]
#[path = "locality_tests.rs"]
mod tests;
