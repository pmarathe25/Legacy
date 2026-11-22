//! Physical stock rooms attached to accepted construction.
use super::*;
use game_economy_building::{Construction, Event};
use game_economy_local_transport::{
    Endpoint, LocalTransport, Store, StoreStructure, walking::Route,
};
use game_lot_runtime::{IntervalConsumptionDraft, StorageExposureDraft};
use game_lots::{Container, Place};

pub(super) fn validate_stores(
    transport: &LocalTransport,
    sites: Option<&CoreSites>,
    physical: &game_lot_runtime::Runtime,
) -> Result<(), String> {
    let mut linked: Vec<_> = transport
        .stores
        .par_iter()
        .filter_map(|store| {
            store
                .structure
                .map(|structure| (structure.site, store, structure))
        })
        .collect();
    if linked.is_empty() {
        return Ok(());
    }
    let sites = &sites
        .ok_or("constructed storage requires physical sites")?
        .facilities;
    linked.par_sort_unstable_by_key(|row| row.0);
    if linked.par_windows(2).any(|pair| pair[0].0 == pair[1].0)
        || linked.par_iter().any(|&(id, store, structure)| {
            let site = sites
                .sites
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|i| &sites.sites[i]);
            id == 0
                || id >= sites.next_id
                || if structure.intact {
                    site.is_none_or(|row| row.region != store.region)
                } else {
                    site.is_some()
                }
                || physical.storage_override_celsius(Place::Inventory(store.id))
                    != structure.intact.then_some(structure.celsius)
        })
    {
        return Err("invalid constructed storage link".into());
    }
    Ok(())
}

impl CoreHost {
    pub(super) fn stage_constructed_stores(
        &self,
        physical: IntervalConsumptionDraft,
        owner: &Construction,
        sites: &crate::WorkSites,
        events: &[Event],
    ) -> Result<(IntervalConsumptionDraft, Option<LocalTransport>), String> {
        let mut constructed: Vec<_> = events
            .par_iter()
            .filter_map(|event| {
                let id = event.site?;
                let definition = &owner.blueprints[owner
                    .blueprints
                    .binary_search_by_key(&event.blueprint, |row| row.id)
                    .ok()?];
                let design = definition.storage?;
                let site =
                    &sites.sites[sites.sites.binary_search_by_key(&id, |row| row.id).ok()?];
                Some((site, definition.name.as_str(), design, event.person))
            })
            .collect();
        if constructed.is_empty() {
            return Ok((physical, None));
        }
        constructed.par_sort_unstable_by_key(|row| row.0.id);
        let mut transport = self.local_transport.clone().unwrap_or_default();
        let previous = transport.stores.last().map_or(0, |store| store.id);
        previous
            .checked_add(constructed.len() as u64)
            .ok_or("local storage identity overflow")?;
        let stores: Vec<_> = constructed
            .par_iter()
            .enumerate()
            .map(|(index, (site, name, design, builder))| {
                let id = previous + index as u64 + 1;
                Store {
                    id,
                    region: site.region,
                    name: (*name).to_owned(),
                    structure: Some(StoreStructure {
                        site: site.id,
                        celsius: design.celsius,
                        intact: true,
                        builder: Some(*builder),
                    }),
                }
            })
            .collect();
        let holders = stores
            .par_iter()
            .zip(&constructed)
            .map(|(store, (_, _, design, _))| {
                Container::new(
                    Place::Inventory(store.id),
                    design.capacity.get(),
                    Vec::new(),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let temperatures: Vec<_> = stores
            .par_iter()
            .zip(&constructed)
            .map(|(store, (_, _, design, _))| (Place::Inventory(store.id), Some(design.celsius)))
            .collect();
        let physical =
            self.core
                .physical
                .configure_interval_storage(physical, holders, &temperatures)?;
        if let Some(walking) = &mut transport.walking {
            let routes = std::sync::Arc::make_mut(&mut walking.routes);
            routes.par_extend(stores.par_iter().zip(&constructed).flat_map_iter(
                |(store, (_, _, design, _))| {
                    let local = Endpoint::Store { id: store.id };
                    [(Endpoint::Stock, local), (local, Endpoint::Stock)].map(|(from, to)| Route {
                        region: store.region,
                        from,
                        to,
                        ticks: 0,
                        duration_millis: Some(design.walk_millis),
                    })
                },
            ));
            routes.par_sort_unstable_by_key(|r| (r.region, r.from, r.to));
        }
        transport.remember_built_cellars(&stores);
        transport.stores.extend(stores);
        Ok((physical, Some(transport)))
    }

    pub(super) fn stage_storage_exposure(
        &self,
        destroyed: &HashSet<u64>,
    ) -> Result<Option<StorageExposureDraft>, String> {
        if destroyed.is_empty() {
            return Ok(None);
        }
        self.core
            .physical
            .prepare_storage_exposure(&self.exposed_stores(destroyed))
    }

    fn exposed_stores(&self, destroyed: &HashSet<u64>) -> Vec<(Place, Option<i16>)> {
        self.local_transport
            .as_ref()
            .map_or_else(Vec::new, |transport| {
                transport
                    .stores
                    .par_iter()
                    .filter_map(|store| {
                        let structure = store.structure?;
                        (structure.intact && destroyed.contains(&structure.site))
                            .then_some((Place::Inventory(store.id), None))
                    })
                    .collect()
            })
    }
}
