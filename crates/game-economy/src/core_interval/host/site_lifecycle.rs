//! Physical demolition and recovery of material left in debris.
use super::*;
use game_economy_building::{demolition, salvage};
use game_lots::Place;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoreSiteChanges {
    pub demolished: Vec<demolition::Event>,
    pub salvaged: Vec<salvage::Event>,
    pub released: Vec<game_economy_confinement::Event>,
}

impl CoreSiteChanges {
    pub(super) fn is_empty(&self) -> bool {
        self.demolished.is_empty() && self.salvaged.is_empty() && self.released.is_empty()
    }
}

impl CoreHost {
    pub(super) fn site_action_busy(&self, blocked: &[u64]) -> Result<Vec<u64>, String> {
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        if offset >= self.core.physical.tick_millis()
            || self.core.next_deadline(None)?.after_millis == 0
        {
            return Err("publish pending interval events before acting at a site".into());
        }
        let busy = self.local_walk_exclusions(blocked);
        Ok(self.block_away_from_stock(&busy).unwrap_or(busy))
    }

    pub(super) fn change_sites_autonomously(
        &mut self,
        blocked: &[u64],
    ) -> Result<(Vec<u64>, CoreSiteChanges), String> {
        let Some(owner) = &self.construction else {
            return Ok((Vec::new(), CoreSiteChanges::default()));
        };
        if owner.demolition.is_empty() && !owner.materials.par_iter().any(|row| row.debris) {
            return Ok((Vec::new(), CoreSiteChanges::default()));
        }
        let mut busy = self.site_action_busy(blocked)?;
        let salvaged = self.recover_site_material(&busy, None)?;
        let mut actors: Vec<_> = salvaged.par_iter().map(|event| event.person).collect();
        busy.par_extend(actors.par_iter().copied());
        busy.par_sort_unstable();
        busy.dedup();
        let sites = self
            .sites
            .as_ref()
            .ok_or("construction requires physical sites")?;
        let owner = self.construction.as_ref().expect("construction checked");
        let tick = self.core.physical.tick();
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = demolition::Context {
            policies: &owner.demolition,
            sites: &sites.facilities,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            manual_site: None,
            repair_sites: &[],
            max_hunger: u32::MAX,
            seed: self.core.seed ^ self.core.decision_time_key(),
            tick,
        }
        .prepare();
        if plan.actors.is_empty() {
            return Ok((
                actors,
                CoreSiteChanges {
                    salvaged,
                    ..Default::default()
                },
            ));
        }
        let mut facilities = sites.facilities.clone();
        let demolished = plan.publish(tick, &sites.facilities, None, &mut facilities);
        actors.par_extend(demolished.par_iter().map(|row| row.person));
        actors.par_sort_unstable();
        let released = self.publish_site_demolition(facilities, &demolished);
        Ok((
            actors,
            CoreSiteChanges {
                demolished,
                salvaged,
                released,
            },
        ))
    }

    fn recover_site_material(
        &mut self,
        busy: &[u64],
        manual: Option<salvage::Manual>,
    ) -> Result<Vec<salvage::Event>, String> {
        let owner = self
            .construction
            .as_ref()
            .ok_or("site recovery is disabled")?;
        if manual.is_none() && !owner.materials.par_iter().any(|row| row.debris) {
            return Ok(Vec::new());
        }
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let claims = self.claims.as_ref().map(game_economy_claims::Claims::index);
        let plan = salvage::Context {
            policies: if manual.is_some() {
                &[]
            } else {
                &owner.salvaging
            },
            materials: &owner.materials,
            people: &self.core.people,
            view: &view,
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy,
            max_hunger: u32::MAX,
            manual,
        }
        .prepare(1)?;
        let mut draft = self
            .core
            .physical
            .prepare_interval_transfer(&plan.actions, self.core.seed)?;
        let outcomes: Vec<_> = draft.transactions().par_iter().copied().collect();
        let recovered = plan.publish_transactions(tick, &outcomes);
        let mut retired = salvage::exhausted(&owner.materials, &view, &recovered);
        if recovered.is_empty() && retired.is_empty() {
            return Ok(recovered);
        }
        retired.par_sort_unstable();
        self.core
            .physical
            .retire_interval_site_custody(&mut draft, retired.clone())?;
        self.core.physical.commit_interval_consumption(draft)?;
        let owner = self.construction.as_mut().expect("construction checked");
        owner
            .materials
            .retain(|row| retired.binary_search(&Place::Site(row.site)).is_err());
        owner.salvage_events = recovered.clone();
        self.work_checked_revision = None;
        Ok(recovered)
    }

    fn publish_site_demolition(
        &mut self,
        facilities: crate::WorkSites,
        events: &[demolition::Event],
    ) -> Vec<game_economy_confinement::Event> {
        if events.is_empty() {
            return Vec::new();
        }
        let owner = self.construction.as_mut().expect("construction checked");
        owner.demolition_events = events.to_vec();
        let destroyed: HashSet<_> = events
            .par_iter()
            .filter_map(|event| event.destroyed.then_some(event.site))
            .collect();
        if destroyed.is_empty() {
            return Vec::new();
        }
        owner.materials.par_iter_mut().for_each(|row| {
            row.debris |= destroyed.contains(&row.site);
        });
        let sites = self.sites.as_mut().expect("sites checked");
        if let Some(maintenance) = &mut sites.maintenance {
            maintenance.sync(&facilities);
        }
        sites.facilities = facilities;
        self.work_checked_revision = None;
        self.release_destroyed_barriers(&destroyed)
    }

    pub fn demolish_controlled_site(&mut self, person: u64, site: u64) -> Result<bool, String> {
        let actor = self.controlled_person(person)?;
        let busy = self.site_action_busy(&[])?;
        if self.construction.is_none() {
            return Err("construction is disabled".into());
        }
        let sites = self.sites.as_ref().ok_or("no physical sites")?;
        let index = sites
            .facilities
            .sites
            .binary_search_by_key(&site, |row| row.id)
            .map_err(|_| "unknown physical site")?;
        if actor.region != Some(sites.facilities.sites[index].region)
            || actor.health == 0
            || busy.binary_search(&person).is_ok()
        {
            return Err("acting at a site requires a living available person within reach".into());
        }
        let mut facilities = sites.facilities.clone();
        let destroyed = facilities.destroy_idle(site);
        self.publish_site_demolition(
            facilities,
            &[demolition::Event {
                tick: self.core.physical.tick(),
                person,
                site,
                destroyed,
            }],
        );
        Ok(destroyed)
    }

    pub fn salvage_controlled_site(
        &mut self,
        person: u64,
        site: u64,
        good: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        self.controlled_person(person)?;
        let busy = self.site_action_busy(&[])?;
        if busy.binary_search(&person).is_ok() {
            return Err("acting at a site requires an available person within reach".into());
        }
        let recovered = self.recover_site_material(
            &busy,
            Some(salvage::Manual {
                person,
                site,
                good,
                quantity,
            }),
        )?;
        Ok(recovered[0].recovered)
    }
}
