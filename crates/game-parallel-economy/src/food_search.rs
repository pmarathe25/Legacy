//! Hungry people travel using bounded personal reports, which may be stale.
use game_parallel_economy_claims::Index as Claims;
use game_parallel_economy_knowledge::Knowledge;
use game_parallel_economy_person::{
    Person,
    travel::{Departure, Travel, routes},
};
use game_parallel_lots::{Place, View};
use game_parallel_production_model::Good;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub enabled: bool,
    #[serde(default)]
    pub next_review: u64,
    /// A remembered destination remains personal while intermediate legs are traveled.
    #[serde(default)]
    pub target: Option<u64>,
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodSearch {
    pub min_hunger: u32,
    pub min_report_food: u32,
    pub max_report_age: u64,
    pub retry_ticks: u64,
    #[serde(default)]
    pub inherit_births: bool,
    pub people: Vec<Preference>,
}
impl Clone for FoodSearch {
    fn clone(&self) -> Self {
        Self {
            min_hunger: self.min_hunger,
            min_report_food: self.min_report_food,
            max_report_age: self.max_report_age,
            retry_ticks: self.retry_ticks,
            inherit_births: self.inherit_births,
            people: self.people.par_iter().cloned().collect(),
        }
    }
}
/// Inherit a parent's inclination, not the parent's food reports.
pub fn enroll(owner: Option<&mut FoodSearch>, family: Option<&crate::Families>, tick: u64) {
    let Some((owner, family)) = owner
        .zip(family)
        .filter(|(owner, family)| owner.inherit_births && family.rules.enroll_births)
    else {
        return;
    };
    let mut preferences: Vec<_> = owner.people.par_iter().collect();
    preferences.par_sort_unstable_by_key(|p| p.person);
    let newborns: Vec<_> = family
        .members
        .par_iter()
        .filter(|m| m.born == Some(tick))
        .filter_map(|m| {
            let enabled = m.parents.iter().find_map(|id| {
                preferences
                    .binary_search_by_key(id, |p| p.person)
                    .ok()
                    .map(|i| preferences[i].enabled)
            })?;
            Some(Preference {
                person: m.id,
                enabled,
                next_review: tick,
                target: None,
            })
        })
        .collect();
    owner.people.par_extend(newborns);
}
impl FoodSearch {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        knowledge: Option<&Knowledge>,
        travel: Option<&Travel>,
    ) -> Result<(), String> {
        if self.min_report_food == 0
            || self.retry_ticks == 0
            || knowledge.is_none()
            || travel.is_none()
        {
            return Err("food search needs valid rules, personal knowledge and routes".into());
        }
        let mut ids: Vec<_> = self.people.par_iter().map(|p| p.person).collect();
        ids.par_sort_unstable();
        let regions: HashSet<_> = travel
            .into_iter()
            .flat_map(|t| &t.routes)
            .flat_map(|r| [r.source, r.destination])
            .collect();
        if ids.par_windows(2).any(|p| p[0] == p[1])
            || ids.par_iter().any(|id| !people.contains(id))
            || self
                .people
                .par_iter()
                .any(|p| p.target.is_some_and(|id| !regions.contains(&id)))
        {
            return Err("invalid personal food search preference".into());
        }
        Ok(())
    }
    pub fn publish(
        &mut self,
        departures: &[Departure],
        targets: &[(u64, u64)],
        abandoned: &[u64],
        tick: u64,
    ) -> Result<(), String> {
        let next = if departures.is_empty() {
            tick
        } else {
            tick.checked_add(self.retry_ticks)
                .ok_or("food search review overflow")?
        };
        let mut ids: Vec<_> = departures.par_iter().map(|d| d.person).collect();
        ids.par_sort_unstable();
        let mut targets: Vec<_> = targets.par_iter().copied().collect();
        targets.par_sort_unstable();
        let mut abandoned: Vec<_> = abandoned.par_iter().copied().collect();
        abandoned.par_sort_unstable();
        self.people.par_iter_mut().for_each(|p| {
            if ids.binary_search(&p.person).is_ok() {
                p.next_review = next;
                p.target = targets
                    .binary_search_by_key(&p.person, |row| row.0)
                    .ok()
                    .map(|i| targets[i].1);
            } else if abandoned.binary_search(&p.person).is_ok() {
                p.target = None;
            }
        });
        Ok(())
    }
}
pub struct Plan {
    pub departures: Vec<Departure>,
    pub targets: Vec<(u64, u64)>,
    pub abandoned: Vec<u64>,
}

