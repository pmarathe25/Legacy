//! Collected water mixes physical contamination and updates a holder's own memory.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};

#[derive(Clone, Copy, Debug)]
pub struct WaterCollectionFacts {
    pub inventory: InventoryId,
    pub quantity_after: Quantity,
    pub collected: Quantity,
    pub contamination: u16,
    pub holder: Option<PersonId>,
    pub now: SimTime,
}
#[derive(Clone, Debug)]
pub struct PreparedWaterCollection {
    inventory: InventoryId,
    before_lot: Option<WaterLot>,
    after_lot: WaterLot,
    knowledge: Option<(PersonId, Option<Knowledge>, Knowledge)>,
}
impl PreparedWaterCollection {
    pub fn validate(&self, store: &Store) -> Result<(), Error> {
        if water_lot(store, self.inventory) != self.before_lot.as_ref()
            || self.knowledge.as_ref().is_some_and(|(actor, before, _)| {
                !store.contains(StableId(actor.0)) || knowledge(store, *actor) != before.as_ref()
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    /// Validate before composing these own-row writes with the material deposit.
    pub fn append_writes(self, batch: &mut Batch) {
        batch.insert(StableId(self.inventory.0), self.after_lot);
        if let Some((actor, _, after)) = self.knowledge {
            batch.insert(StableId(actor.0), after);
        }
    }
    pub fn stage_into(
        self,
        store: &Store,
        batch: &mut Batch,
    ) -> Result<game_ecs::ReadGuard, Error> {
        self.validate(store)?;
        let mut reads = game_ecs::ReadSet::new();
        let mut admitted = game_ecs::ReadFootprint::new();
        reads.expect_row::<WaterLot>(StableId(self.inventory.0), self.before_lot.clone());
        admitted.protect::<WaterLot>(StableId(self.inventory.0));
        if let Some((actor, before, _)) = &self.knowledge {
            reads.entity(store, StableId(actor.0));
            reads.expect_row::<Knowledge>(StableId(actor.0), before.clone());
            admitted.protect::<Knowledge>(StableId(actor.0));
        }
        self.append_writes(batch);
        reads.bind(store, batch, &admitted)
    }
    pub fn commit(self, store: &mut Store) -> Result<(), Error> {
        let mut batch = Batch::new();
        let guard = self.stage_into(store, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)
    }
}
pub fn prepare_water_collection(
    store: &Store,
    facts: WaterCollectionFacts,
) -> Result<PreparedWaterCollection, Error> {
    if facts.inventory.0 == 0
        || facts
            .holder
            .is_some_and(|actor| actor.0 == 0 || !store.contains(StableId(actor.0)))
    {
        return Err(Error::Unavailable);
    }
    let before_lot = water_lot(store, facts.inventory).cloned();
    let after_lot = collect_water(CollectionInput {
        quantity_after: facts.quantity_after.0,
        existing_load: before_lot.as_ref().map_or(0, WaterLot::load),
        collected: facts.collected.0,
        contamination: facts.contamination,
    })?;
    let observation = if let Some(actor) = facts.holder {
        let before = knowledge(store, actor).cloned();
        let old = before
            .as_ref()
            .and_then(|row| row.water.get(&facts.inventory));
        let purified = facts.contamination == 0
            && (facts.quantity_after == facts.collected || old.is_some_and(|seen| seen.purified));
        let suspected_unsafe =
            facts.contamination > 0 || old.is_some_and(|seen| seen.suspected_unsafe);
        let mut after = before.clone().unwrap_or_default();
        after.water.insert(
            facts.inventory,
            WaterSafetyView {
                inventory: facts.inventory,
                purified,
                suspected_unsafe,
                observed_at: facts.now,
            },
        );
        Some((actor, before, after))
    } else {
        None
    };
    Ok(PreparedWaterCollection {
        inventory: facts.inventory,
        before_lot,
        after_lot,
        knowledge: observation,
    })
}

/// Admit bounded future shapes under the built-in water/knowledge validators.
/// Any accepted collection has quantity<=capacity and contamination<=1000,
/// hence its complete physical load is bounded by capacity*1000 in u128.
/// Those validators do not constrain observation booleans or timestamps.
pub fn append_water_collection_schema_bounds(
    store: &Store,
    inventory: InventoryId,
    capacity: Quantity,
    holder: Option<PersonId>,
    at: SimTime,
    batch: &mut Batch,
) -> Result<(), Error> {
    if inventory.0 == 0
        || holder.is_some_and(|actor| actor.0 == 0 || !store.contains(StableId(actor.0)))
    {
        return Err(Error::Unavailable);
    }
    batch.insert(
        StableId(inventory.0),
        WaterLot::new(capacity.0, u128::from(capacity.0) * 1000),
    );
    if let Some(actor) = holder {
        let mut after = knowledge(store, actor).cloned().unwrap_or_default();
        after.water.insert(
            inventory,
            WaterSafetyView {
                inventory,
                purified: true,
                suspected_unsafe: true,
                observed_at: at,
            },
        );
        batch.insert(StableId(actor.0), after);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn facts() -> WaterCollectionFacts {
        WaterCollectionFacts {
            inventory: InventoryId(10),
            quantity_after: Quantity(200),
            collected: Quantity(100),
            contamination: 500,
            holder: Some(PersonId(1)),
            now: SimTime(10),
        }
    }
    #[test]
    fn mixed_collection_preserves_unrelated_memory_and_rejects_stale_own_rows() {
        let mut store = crate::component_store();
        store.ensure(StableId(1)).unwrap();
        store.ensure(StableId(10)).unwrap();
        insert_water_lot(&mut store, InventoryId(10), WaterLot::new(100, 10000)).unwrap();
        let prepared = prepare_water_collection(&store, facts()).unwrap();
        let before = store.clone();
        let mut changed = Knowledge::default();
        changed.water.insert(
            InventoryId(11),
            WaterSafetyView {
                inventory: InventoryId(11),
                purified: true,
                suspected_unsafe: false,
                observed_at: SimTime(1),
            },
        );
        insert_knowledge(&mut store, PersonId(1), changed).unwrap();
        let stale = store.clone();
        assert_eq!(prepared.commit(&mut store), Err(Error::Unavailable));
        assert_eq!(store, stale);
        prepare_water_collection(&store, facts())
            .unwrap()
            .commit(&mut store)
            .unwrap();
        let lot = water_lot(&store, InventoryId(10)).unwrap();
        assert_eq!((lot.quantity(), lot.load()), (200, 60000));
        let observed = knowledge(&store, PersonId(1)).unwrap();
        assert!(observed.water[&InventoryId(10)].suspected_unsafe);
        assert!(observed.water[&InventoryId(11)].purified);
        assert_ne!(store, before);
    }
    #[test]
    fn clean_collection_keeps_subjective_history_without_inventing_purification() {
        for old_purified in [false, true] {
            let mut store = crate::component_store();
            store.ensure(StableId(1)).unwrap();
            store.ensure(StableId(10)).unwrap();
            let mut knowledge = Knowledge::default();
            knowledge.water.insert(
                InventoryId(10),
                WaterSafetyView {
                    inventory: InventoryId(10),
                    purified: old_purified,
                    suspected_unsafe: true,
                    observed_at: SimTime(1),
                },
            );
            insert_knowledge(&mut store, PersonId(1), knowledge).unwrap();
            prepare_water_collection(
                &store,
                WaterCollectionFacts {
                    contamination: 0,
                    ..facts()
                },
            )
            .unwrap()
            .commit(&mut store)
            .unwrap();
            let known = &crate::knowledge(&store, PersonId(1)).unwrap().water[&InventoryId(10)];
            assert_eq!(known.purified, old_purified);
            assert!(known.suspected_unsafe);
        }
        let mut store = crate::component_store();
        store.ensure(StableId(1)).unwrap();
        store.ensure(StableId(10)).unwrap();
        prepare_water_collection(
            &store,
            WaterCollectionFacts {
                collected: Quantity(200),
                contamination: 0,
                ..facts()
            },
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
        assert!(knowledge(&store, PersonId(1)).unwrap().water[&InventoryId(10)].purified);
    }
    #[test]
    fn schema_bounds_admit_maximal_physical_load_without_publishing_observations() {
        let mut store = crate::component_store();
        store.ensure(StableId(1)).unwrap();
        store.ensure(StableId(10)).unwrap();
        let before = store.clone();
        let mut batch = Batch::new();
        append_water_collection_schema_bounds(
            &store,
            InventoryId(10),
            Quantity(u64::MAX),
            Some(PersonId(1)),
            SimTime(u64::MAX),
            &mut batch,
        )
        .unwrap();
        store.validate_batch(&batch).unwrap();
        assert_eq!(store, before);
        prepare_water_collection(
            &store,
            WaterCollectionFacts {
                quantity_after: Quantity(u64::MAX),
                collected: Quantity(u64::MAX),
                contamination: 1000,
                ..facts()
            },
        )
        .unwrap()
        .commit(&mut store)
        .unwrap();
        assert_eq!(
            water_lot(&store, InventoryId(10)).unwrap().load(),
            u128::from(u64::MAX) * 1000
        );
        assert!(knowledge(&store, PersonId(1)).unwrap().water[&InventoryId(10)].suspected_unsafe);
    }
}
