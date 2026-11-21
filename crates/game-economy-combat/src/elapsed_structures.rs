//! Geometry and authored motivations for an attack on a physical site.
use crate::{Map, Point, structure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Structures {
    pub targets: Vec<structure::Target>,
    pub preferences: Vec<structure::Preference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteStrike {
    pub at_millis: u128,
    pub engagement: u64,
    pub sequence: u64,
    pub actor: u64,
    pub site: u64,
    pub harm: u32,
    pub collapsed: bool,
}

impl Structures {
    pub fn new(
        mut targets: Vec<structure::Target>,
        mut preferences: Vec<structure::Preference>,
    ) -> Self {
        targets.par_sort_unstable_by_key(|row| row.site);
        preferences.par_sort_unstable_by_key(|row| row.person);
        Self {
            targets,
            preferences,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && self.preferences.is_empty()
    }

    pub fn validate(&self, maps: &[Map], people: &HashSet<u64>) -> Result<(), String> {
        structure::validate_preferences(&self.preferences, people)?;
        if self
            .targets
            .par_windows(2)
            .any(|pair| pair[0].site >= pair[1].site)
            || self.targets.par_iter().any(|target| {
                target.site == 0
                    || maps
                        .binary_search_by_key(&target.region, |row| row.region)
                        .ok()
                        .is_none_or(|index| {
                            !maps[index].open(target.position)
                                || target
                                    .barrier
                                    .is_some_and(|wall| !maps[index].obstacles.contains(&wall))
                        })
            })
        {
            return Err("invalid elapsed tactical structure placement".into());
        }
        let mut walls: Vec<_> = self
            .targets
            .par_iter()
            .filter_map(|target| {
                let wall = target.barrier?;
                Some((
                    target.region,
                    wall.min.x,
                    wall.min.y,
                    wall.max.x,
                    wall.max.y,
                ))
            })
            .collect();
        walls.par_sort_unstable();
        if walls.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("a physical tactical barrier belongs to one site".into());
        }
        Ok(())
    }

    pub fn target(&self, site: u64, region: u64) -> Option<&structure::Target> {
        self.targets
            .binary_search_by_key(&site, |row| row.site)
            .ok()
            .map(|index| &self.targets[index])
            .filter(|target| target.region == region)
    }

    pub fn chosen(
        &self,
        actor: u64,
        from: Point,
        map: &Map,
        feelings: &(dyn Fn(u64, u64) -> (u64, u64) + Sync),
    ) -> Option<u64> {
        let preference = &self.preferences[self
            .preferences
            .binary_search_by_key(&actor, |row| row.person)
            .ok()?];
        let target = self.target(preference.site, map.region)?;
        let motivated = preference.willing
            || preference.pressure_from.is_some_and(|other| {
                let (fear, obligation) = feelings(actor, other);
                preference
                    .fear_at_least
                    .is_some_and(|minimum| fear >= minimum)
                    || preference
                        .obligation_at_least
                        .is_some_and(|minimum| obligation >= minimum)
            });
        (motivated && map.can_attack(from, target.position)).then_some(target.site)
    }

    /// Record destruction in the map only after the physical site is removed.
    pub fn remove(&mut self, destroyed: &HashSet<u64>, maps: &mut [Map]) {
        let mut walls: Vec<_> = self
            .targets
            .par_iter()
            .filter_map(|target| {
                if !destroyed.contains(&target.site) {
                    return None;
                }
                Some((target.region, target.barrier?))
            })
            .collect();
        if !walls.is_empty() {
            walls.par_sort_unstable_by_key(|row| row.0);
            maps.par_iter_mut().for_each(|map| {
                let start = walls.partition_point(|row| row.0 < map.region);
                let end = walls.partition_point(|row| row.0 <= map.region);
                for &(_, wall) in &walls[start..end] {
                    if let Some(index) = map.obstacles.iter().position(|row| *row == wall) {
                        map.obstacles.remove(index);
                    }
                }
            });
        }
        self.targets = std::mem::take(&mut self.targets)
            .into_par_iter()
            .filter(|target| !destroyed.contains(&target.site))
            .collect();
    }
}
