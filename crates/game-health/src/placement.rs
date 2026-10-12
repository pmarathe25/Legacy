//! Coalesced shelter departure and projected placement, owned by Health.
use super::*;
#[derive(Clone, Copy, Debug)]
pub struct ShelterPlacement {
    pub actor: PersonId,
    pub final_area: Option<AreaId>,
}
/// Actual opening body, carrier, activity and area facts. Health follows the
/// carrier chain and selects the effective shelter itself.
pub trait ShelterPlacementFacts {
    fn actor(&self, actor: PersonId) -> Option<ShelterActorFacts>;
}
impl ShelterPlacementFacts for BTreeMap<PersonId, ShelterActorFacts> {
    fn actor(&self, actor: PersonId) -> Option<ShelterActorFacts> {
        self.get(&actor).copied()
    }
}
pub struct PreparedPlacement {
    change: PreparedHealth,
    actors: BTreeMap<PersonId, ShelterActorFacts>,
    occupancy: BTreeMap<PersonId, Option<Occupancy>>,
}
impl Runtime {
    pub fn prepare_placements(
        &self,
        store: &Store,
        leaving: &[PersonId],
        placements: &[ShelterPlacement],
        facts: &impl ShelterPlacementFacts,
    ) -> Result<PreparedPlacement, Error> {
        let mut change = PreparedHealth::new(self);
        let mut seen = BTreeSet::new();
        let leaving = leaving
            .iter()
            .copied()
            .filter(|actor| seen.insert(*actor))
            .collect::<Vec<_>>();
        change.leave(store, &leaving);
        let mut actor_reads = BTreeMap::new();
        let mut occupancy_reads = BTreeMap::new();
        seen.clear();
        for placement in placements {
            if placement.actor.0 == 0
                || !seen.insert(placement.actor)
                || placement.final_area.is_some_and(|area| area.0 == 0)
            {
                return Err(Error::Unavailable);
            }
            let inherited = self.effective_shelter(store, placement.actor, |actor| {
                let row = facts.actor(actor)?;
                actor_reads.insert(actor, row);
                occupancy_reads.insert(actor, occupancy(store, actor).cloned());
                change.occupancy.entry(actor).or_insert_with(|| {
                    let before = occupancy(store, actor).cloned();
                    (before.clone(), before)
                });
                if let Some(occupied) = occupancy(store, actor) {
                    let before = shelter(store, occupied.shelter).cloned();
                    change
                        .shelter_reads
                        .insert(occupied.shelter, before.clone());
                    change
                        .shelters
                        .entry(occupied.shelter)
                        .or_insert((before.clone(), before));
                }
                Some(row)
            })?;
            let after = inherited
                .filter(|id| {
                    shelter(store, *id)
                        .is_some_and(|shelter| Some(shelter.area) == placement.final_area)
                })
                .map(|id| Occupancy { shelter: id });
            change.occupancy.insert(
                placement.actor,
                (occupancy(store, placement.actor).cloned(), after),
            );
        }
        Ok(PreparedPlacement {
            change,
            actors: actor_reads,
            occupancy: occupancy_reads,
        })
    }
}
impl PreparedPlacement {
    pub fn effects(&self) -> &[HealthEffect] {
        self.change.effects()
    }
    pub fn validate(
        &self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl ShelterPlacementFacts,
    ) -> Result<(), Error> {
        if self
            .actors
            .iter()
            .any(|(actor, before)| facts.actor(*actor).as_ref() != Some(before))
            || self
                .occupancy
                .iter()
                .any(|(actor, before)| occupancy(store, *actor) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        self.change.validate(runtime, store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl ShelterPlacementFacts,
        batch: &mut Batch,
    ) -> Result<PlacementFinalization, Error> {
        self.validate(runtime, store, facts)?;
        let occupancy = self
            .change
            .occupancy
            .keys()
            .map(|id| StableId(id.0))
            .collect();
        let shelters = self
            .change
            .shelters
            .keys()
            .map(|id| StableId(id.0))
            .collect();
        let finish = self.change.stage_into(runtime, store, batch)?;
        Ok(PlacementFinalization {
            finish,
            occupancy,
            shelters,
            batch_key: batch.key(),
        })
    }
    pub fn commit(
        self,
        runtime: &mut Runtime,
        store: &mut Store,
        facts: &impl ShelterPlacementFacts,
    ) -> Result<Vec<HealthEffect>, Error> {
        let mut batch = Batch::new();
        let finish = self.stage_into(runtime, store, facts, &mut batch)?;
        finish.validate_footprint(&batch)?;
        store.apply_batch(batch)?;
        Ok(finish.finish())
    }
    pub fn prepare_projected_observations<K: Clone + Ord>(
        &self,
        runtime: &Runtime,
        store: &Store,
        facts: &impl ShelterPlacementFacts,
        at: SimTime,
        actors: &[LocalHealthActor<K>],
        geometry: &[HealthGeometry],
    ) -> Result<PreparedLocalObservation, Error> {
        self.validate(runtime, store, facts)?;
        let after = self
            .change
            .occupancy
            .iter()
            .map(|(actor, (_, after))| (*actor, after.clone()))
            .collect();
        runtime.prepare_projected_local_observations(store, at, actors, geometry, &after)
    }
}
#[cfg(test)]
#[path = "placement/tests.rs"]
mod tests;

pub struct PlacementFinalization {
    finish: HealthFinalization,
    occupancy: BTreeSet<StableId>,
    shelters: BTreeSet<StableId>,
    batch_key: game_ecs::BatchKey,
}
impl PlacementFinalization {
    pub fn validate_footprint(&self, batch: &Batch) -> Result<(), Error> {
        if self.batch_key != batch.key() {
            return Err(Error::Unavailable);
        }
        for id in &self.occupancy {
            if batch
                .edited_ids::<Occupancy>()
                .filter(|edited| edited == id)
                .count()
                != 1
            {
                return Err(Error::Unavailable);
            }
        }
        for id in &self.shelters {
            if batch
                .edited_ids::<Shelter>()
                .filter(|edited| edited == id)
                .count()
                != 1
            {
                return Err(Error::Unavailable);
            }
        }
        Ok(())
    }
    pub fn finish(self) -> Vec<HealthEffect> {
        self.finish.finish()
    }
}
