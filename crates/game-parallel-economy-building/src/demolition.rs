//! Personal decisions to destroy particular physical sites.
use game_parallel_economy_person::Person;
use game_parallel_opinions::Opinions;
use game_parallel_work_sites::{Facilities, Plan as SitePlan};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub person: u64,
    pub site: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub site: u64,
    pub destroyed: bool,
}

pub(crate) fn valid(
    policies: &[Policy],
    events: &[Event],
    people: &HashSet<u64>,
    tick: u64,
) -> bool {
    !policies.par_windows(2).any(|w| w[0].person >= w[1].person)
        && !policies.par_iter().any(|p| {
            !people.contains(&p.person)
                || p.site == 0
                || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                || p.pressure_threshold == Some(0)
                || p.pressured_by
                    .is_some_and(|other| other == p.person || !people.contains(&other))
        })
        && !events
            .par_iter()
            .any(|e| e.tick > tick || e.site == 0 || !people.contains(&e.person))
}

#[derive(Clone, Copy)]
struct Choice {
    person: u64,
    site: u64,
    priority: u64,
}

pub struct Plan {
    choices: Vec<Choice>,
    pub actors: Vec<u64>,
}

pub struct Context<'a> {
    pub policies: &'a [Policy],
    pub sites: &'a Facilities,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub busy: &'a [u64],
    pub manual_site: Option<u64>,
    pub max_hunger: u32,
    pub seed: u64,
    pub tick: u64,
}

fn priority(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

impl Context<'_> {
    pub fn prepare(&self) -> Plan {
        if self.policies.is_empty() {
            return Plan {
                choices: Vec::new(),
                actors: Vec::new(),
            };
        }
        let opinions = self.opinions.map(Opinions::index);
        let mut choices: Vec<_> = self
            .policies
            .par_iter()
            .filter_map(|policy| {
                if self.busy.binary_search(&policy.person).is_ok()
                    || self.manual_site == Some(policy.site)
                {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&policy.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let site = self
                    .sites
                    .sites
                    .binary_search_by_key(&policy.site, |s| s.id)
                    .ok()
                    .map(|i| &self.sites.sites[i])?;
                if person.controlled
                    || person.health == 0
                    || person.hunger > self.max_hunger
                    || person.region != Some(site.region)
                {
                    return None;
                }
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(other, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, duty) = index.feelings(person.id, other);
                            fear.saturating_add(duty) >= threshold
                        })
                    });
                (policy.willing || pressured).then_some(Choice {
                    person: person.id,
                    site: site.id,
                    priority: priority(self.seed ^ self.tick.rotate_left(17) ^ person.id),
                })
            })
            .collect();
        choices.par_sort_unstable_by_key(|c| (c.site, c.priority, c.person));
        let actors = choices.par_iter().map(|c| c.person).collect();
        Plan { choices, actors }
    }
}

impl Plan {
    /// Competing people make independent attempts; each physical site is removed at most once.
    pub fn publish(
        self,
        tick: u64,
        prior: &Facilities,
        reservations: Option<&SitePlan>,
        sites: &mut Facilities,
    ) -> Vec<Event> {
        if self.choices.is_empty() {
            return Vec::new();
        }
        let mut blocked: HashSet<u64> = prior
            .bindings
            .par_iter()
            .map(|binding| binding.site)
            .collect();
        if let Some(plan) = reservations {
            blocked.par_extend(plan.reserved_sites());
        }
        blocked.par_extend(sites.bindings.par_iter().map(|binding| binding.site));
        let winners: HashSet<_> = self
            .choices
            .par_chunk_by(|a, b| a.site == b.site)
            .filter(|group| !blocked.contains(&group[0].site))
            .map(|group| group[0].person)
            .collect();
        let destroyed: HashSet<_> = self
            .choices
            .par_iter()
            .filter(|c| winners.contains(&c.person))
            .map(|c| c.site)
            .collect();
        sites.destroy_idle_many(&destroyed);
        self.choices
            .par_iter()
            .map(|c| Event {
                tick,
                person: c.person,
                site: c.site,
                destroyed: winners.contains(&c.person),
            })
            .collect()
    }
}
