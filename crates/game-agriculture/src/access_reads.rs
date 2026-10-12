//! A local field access witness for receiving owners' factual admission reads.
use crate::Field;
use game_core::{AreaId, Error, PersonId, SiteId};
use game_ecs::{Batch, ReadFootprint, StableId, Store};

/// Retains one selected opening field. Other fields do not invalidate this
/// positive access witness; changing the selected field requires preparation.
pub struct FarmingAccessRead {
    entity: StableId,
    before: Field,
}

/// Selects the first accessible field in stable entity order, preserving the
/// local access predicate without inspecting crop state or the person's body.
pub fn prepare_farming_access(
    store: &Store,
    actor: PersonId,
    area: AreaId,
) -> Option<FarmingAccessRead> {
    store
        .iter::<Field>()
        .find(|(_, field)| field.area == area && field.access.contains(&actor))
        .map(|(entity, field)| FarmingAccessRead {
            entity,
            before: field.clone(),
        })
}

impl FarmingAccessRead {
    pub fn site(&self) -> SiteId {
        self.before.id
    }
    pub fn reads(&self) -> ReadFootprint {
        let mut reads = ReadFootprint::new();
        reads.protect::<Field>(self.entity);
        reads
    }
    pub fn check_source(&self, store: &Store) -> Result<(), Error> {
        if store.get::<Field>(self.entity) != Some(&self.before) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.check_source(store)?;
        if !self.reads().is_untouched(batch) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_core::InventoryId;
    use game_ecs::Schema;
    use std::collections::BTreeSet;

    fn field(id: u64, area: u64, actor: u64) -> Field {
        Field {
            id: SiteId(id),
            area: AreaId(area),
            access: BTreeSet::from([PersonId(actor)]),
            custodian: None,
            seed: InventoryId(101),
            tool: InventoryId(102),
            water: InventoryId(103),
            food: InventoryId(104),
        }
    }
    fn fixture() -> Store {
        let mut schema = Schema::new();
        crate::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let mut batch = Batch::new();
        // Insertion order must not become witness-selection order.
        batch
            .insert(StableId(17), field(17, 3, 7))
            .insert(StableId(11), field(11, 3, 7))
            .insert(StableId(5), field(5, 4, 7))
            .insert(StableId(3), field(3, 3, 8));
        store.apply_batch(batch).unwrap();
        store
    }

    #[test]
    fn local_farming_access_selects_the_first_actual_access_witness() {
        let store = fixture();
        let read = prepare_farming_access(&store, PersonId(7), AreaId(3)).unwrap();
        assert_eq!(read.site(), SiteId(11));
        assert!(read.reads().contains::<Field>(StableId(11)));
        assert!(!read.reads().contains::<Field>(StableId(17)));
        assert!(prepare_farming_access(&store, PersonId(7), AreaId(9)).is_none());
        assert!(prepare_farming_access(&store, PersonId(9), AreaId(3)).is_none());
        read.validate_batch(&store, &Batch::new()).unwrap();
    }

    #[test]
    fn farming_access_rejects_selected_field_edits_and_accepts_unrelated_field_updates() {
        let mut store = fixture();
        let read = prepare_farming_access(&store, PersonId(7), AreaId(3)).unwrap();
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        batch.insert(StableId(17), field(17, 4, 8));
        read.validate_batch(&store, &batch).unwrap();
        batch.remove::<Field>(StableId(11));
        assert!(read.validate_batch(&store, &batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);

        let mut batch = Batch::new();
        batch.insert(StableId(11), field(11, 3, 8));
        assert!(read.validate_batch(&store, &batch).is_err());
        store.get_mut::<Field>(StableId(11)).unwrap().access.clear();
        assert!(read.check_source(&store).is_err());
        // Another eligible field does not repair a stale selected witness.
        assert_eq!(
            prepare_farming_access(&store, PersonId(7), AreaId(3))
                .unwrap()
                .site(),
            SiteId(17)
        );
    }
}
