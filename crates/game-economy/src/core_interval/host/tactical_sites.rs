//! Resolve real site wear and collapse from accepted tactical strikes.
use super::*;
use game_economy_building::maintenance::Maintenance;
use game_economy_combat::{elapsed::Tactical, elapsed_structures::SiteStrike};

pub(super) fn validate_targets(
    owner: Option<&Tactical>,
    sites: Option<&CoreSites>,
) -> Result<(), String> {
    let Some(owner) = owner.filter(|owner| !owner.structures.targets.is_empty()) else {
        return Ok(());
    };
    let sites = sites.ok_or("tactical structures require physical sites")?;
    let conditions = &sites
        .maintenance
        .as_ref()
        .ok_or("tactical structures require site condition")?
        .conditions;
    if owner.structures.targets.par_iter().any(|row| {
        conditions
            .binary_search_by_key(&row.site, |condition| condition.site)
            .is_err()
            || sites
                .facilities
                .sites
                .binary_search_by_key(&row.site, |site| site.id)
                .ok()
                .is_none_or(|index| sites.facilities.sites[index].region != row.region)
    }) {
        return Err("tactical structures must match maintained local physical sites".into());
    }
    Ok(())
}

pub(super) struct CoreTacticalSiteDraft {
    maintenance: Maintenance,
    destroyed: HashSet<u64>,
    interrupted: Vec<(u64, u64)>,
}

impl CoreHost {
    pub(super) fn stage_tactical_sites(
        &self,
        strikes: &[SiteStrike],
    ) -> Result<Option<CoreTacticalSiteDraft>, String> {
        if strikes.is_empty() {
            return Ok(None);
        }
        let sites = self
            .sites
            .as_ref()
            .ok_or("tactical structures require physical sites")?;
        let mut maintenance = sites
            .maintenance
            .clone()
            .ok_or("tactical structures require site condition")?;
        let tactical = self.tactical().expect("tactical sites checked");
        let impacts: Vec<_> = strikes
            .par_iter()
            .map(|strike| (strike.site, strike.harm))
            .collect();
        maintenance.damage(&sites.facilities, &impacts);
        let damaged: HashSet<_> = impacts.par_iter().map(|row| row.0).collect();
        let destroyed: HashSet<_> = tactical
            .structures
            .targets
            .par_iter()
            .filter_map(|target| {
                if !target.collapses_at_limit || !damaged.contains(&target.site) {
                    return None;
                }
                let condition = &maintenance.conditions[maintenance
                    .conditions
                    .binary_search_by_key(&target.site, |row| row.site)
                    .ok()?];
                let site = &sites.facilities.sites[sites
                    .facilities
                    .sites
                    .binary_search_by_key(&target.site, |row| row.id)
                    .ok()?];
                let limit = maintenance.rules[maintenance
                    .rules
                    .binary_search_by(|row| row.kind.cmp(&site.kind))
                    .ok()?]
                .wear_limit;
                (condition.wear >= limit).then_some(site.id)
            })
            .collect();
        let mut interrupted: Vec<_> = sites
            .facilities
            .bindings
            .par_iter()
            .filter_map(|binding| {
                if !destroyed.contains(&binding.site) {
                    return None;
                }
                let index = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&binding.job, |row| row.job.id)
                    .ok()?;
                let work = &self.core.physical.work()[index];
                (work.phase == game_lot_runtime::Phase::Working && work.job.remaining != 0)
                    .then_some((work.job.id, work.job.worker))
            })
            .collect();
        interrupted.par_sort_unstable();
        interrupted.dedup();
        Ok(Some(CoreTacticalSiteDraft {
            maintenance,
            destroyed,
            interrupted,
        }))
    }

    pub(super) fn publish_tactical_sites(
        &mut self,
        draft: CoreTacticalSiteDraft,
        strikes: &mut [SiteStrike],
    ) -> Result<(Vec<u64>, Vec<game_economy_confinement::Event>), String> {
        let CoreTacticalSiteDraft {
            maintenance,
            destroyed,
            interrupted,
        } = draft;
        if !interrupted.is_empty() {
            self.abandon_work(&interrupted)?;
        }
        if destroyed.is_empty() {
            self.sites.as_mut().expect("site owner checked").maintenance = Some(maintenance);
            self.work_checked_revision = None;
            return Ok((Vec::new(), Vec::new()));
        }
        let mut facilities = self
            .sites
            .as_ref()
            .expect("site owner checked")
            .facilities
            .clone();
        facilities
            .bindings
            .retain(|binding| !destroyed.contains(&binding.site));
        facilities.destroy_idle_many(&destroyed);
        self.sites.as_mut().expect("site owner checked").maintenance = Some(maintenance);
        let released = self.publish_destroyed_sites(facilities, &destroyed);
        strikes
            .par_iter_mut()
            .for_each(|strike| strike.collapsed = destroyed.contains(&strike.site));
        Ok((
            interrupted.into_par_iter().map(|row| row.0).collect(),
            released,
        ))
    }
}
