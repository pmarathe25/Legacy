//! Cancellation stages only this feature's existing work rows.
use crate::*;
use game_core::{Error, PersonId};
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeSet;
pub struct PreparedCultivationCancellation {
    before: Vec<(PersonId, Option<Cultivation>)>,
}
impl PreparedCultivationCancellation {
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if self
            .before
            .iter()
            .any(|(actor, before)| cultivation(store, *actor) != before.as_ref())
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(self, store: &Store, batch: &mut Batch) -> Result<(), Error> {
        self.stage_guarded_into(store, batch)?;
        Ok(())
    }
    pub fn stage_guarded_into(self, store: &Store, batch: &mut Batch) -> Result<ReadGuard, Error> {
        self.validate(store)?;
        let mut reads = ReadSet::new();
        let mut admitted = ReadFootprint::new();
        for (actor, before) in self.before {
            let id = StableId(actor.0);
            if batch.touches::<Cultivation>(id) {
                return Err(Error::Unavailable);
            }
            reads.expect_row(id, before.clone());
            if before.is_some() {
                admitted.protect::<Cultivation>(id);
                batch.remove::<Cultivation>(id);
            }
        }
        reads.bind(store, batch, &admitted)
    }
}

impl Runtime {
    pub fn prepare_cancel_cultivation(
        &self,
        store: &Store,
        actors: &[PersonId],
    ) -> PreparedCultivationCancellation {
        let mut seen = BTreeSet::new();
        PreparedCultivationCancellation {
            before: actors
                .iter()
                .copied()
                .filter(|actor| seen.insert(*actor))
                .map(|actor| (actor, cultivation(store, actor).cloned()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        Store::with_schema(schema)
    }

    fn cultivation() -> Cultivation {
        Cultivation {
            field: game_core::SiteId(50),
            operation: FarmOperation::Prepare,
            labor_ms: 0,
            completes: game_core::SimTime(500),
        }
    }

    fn runtime() -> Runtime {
        let crop = CropId("test/cereal".into());
        Runtime::new(
            Catalog {
                default_crop: crop.clone(),
                crops: std::collections::BTreeMap::from([(
                    crop,
                    CropRules {
                        planting_first_day: 0,
                        planting_last_day: 9,
                        prepare_labor: game_core::SimDuration(14_400_000),
                        plant_labor: game_core::SimDuration(7_200_000),
                        tend_labor: game_core::SimDuration(3_600_000),
                        irrigation_labor: game_core::SimDuration(3_600_000),
                        harvest_labor: game_core::SimDuration(14_400_000),
                        fallow_labor: game_core::SimDuration(3_600_000),
                        seed_input: game_core::Quantity(1_000),
                        irrigation_input: game_core::Quantity(1_000),
                        irrigation_water: 30,
                        base_temperature: 0,
                        ripe_degree_days: 20,
                        optimal_water: 50,
                        failure_stress_days: 12,
                        food_yield: game_core::Quantity(6_000_000),
                        seed_yield: game_core::Quantity(2_000),
                        soil_depletion: 20,
                        soil_recovery: 1,
                    },
                )]),
            },
            0,
        )
        .unwrap()
    }

    #[test]
    fn guarded_cancellation_rejects_a_late_row_for_a_selected_absent_actor() {
        let store = store();
        let actor = PersonId(1);
        let plan = runtime().prepare_cancel_cultivation(&store, &[actor]);
        let mut batch = Batch::new();
        let guard = plan.stage_guarded_into(&store, &mut batch).unwrap();
        batch.insert(StableId(actor.0), cultivation());
        assert_eq!(guard.validate(&store, &batch), Err(Error::Unavailable));
    }

    #[test]
    fn guarded_cancellation_allows_later_writes_for_unrelated_actors() {
        let mut store = store();
        let actor = PersonId(1);
        let unrelated = PersonId(2);
        insert_cultivation(&mut store, actor, cultivation()).unwrap();
        let plan = runtime().prepare_cancel_cultivation(&store, &[actor]);
        let mut batch = Batch::new();
        let guard = plan.stage_guarded_into(&store, &mut batch).unwrap();
        batch.insert(StableId(unrelated.0), cultivation());
        assert_eq!(guard.validate(&store, &batch), Ok(()));
    }
}