pub struct Context<'a> {
    pub owner: &'a FoodSearch,
    pub knowledge: &'a Knowledge,
    pub travel: &'a Travel,
    pub people: &'a [Person],
    pub goods: &'a [Good],
    pub view: &'a View,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub now: u64,
}
impl Context<'_> {
    pub fn prepare(&self) -> Plan {
        let portions: HashMap<_, _> = self
            .goods
            .iter()
            .filter_map(|g| g.food.as_ref().map(|food| (g.id, food.portion)))
            .collect();
        let mut local: Vec<_> = self
            .view
            .containers
            .par_iter()
            .filter_map(|c| {
                let Place::Stock { region, good } = c.place else {
                    return None;
                };
                let portion = *portions.get(&good)?;
                (c.quantity(good) >= portion).then_some((region, good))
            })
            .collect();
        if local.par_windows(2).any(|p| p[0] > p[1]) {
            local.par_sort_unstable();
        }
        let mut minds: Vec<_> = self.knowledge.minds.par_iter().collect();
        if minds.par_windows(2).any(|p| p[0].person > p[1].person) {
            minds.par_sort_unstable_by_key(|m| m.person);
        }
        let mut destinations: Vec<_> = minds
            .par_iter()
            .flat_map_iter(|mind| {
                mind.records
                    .iter()
                    .filter(|r| {
                        r.food >= self.owner.min_report_food
                            && self.now.saturating_sub(r.observed_at) <= self.owner.max_report_age
                    })
                    .map(|r| r.region)
            })
            .chain(self.owner.people.par_iter().filter_map(|p| p.target))
            .collect();
        destinations.par_sort_unstable();
        destinations = destinations
            .par_chunk_by(|a, b| a == b)
            .map(|group| group[0])
            .collect();
        let routes = routes(self.travel, &destinations);
        let decisions: Vec<_> = self
            .owner
            .people
            .par_iter()
            .filter_map(|preference| {
                let person = self
                    .people
                    .binary_search_by_key(&preference.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let region = person.region?;
                if !preference.enabled
                    || person.controlled
                    || person.health == 0
                    || !person.seek_meals
                    || person.hunger < self.owner.min_hunger
                {
                    return preference.target.map(|_| (person.id, None));
                }
                let holding = self.view.container(Place::Person(person.id))?;
                if holding.goods.iter().any(|item| {
                    portions
                        .get(&item.good)
                        .is_some_and(|portion| item.quantity >= *portion)
                }) {
                    return preference.target.map(|_| (person.id, None));
                }
                let start = local.partition_point(|row| row.0 < region);
                let end = local.partition_point(|row| row.0 <= region);
                if local[start..end].iter().any(|(_, good)| {
                    self.claims
                        .is_none_or(|claims| claims.allows(person.id, region, *good))
                }) {
                    return preference.target.map(|_| (person.id, None));
                }
                if preference.target == Some(region) {
                    return Some((person.id, None));
                }
                if self.busy.binary_search(&person.id).is_ok()
                    || (preference.target.is_none() && self.now < preference.next_review)
                {
                    return None;
                }
                let mind = minds
                    .binary_search_by_key(&person.id, |m| m.person)
                    .ok()
                    .map(|i| minds[i])?;
                let destination = preference.target.or_else(|| {
                    mind.records
                        .iter()
                        .filter_map(|record| {
                            let (ticks, _) = routes.get(&(region, record.region))?;
                            (record.region != region
                                && record.food >= self.owner.min_report_food
                                && self.now.saturating_sub(record.observed_at)
                                    <= self.owner.max_report_age)
                                .then_some((
                                    record.food,
                                    record.observed_at,
                                    std::cmp::Reverse(*ticks),
                                    std::cmp::Reverse(record.region),
                                ))
                        })
                        .max()
                        .map(|row| row.3.0)
                })?;
                let Some((_, next)) = routes.get(&(region, destination)) else {
                    return preference.target.map(|_| (person.id, None));
                };
                Some((person.id, Some((*next, destination))))
            })
            .collect();
        let departures = decisions
            .par_iter()
            .filter_map(|&(person, choice)| {
                choice.map(|(destination, _)| Departure {
                    person,
                    destination,
                })
            })
            .collect();
        let targets = decisions
            .par_iter()
            .filter_map(|&(person, choice)| choice.map(|(_, target)| (person, target)))
            .collect();
        let abandoned = decisions
            .par_iter()
            .filter_map(|&(person, choice)| choice.is_none().then_some(person))
            .collect();
        Plan {
            departures,
            targets,
            abandoned,
        }
    }
}
