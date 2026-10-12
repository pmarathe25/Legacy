//! A site's first farm output is a sparse, guarded native read.
use crate::{Project, ProjectKind, projects};
use game_core::{Error, Good, InventoryId, SiteId};
use game_ecs::{Batch, BatchKey, ReadFootprint, StableId, Store};

pub struct SiteOutputRead {
    site: SiteId,
    good: Good,
    selected: Option<(StableId, Project)>,
}
pub struct SiteOutputGuard {
    key: BatchKey,
    source: SiteOutputRead,
}
fn first_farm(store: &Store, site: SiteId) -> Option<(StableId, &Project)> {
    projects(store)
        .find(|(_, row)| matches!(row.kind,ProjectKind::Farm {plot,..} if plot==site))
        .map(|(id, row)| (StableId(id.0), row))
}
fn endpoint(row: &Project, good: Good) -> Option<InventoryId> {
    match good {
        Good::Food => Some(row.output),
        Good::Seed => Some(row.input),
        _ => None,
    }
}
pub(crate) fn site_output(store: &Store, site: SiteId, good: Good) -> Option<InventoryId> {
    first_farm(store, site).and_then(|(_, row)| endpoint(row, good))
}
/// Selection precedes good mapping: the earliest farm may return no endpoint.
/// Its status and the actual inventory's good do not change this lookup.
pub fn prepare_site_output_read(store: &Store, site: SiteId, good: Good) -> SiteOutputRead {
    SiteOutputRead {
        site,
        good,
        selected: first_farm(store, site).map(|(id, row)| (id, row.clone())),
    }
}
impl SiteOutputRead {
    pub fn output(&self) -> Option<InventoryId> {
        self.selected
            .as_ref()
            .and_then(|(_, row)| endpoint(row, self.good))
    }
    pub fn reads(&self) -> ReadFootprint {
        let mut reads = ReadFootprint::new();
        if let Some((id, _)) = &self.selected {
            reads.protect::<Project>(*id);
        }
        reads
    }
    pub fn check_source(&self, store: &Store) -> Result<(), Error> {
        let current = first_farm(store, self.site);
        if current.map(|(id, _)| id) != self.selected.as_ref().map(|(id, _)| *id)
            || current.is_some_and(|(_, row)| {
                self.selected
                    .as_ref()
                    .is_none_or(|(_, before)| row != before)
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    fn check_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        self.check_source(store)?;
        if !self.reads().is_untouched(batch)
            || batch.insertions::<Project>().any(|(id, row)| {
                self.selected
                    .as_ref()
                    .is_none_or(|(selected, _)| id < *selected)
                    && matches!(row.kind,ProjectKind::Farm {plot,..} if plot==self.site)
            })
        {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn stage_into(self, store: &Store, batch: &Batch) -> Result<SiteOutputGuard, Error> {
        self.check_batch(store, batch)?;
        Ok(SiteOutputGuard {
            key: batch.key(),
            source: self,
        })
    }
}
impl SiteOutputGuard {
    pub fn validate_batch(&self, store: &Store, batch: &Batch) -> Result<(), Error> {
        if self.key != batch.key() {
            return Err(Error::Unavailable);
        }
        self.source.check_batch(store, batch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectStatus, register_components};
    use game_core::{AreaId, PersonId, ProjectId, SimTime};
    use game_ecs::Schema;
    use std::collections::{BTreeMap, BTreeSet};
    fn farm(id: u64, plot: u64) -> Project {
        Project {
            id: ProjectId(id),
            kind: ProjectKind::Farm {
                plot: SiteId(plot),
                water: InventoryId(9),
            },
            sponsor: PersonId(3),
            area: AreaId(2),
            materials: InventoryId(4),
            tool: InventoryId(5),
            input: InventoryId(id + 100),
            output: InventoryId(id + 200),
            beneficiaries: BTreeSet::from([PersonId(3)]),
            workers: BTreeSet::from([PersonId(3)]),
            support: BTreeSet::new(),
            status: ProjectStatus::Ready,
            blocker: None,
            progress_ms: 0,
            contributions: BTreeMap::new(),
            materials_consumed: false,
            finish_by: SimTime(500),
            enterprise: None,
        }
    }
    fn fixture() -> Store {
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        game_cargo::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let mut batch = Batch::new();
        batch
            .insert(StableId(17), farm(17, 20))
            .insert(StableId(11), farm(11, 20));
        store.apply_batch(batch).unwrap();
        store
    }
    #[test]
    fn lookup_preserves_first_farm_then_good_mapping_and_terminal_status() {
        let mut store = fixture();
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id: InventoryId(211),
                good: Good::Timber,
                location: game_cargo::InventoryLocation::Area(AreaId(2)),
                quantity: game_core::Quantity(0),
                capacity: game_core::Quantity(1_000),
                access: BTreeSet::new(),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
        let row = store.get_mut::<Project>(StableId(11)).unwrap();
        row.status = ProjectStatus::Failed;
        assert_eq!(
            prepare_site_output_read(&store, SiteId(20), Good::Food).output(),
            Some(InventoryId(211))
        );
        assert_eq!(
            prepare_site_output_read(&store, SiteId(20), Good::Seed).output(),
            Some(InventoryId(111))
        );
        let unsupported = prepare_site_output_read(&store, SiteId(20), Good::Timber);
        assert_eq!(unsupported.output(), None);
        assert!(unsupported.reads().contains::<Project>(StableId(11)));
        assert!(!unsupported.reads().contains::<Project>(StableId(17)));
        assert_eq!(
            unsupported.output(),
            crate::site_output(&store, SiteId(20), Good::Timber)
        );
    }
    #[test]
    fn final_site_output_guard_is_scoped_to_selected_row_and_earlier_membership() {
        let mut store = fixture();
        let witness = prepare_site_output_read(&store, SiteId(20), Good::Food);
        let before = store.snapshot().unwrap();
        let mut batch = Batch::new();
        let guard = witness.stage_into(&store, &batch).unwrap();
        batch
            .insert(StableId(17), farm(17, 30))
            .insert(StableId(5), farm(5, 30));
        guard.validate_batch(&store, &batch).unwrap();
        assert!(guard.validate_batch(&store, &Batch::new()).is_err());
        batch.insert(StableId(7), farm(7, 20));
        assert!(guard.validate_batch(&store, &batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        let witness = prepare_site_output_read(&store, SiteId(20), Good::Food);
        let mut batch = Batch::new();
        let guard = witness.stage_into(&store, &batch).unwrap();
        batch.remove::<Project>(StableId(11));
        assert!(guard.validate_batch(&store, &batch).is_err());
        let witness = prepare_site_output_read(&store, SiteId(20), Good::Food);
        let mut batch = Batch::new();
        let guard = witness.stage_into(&store, &batch).unwrap();
        batch.insert(
            StableId(11),
            store.get::<Project>(StableId(11)).unwrap().clone(),
        );
        assert!(guard.validate_batch(&store, &batch).is_err());
        let witness = prepare_site_output_read(&store, SiteId(20), Good::Food);
        let batch = Batch::new();
        let guard = witness.stage_into(&store, &batch).unwrap();
        store.get_mut::<Project>(StableId(11)).unwrap().finish_by = SimTime(501);
        assert!(guard.validate_batch(&store, &batch).is_err());
    }
    #[test]
    fn absent_or_unsupported_output_still_guards_query_membership() {
        let store = fixture();
        for (site, good) in [(SiteId(30), Good::Food), (SiteId(20), Good::Timber)] {
            let witness = prepare_site_output_read(&store, site, good);
            assert_eq!(witness.output(), None);
            let mut batch = Batch::new();
            let guard = witness.stage_into(&store, &batch).unwrap();
            batch.insert(StableId(21), farm(21, 40));
            guard.validate_batch(&store, &batch).unwrap();
            batch.insert(StableId(7), farm(7, site.0));
            assert!(guard.validate_batch(&store, &batch).is_err());
        }
    }
}
