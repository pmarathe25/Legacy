//! Wear from completed work and personally chosen repairs of idle sites.
use game_economy_claims::Index as Claims;
use game_economy_person::Person;
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction};
use game_opinions::Opinions;
use game_work_sites::Facilities;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// IDs are often contiguous, but saves and scenarios can contain gaps.
fn index_by<T>(rows: &[T], id: u64, key: impl Fn(&T) -> u64) -> Option<usize> {
    rows.first()
        .and_then(|first| id.checked_sub(key(first)))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&i| rows.get(i).is_some_and(|row| key(row) == id))
        .or_else(|| rows.binary_search_by_key(&id, key).ok())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub kind: String,
    pub wear_limit: u64,
    pub wear_per_slot: u64,
    pub repair_good: u32,
    pub repair_quantity: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub site: u64,
    pub wear: u64,
}
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
    #[serde(default)]
    pub disregard_claims: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    pub site: u64,
    pub repaired: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maintenance {
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(default)]
    pub policies: Vec<Policy>,
    #[serde(default)]
    pub events: Vec<Event>,
}
impl Maintenance {
    /// Apply simultaneous, attributed external damage to persistent site wear.
    /// Existing work has already resolved for this boundary; later jobs see
    /// the resulting condition through the usual site-admission rules.
    pub fn damage(&mut self, sites: &Facilities, strikes: &[(u64, u32)]) {
        if strikes.is_empty() {
            return;
        }
        let mut rows: Vec<_> = strikes.par_iter().copied().collect();
        if rows.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            rows.par_sort_unstable_by_key(|row| row.0);
        }
        let totals: Vec<_> = rows
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                (
                    group[0].0,
                    group
                        .par_iter()
                        .map(|row| u64::from(row.1))
                        .reduce(|| 0, u64::saturating_add),
                )
            })
            .collect();
        let rules = &self.rules;
        self.conditions.par_chunks_mut(2048).for_each(|chunk| {
            let first = chunk[0].site;
            let last = chunk[chunk.len() - 1].site;
            let start = totals.partition_point(|row| row.0 < first);
            let end = totals.partition_point(|row| row.0 <= last);
            if start == end {
                return;
            }
            let local = &totals[start..end];
            for condition in chunk {
                let Ok(i) = local.binary_search_by_key(&condition.site, |row| row.0) else {
                    continue;
                };
                let Some(site) =
                    index_by(&sites.sites, condition.site, |site| site.id).map(|i| &sites.sites[i])
                else {
                    continue;
                };
                let Ok(rule) = rules.binary_search_by(|rule| rule.kind.cmp(&site.kind)) else {
                    continue;
                };
                condition.wear = condition
                    .wear
                    .saturating_add(local[i].1)
                    .min(rules[rule].wear_limit);
            }
        });
    }
    pub fn normalize(&mut self) {
        self.rules.par_sort_unstable_by(|a, b| a.kind.cmp(&b.kind));
        self.conditions.par_sort_unstable_by_key(|r| r.site);
        self.policies.par_sort_unstable_by_key(|r| r.person);
    }
    pub fn sync(&mut self, sites: &Facilities) {
        self.conditions = self
            .conditions
            .par_iter()
            .filter(|r| sites.sites.binary_search_by_key(&r.site, |s| s.id).is_ok())
            .cloned()
            .collect();
        self.enroll(sites);
    }
    pub fn enroll(&mut self, sites: &Facilities) {
        let mut added: Vec<_> = sites
            .sites
            .par_iter()
            .filter(|site| {
                self.rules
                    .binary_search_by(|r| r.kind.cmp(&site.kind))
                    .is_ok()
                    && self
                        .conditions
                        .binary_search_by_key(&site.id, |r| r.site)
                        .is_err()
            })
            .map(|site| Condition {
                site: site.id,
                wear: 0,
            })
            .collect();
        if !added.is_empty() {
            self.conditions.append(&mut added);
            self.conditions.par_sort_unstable_by_key(|r| r.site);
        }
    }
    pub fn valid(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        sites: &Facilities,
        tick: u64,
    ) -> bool {
        !self.rules.par_windows(2).any(|w| w[0].kind >= w[1].kind)
            && !self.rules.par_iter().any(|r| {
                r.kind.is_empty()
                    || r.wear_limit == 0
                    || r.wear_per_slot == 0
                    || r.repair_quantity == 0
                    || !goods.contains(&r.repair_good)
            })
            && !self
                .conditions
                .par_windows(2)
                .any(|w| w[0].site >= w[1].site)
            && !self.conditions.par_iter().any(|c| {
                sites
                    .sites
                    .binary_search_by_key(&c.site, |s| s.id)
                    .ok()
                    .and_then(|i| {
                        self.rules
                            .binary_search_by(|r| r.kind.cmp(&sites.sites[i].kind))
                            .ok()
                    })
                    .is_none_or(|i| c.wear > self.rules[i].wear_limit)
            })
            && !self
                .policies
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            && !self.policies.par_iter().any(|p| {
                !people.contains(&p.person)
                    || p.site == 0
                    || p.pressured_by.is_some() != p.pressure_threshold.is_some()
                    || p.pressure_threshold == Some(0)
                    || p.pressured_by
                        .is_some_and(|other| other == p.person || !people.contains(&other))
            })
            && !self
                .events
                .par_iter()
                .any(|e| e.tick > tick || e.site == 0 || !people.contains(&e.person))
    }
    pub fn broken(&self, sites: &Facilities) -> Vec<u64> {
        self.conditions
            .par_iter()
            .filter_map(|c| {
                let site = &sites.sites[index_by(&sites.sites, c.site, |s| s.id)?];
                let rule = self
                    .rules
                    .binary_search_by(|r| r.kind.cmp(&site.kind))
                    .ok()
                    .map(|i| &self.rules[i])?;
                (c.wear >= rule.wear_limit).then_some(c.site)
            })
            .collect()
    }
    pub fn finish(
        &mut self,
        prior: &Facilities,
        current: &Facilities,
        events: &Events,
        repairs: Vec<Event>,
    ) {
        let finished: HashSet<_> = events.finished.par_iter().copied().collect();
        let mut usage: Vec<_> = prior
            .bindings
            .par_iter()
            .filter(|b| finished.contains(&b.job))
            .map(|b| (b.site, b.slots))
            .collect();
        usage.par_sort_unstable_by_key(|r| r.0);
        let totals: Vec<_> = usage
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|rows| {
                (
                    rows[0].0,
                    rows.par_iter()
                        .map(|r| r.1)
                        .reduce(|| 0, u64::saturating_add),
                )
            })
            .collect();
        let mut repaired: Vec<_> = repairs
            .par_iter()
            .filter(|e| e.repaired)
            .map(|e| e.site)
            .collect();
        if !totals.is_empty() {
            self.conditions.par_iter_mut().for_each(|c| {
                if let Ok(i) = totals.binary_search_by_key(&c.site, |r| r.0)
                    && let Ok(j) = prior.sites.binary_search_by_key(&c.site, |s| s.id)
                    && let Ok(k) = self
                        .rules
                        .binary_search_by(|r| r.kind.cmp(&prior.sites[j].kind))
                {
                    let r = &self.rules[k];
                    c.wear = c
                        .wear
                        .saturating_add(totals[i].1.saturating_mul(r.wear_per_slot))
                        .min(r.wear_limit);
                }
            });
        }
        if !repaired.is_empty() {
            if repaired.par_windows(2).any(|w| w[0] > w[1]) {
                repaired.par_sort_unstable();
            }
            let all = repaired.len() == self.conditions.len()
                && repaired
                    .par_iter()
                    .zip(self.conditions.par_iter())
                    .all(|(id, c)| *id == c.site);
            if all {
                self.conditions.par_iter_mut().for_each(|c| c.wear = 0);
            } else {
                self.conditions
                    .par_iter_mut()
                    .filter(|c| repaired.binary_search(&c.site).is_ok())
                    .for_each(|c| c.wear = 0);
            }
        }
        self.events = repairs;
        if prior.sites.len() != current.sites.len()
            || prior
                .sites
                .par_iter()
                .zip(current.sites.par_iter())
                .any(|(a, b)| a.id != b.id || a.kind != b.kind)
        {
            self.sync(current);
        }
    }
}
#[derive(Clone, Copy)]
pub struct Manual {
    pub person: u64,
    pub site: u64,
}
#[derive(Clone, Copy)]
struct Choice {
    person: u64,
    site: u64,
    good: u32,
    quantity: u64,
    priority: u64,
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    pub sites: Vec<u64>,
    rows: Vec<Choice>,
}
pub struct Context<'a> {
    pub owner: &'a Maintenance,
    pub sites: &'a Facilities,
    pub people: &'a [Person],
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
    pub manual: Option<Manual>,
    pub manual_destroy: Option<u64>,
    pub seed: u64,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        if self.owner.policies.is_empty() && self.manual.is_none() {
            return Ok(Plan {
                actions: Vec::new(),
                actors: Vec::new(),
                sites: Vec::new(),
                rows: Vec::new(),
            });
        }
        let occupied: HashSet<_> = self.sites.bindings.par_iter().map(|b| b.site).collect();
        let opinions = self
            .opinions
            .filter(|_| {
                self.owner
                    .policies
                    .par_iter()
                    .any(|p| p.pressured_by.is_some())
            })
            .map(Opinions::index);
        let make = |person: u64, site: u64, controlled: bool, disregard: bool| -> Option<Choice> {
            let p = &self.people[index_by(self.people, person, |p| p.id)?];
            let s = &self.sites.sites[index_by(&self.sites.sites, site, |s| s.id)?];
            let c = &self.owner.conditions[index_by(&self.owner.conditions, site, |c| c.site)?];
            let r = self
                .owner
                .rules
                .binary_search_by(|r| r.kind.cmp(&s.kind))
                .ok()
                .map(|i| &self.owner.rules[i])?;
            if p.controlled != controlled
                || p.health == 0
                || p.hunger > self.max_hunger
                || p.region != Some(s.region)
                || c.wear == 0
                || occupied.contains(&site)
                || self.manual_destroy == Some(site)
                || (!controlled && self.busy.binary_search(&person).is_ok())
                || (!controlled
                    && !disregard
                    && self.claims.is_some_and(|c| !c.allows_site(person, site)))
            {
                return None;
            }
            let mut priority = self.seed ^ self.tick.rotate_left(17) ^ person;
            priority = (priority ^ (priority >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            priority = (priority ^ (priority >> 27)).wrapping_mul(0x94d049bb133111eb);
            priority ^= priority >> 31;
            Some(Choice {
                person,
                site,
                good: r.repair_good,
                quantity: r.repair_quantity,
                priority,
            })
        };
        let mut choices: Vec<_> =
            self.owner
                .policies
                .par_iter()
                .filter_map(|p| {
                    let pressure = p.pressured_by.zip(p.pressure_threshold).is_some_and(
                        |(other, threshold)| {
                            opinions.as_ref().is_some_and(|index| {
                                let (fear, obligation) = index.feelings(p.person, other);
                                fear.saturating_add(obligation) >= threshold
                            })
                        },
                    );
                    (p.willing || pressure)
                        .then(|| make(p.person, p.site, false, p.disregard_claims))
                        .flatten()
                })
                .collect();
        if let Some(m) = self.manual {
            choices.push(
                make(m.person, m.site, true, true).ok_or(
                    "repair requires a living available person and a worn, idle local site",
                )?,
            );
        }
        let order = |c: &Choice| {
            (
                c.site,
                if self.manual.is_some_and(|m| m.person == c.person) {
                    0
                } else {
                    1
                },
                c.priority,
                c.person,
            )
        };
        if choices.par_windows(2).any(|w| order(&w[0]) > order(&w[1])) {
            choices.par_sort_unstable_by_key(order);
        }
        let rows: Vec<_> = choices
            .par_chunk_by(|a, b| a.site == b.site)
            .map(|g| g[0])
            .collect();
        base.checked_add(rows.len() as u64)
            .ok_or("repair action overflow")?;
        let actions = rows
            .par_iter()
            .enumerate()
            .map(|(i, c)| {
                let id = base + i as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        item: None,
                        id,
                        from: Place::Person(c.person),
                        to: None,
                        good: c.good,
                        quantity: c.quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let mut actors: Vec<_> = rows.par_iter().map(|r| r.person).collect();
        actors.par_sort_unstable();
        let sites = rows.par_iter().map(|r| r.site).collect();
        Ok(Plan {
            actions,
            actors,
            sites,
            rows,
        })
    }
}
impl Plan {
    pub fn publish(self, tick: u64, events: &Events) -> Vec<Event> {
        if self.rows.is_empty() {
            return Vec::new();
        }
        // Composed actions often retain their contiguous order. Verify the whole
        // range before using it; mixed boundaries can arrange receipts differently.
        let first = self.actions[0].id;
        if let Some(start) = events
            .physical_actions
            .par_iter()
            .position_any(|r| r.id == first)
            && let Some(end) = start.checked_add(self.actions.len())
            && let Some(receipts) = events.physical_actions.get(start..end)
            && receipts
                .par_iter()
                .zip(self.actions.par_iter())
                .all(|(r, a)| r.id == a.id)
        {
            return self
                .rows
                .into_par_iter()
                .zip(receipts.par_iter())
                .map(|(row, receipt)| Event {
                    tick,
                    person: row.person,
                    site: row.site,
                    repaired: receipt.accepted,
                })
                .collect();
        }
        let accepted: HashSet<_> = events
            .physical_actions
            .par_iter()
            .filter(|r| r.accepted)
            .map(|r| r.id)
            .collect();
        self.rows
            .into_par_iter()
            .zip(self.actions.into_par_iter())
            .map(|(row, action)| Event {
                tick,
                person: row.person,
                site: row.site,
                repaired: accepted.contains(&action.id),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_lots::TransactionOutcome;

    #[test]
    fn ordered_index_checks_dense_and_sparse_ids() {
        let ids = [3, 4, 8, 11];
        for (expected, &id) in ids.iter().enumerate() {
            assert_eq!(index_by(&ids, id, |value| *value), Some(expected));
        }
        for id in [0, 2, 5, 9, 12, u64::MAX] {
            assert_eq!(index_by(&ids, id, |value| *value), None);
        }
        assert_eq!(index_by::<u64>(&[], 3, |value| *value), None);
    }

    fn plan() -> Plan {
        let rows = (1..=2)
            .map(|person| Choice {
                person,
                site: person,
                good: 1,
                quantity: 1,
                priority: 0,
            })
            .collect();
        let actions = (10..=11)
            .map(|id| Transaction {
                id,
                movements: vec![],
                creations: vec![],
            })
            .collect();
        Plan {
            actions,
            rows,
            actors: vec![1, 2],
            sites: vec![1, 2],
        }
    }
    fn events(physical_actions: Vec<TransactionOutcome>) -> Events {
        Events {
            tick: 1,
            started: vec![],
            start_denied: vec![],
            abandoned: vec![],
            finished: vec![],
            delivered: vec![],
            output_lost: vec![],
            interrupted: vec![],
            returned: vec![],
            empty_returns: vec![],
            spoiled: vec![],
            physical_actions,
            physical_movements: Default::default(),
        }
    }
    #[test]
    fn repair_receipts_work_in_order_and_with_shuffled_or_missing_actions() {
        let receipt = |id, accepted| TransactionOutcome { id, accepted };
        let expected = vec![
            Event {
                tick: 1,
                person: 1,
                site: 1,
                repaired: true,
            },
            Event {
                tick: 1,
                person: 2,
                site: 2,
                repaired: false,
            },
        ];
        assert_eq!(
            plan().publish(
                1,
                &events(vec![
                    receipt(2, true),
                    receipt(10, true),
                    receipt(11, false)
                ])
            ),
            expected
        );
        assert_eq!(
            plan().publish(
                1,
                &events(vec![
                    receipt(11, false),
                    receipt(2, true),
                    receipt(10, true)
                ])
            ),
            expected
        );
        assert_eq!(
            plan().publish(1, &events(vec![receipt(10, true)])),
            expected
        );
    }
}
