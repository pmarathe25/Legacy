//! Physical barriers and individually chosen attempts to escape them.
mod autonomous;
pub use autonomous::Context as AutonomousContext;
use game_parallel_economy_person::Person;
use game_parallel_work_sites::Facilities;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn ordered_index<T>(rows: &[T], id: u64, key: impl Fn(&T) -> u64) -> Option<usize> {
    if id < key(rows.first()?) || id > key(rows.last()?) {
        return None;
    }
    rows.first()
        .and_then(|first| id.checked_sub(key(first)))
        .and_then(|offset| usize::try_from(offset).ok())
        .filter(|&index| rows.get(index).is_some_and(|row| key(row) == id))
        .or_else(|| rows.binary_search_by_key(&id, key).ok())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {
    pub site: u64,
    /// Remaining physical resistance; zero is a broken barrier.
    pub resistance: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub person: u64,
    pub site: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EscapePreference {
    pub person: u64,
    pub willing: bool,
    pub effort: u64,
}

/// A person's own inclination to yield; force can succeed regardless of this choice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CaptureResponse {
    pub person: u64,
    pub yield_willingly: bool,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

/// One person's own intention to capture another, optionally motivated by pressure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturePolicy {
    pub person: u64,
    pub target: u64,
    pub site: u64,
    pub willing: bool,
    #[serde(default)]
    pub pressured_by: Option<u64>,
    #[serde(default)]
    pub pressure_threshold: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    pub actor: u64,
    pub target: u64,
    pub site: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub tick: u64,
    pub person: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<u64>,
    pub site: u64,
    pub kind: EventKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    EscapeAttempt,
    Confined,
    Released,
    CaptureResisted,
    CaptureUnavailable,
}

fn default_effort() -> u64 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confinement {
    #[serde(default = "default_effort")]
    pub default_effort: u64,
    pub cells: Vec<Cell>,
    #[serde(default)]
    pub holds: Vec<Hold>,
    #[serde(default)]
    pub escape: Vec<EscapePreference>,
    #[serde(default)]
    pub capture_responses: Vec<CaptureResponse>,
    #[serde(default)]
    pub capture_policies: Vec<CapturePolicy>,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Confinement {
    pub fn normalize(&mut self) {
        self.cells.par_sort_unstable_by_key(|cell| cell.site);
        self.holds.par_sort_unstable_by_key(|hold| hold.person);
        self.escape.par_sort_unstable_by_key(|rule| rule.person);
        self.capture_responses
            .par_sort_unstable_by_key(|rule| rule.person);
        self.capture_policies
            .par_sort_unstable_by_key(|rule| rule.person);
    }

    pub fn validate(&self, people: &[Person], sites: &Facilities, tick: u64) -> Result<(), String> {
        let production_kinds: HashSet<_> = sites
            .requirements
            .iter()
            .map(|rule| rule.kind.as_str())
            .collect();
        let occupied_sites: HashSet<_> = sites
            .bindings
            .par_iter()
            .map(|binding| binding.site)
            .collect();
        if self.default_effort == 0
            || self.cells.par_windows(2).any(|w| w[0].site >= w[1].site)
            || self
                .holds
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self
                .escape
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self
                .capture_responses
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self
                .capture_policies
                .par_windows(2)
                .any(|w| w[0].person >= w[1].person)
            || self.cells.par_iter().any(|cell| {
                sites
                    .sites
                    .binary_search_by_key(&cell.site, |site| site.id)
                    .ok()
                    .is_none_or(|i| production_kinds.contains(sites.sites[i].kind.as_str()))
                    || occupied_sites.contains(&cell.site)
            })
            || self.escape.par_iter().any(|rule| {
                rule.effort == 0 || people.binary_search_by_key(&rule.person, |p| p.id).is_err()
            })
            || self.capture_responses.par_iter().any(|rule| {
                rule.pressure_threshold == Some(0)
                    || people.binary_search_by_key(&rule.person, |p| p.id).is_err()
            })
            || self.capture_policies.par_iter().any(|rule| {
                rule.person == rule.target
                    || people.binary_search_by_key(&rule.person, |p| p.id).is_err()
                    || people.binary_search_by_key(&rule.target, |p| p.id).is_err()
                    || rule.site == 0
                    || rule.site >= sites.next_id
                    || rule.pressured_by.is_some() != rule.pressure_threshold.is_some()
                    || rule.pressure_threshold == Some(0)
                    || rule.pressured_by.is_some_and(|id| {
                        id == rule.person || people.binary_search_by_key(&id, |p| p.id).is_err()
                    })
            })
            || self.holds.par_iter().any(|hold| {
                let person = people
                    .binary_search_by_key(&hold.person, |p| p.id)
                    .ok()
                    .map(|i| &people[i]);
                let site = sites
                    .sites
                    .binary_search_by_key(&hold.site, |s| s.id)
                    .ok()
                    .map(|i| &sites.sites[i]);
                let cell = self
                    .cells
                    .binary_search_by_key(&hold.site, |c| c.site)
                    .ok()
                    .map(|i| &self.cells[i]);
                match (person, site, cell) {
                    (Some(person), Some(site), Some(cell)) => {
                        person.region != Some(site.region) || cell.resistance == 0
                    }
                    _ => true,
                }
            })
            || self.events.par_iter().any(|event| {
                event.tick > tick
                    || people
                        .binary_search_by_key(&event.person, |p| p.id)
                        .is_err()
                    || event
                        .actor
                        .is_some_and(|id| people.binary_search_by_key(&id, |p| p.id).is_err())
            })
        {
            return Err("invalid physical confinement".into());
        }
        let mut counts: Vec<_> = self.holds.par_iter().map(|hold| hold.site).collect();
        counts.par_sort_unstable();
        if counts.par_chunk_by(|a, b| a == b).any(|group| {
            let site = &sites.sites[sites
                .sites
                .binary_search_by_key(&group[0], |site| site.id)
                .expect("validated site")];
            group.len() as u128 > u128::from(site.capacity)
        }) {
            return Err("confinement exceeds physical capacity".into());
        }
        Ok(())
    }

    pub fn held(&self, person: u64) -> bool {
        self.holds
            .binary_search_by_key(&person, |hold| hold.person)
            .is_ok()
    }

    /// Attempt a physical capture; a conscious person can yield, resist, or be overpowered.
    pub fn capture(
        &mut self,
        action: Capture,
        people: &[Person],
        sites: &Facilities,
        busy: &[u64],
        pressured: bool,
        tick: u64,
    ) -> Result<bool, String> {
        let Capture {
            actor,
            target,
            site: site_id,
        } = action;
        let person = &people[people
            .binary_search_by_key(&actor, |p| p.id)
            .map_err(|_| "unknown person")?];
        let target_person = &people[people
            .binary_search_by_key(&target, |p| p.id)
            .map_err(|_| "unknown target")?];
        let site = &sites.sites[sites
            .sites
            .binary_search_by_key(&site_id, |site| site.id)
            .map_err(|_| "unknown cell site")?];
        let cell = &self.cells[self
            .cells
            .binary_search_by_key(&site_id, |cell| cell.site)
            .map_err(|_| "site has no physical barrier")?];
        if actor == target
            || !person.controlled
            || person.health == 0
            || person.region != Some(site.region)
            || target_person.region != Some(site.region)
            || (target_person.health == 0 && target_person.immortal.is_none())
            || cell.resistance == 0
            || self.held(target)
            || self
                .holds
                .iter()
                .filter(|hold| hold.site == site_id)
                .count() as u128
                >= u128::from(site.capacity)
        {
            return Err("capture requires an available local cell and a living target".into());
        }
        if target_person.health > 0 {
            let response = ordered_index(&self.capture_responses, target, |row| row.person)
                .map(|index| &self.capture_responses[index]);
            let effort = |id| {
                ordered_index(&self.escape, id, |row| row.person)
                    .map_or(self.default_effort, |index| self.escape[index].effort)
            };
            let kind = if busy.binary_search(&target).is_ok() {
                Some(EventKind::CaptureUnavailable)
            } else if !response.is_some_and(|row| row.yield_willingly)
                && !pressured
                && effort(actor) <= effort(target)
            {
                Some(EventKind::CaptureResisted)
            } else {
                None
            };
            if let Some(kind) = kind {
                self.events.push(Event {
                    tick,
                    person: target,
                    actor: Some(actor),
                    site: site_id,
                    kind,
                });
                return Ok(false);
            }
        }
        self.holds.push(Hold {
            person: target,
            site: site_id,
        });
        self.holds.par_sort_unstable_by_key(|hold| hold.person);
        self.events.push(Event {
            tick,
            person: target,
            actor: Some(actor),
            site: site_id,
            kind: EventKind::Confined,
        });
        Ok(true)
    }

    /// Any capable local person can physically open the cell.
    pub fn release(
        &mut self,
        actor: u64,
        target: u64,
        people: &[Person],
        sites: &Facilities,
        tick: u64,
    ) -> Result<(), String> {
        let person = &people[people
            .binary_search_by_key(&actor, |p| p.id)
            .map_err(|_| "unknown person")?];
        let index = self
            .holds
            .binary_search_by_key(&target, |hold| hold.person)
            .map_err(|_| "person is not held")?;
        let site_id = self.holds[index].site;
        let site = &sites.sites[sites
            .sites
            .binary_search_by_key(&site_id, |site| site.id)
            .map_err(|_| "unknown cell site")?];
        if !person.controlled
            || person.health == 0
            || person.region != Some(site.region)
            || self.held(actor)
        {
            return Err("release requires a capable person outside the local cell".into());
        }
        self.holds.remove(index);
        self.events.push(Event {
            tick,
            person: target,
            actor: Some(actor),
            site: site_id,
            kind: EventKind::Released,
        });
        Ok(())
    }

    /// Disjoint barrier groups resolve concurrently; broken barriers free everyone behind them.
    pub fn escape_attempts(
        &self,
        people: &[Person],
        busy: &[u64],
        tick: u64,
        manual: Option<u64>,
    ) -> Result<(Self, Vec<u64>), String> {
        if let Some(person) = manual {
            let index = people
                .binary_search_by_key(&person, |p| p.id)
                .map_err(|_| "unknown escapee")?;
            if !people[index].controlled || people[index].health == 0 || !self.held(person) {
                return Err("escape requires a capable held person".into());
            }
        }
        let mut attempts: Vec<_> = self
            .holds
            .par_iter()
            .filter_map(|hold| {
                let person = &people[ordered_index(people, hold.person, |p| p.id)?];
                let rule =
                    ordered_index(&self.escape, hold.person, |r| r.person).map(|i| &self.escape[i]);
                (person.health > 0
                    && (Some(person.id) == manual
                        || (!person.controlled
                            && rule.is_some_and(|rule| rule.willing)
                            && busy.binary_search(&person.id).is_err())))
                .then_some((
                    hold.site,
                    hold.person,
                    rule.map_or(self.default_effort, |rule| rule.effort),
                ))
            })
            .collect();
        if attempts
            .par_windows(2)
            .any(|pair| (pair[0].0, pair[0].1) > (pair[1].0, pair[1].1))
        {
            attempts.par_sort_unstable_by_key(|row| (row.0, row.1));
        }
        let changes: Vec<_> = attempts
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                let effort = group
                    .par_iter()
                    .map(|row| row.2)
                    .reduce(|| 0, u64::saturating_add);
                (group[0].0, effort)
            })
            .collect();
        let mut next = Self {
            default_effort: self.default_effort,
            cells: self.cells.par_iter().cloned().collect(),
            holds: self.holds.par_iter().cloned().collect(),
            escape: self.escape.par_iter().cloned().collect(),
            capture_responses: self.capture_responses.par_iter().cloned().collect(),
            capture_policies: self.capture_policies.par_iter().cloned().collect(),
            events: attempts
                .par_iter()
                .map(|&(site, person, _)| Event {
                    tick,
                    person,
                    actor: Some(person),
                    site,
                    kind: EventKind::EscapeAttempt,
                })
                .collect(),
        };
        next.cells.par_iter_mut().for_each(|cell| {
            if let Ok(index) = changes.binary_search_by_key(&cell.site, |row| row.0) {
                cell.resistance = cell.resistance.saturating_sub(changes[index].1);
            }
        });
        let broken: HashSet<_> = next
            .cells
            .par_iter()
            .filter(|cell| cell.resistance == 0)
            .map(|cell| cell.site)
            .collect();
        next.holds = next
            .holds
            .into_par_iter()
            .filter(|hold| !broken.contains(&hold.site))
            .collect();
        Ok((next, attempts.into_par_iter().map(|row| row.1).collect()))
    }

    /// Physical destruction of a site removes its barrier and releases its occupants.
    pub fn retain_sites(&mut self, sites: &Facilities) {
        if self.cells.par_iter().all(|cell| {
            sites
                .sites
                .binary_search_by_key(&cell.site, |site| site.id)
                .is_ok()
        }) {
            return;
        }
        self.cells = self
            .cells
            .par_iter()
            .filter(|cell| {
                sites
                    .sites
                    .binary_search_by_key(&cell.site, |site| site.id)
                    .is_ok()
            })
            .cloned()
            .collect();
        self.holds = self
            .holds
            .par_iter()
            .filter(|hold| {
                self.cells
                    .binary_search_by_key(&hold.site, |cell| cell.site)
                    .is_ok()
            })
            .cloned()
            .collect();
    }
}

#[macro_export]
macro_rules! escape_method {
    () => {
        /// Change an individual's inclination to carry out their own capture intention.
        pub fn set_capture_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
            if self
                .people
                .binary_search_by_key(&person, |row| row.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            {
                return Err("person must be capable".into());
            }
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            let owner = self.confinement.as_mut().ok_or("confinement is disabled")?;
            let index = owner
                .capture_policies
                .binary_search_by_key(&person, |row| row.person)
                .map_err(|_| "person has no capture preference")?;
            owner.capture_policies[index].willing = willing;
            self.epoch = epoch;
            Ok(())
        }
        /// Change only this person's own inclination to yield to capture.
        pub fn set_capture_yield(&mut self, person: u64, willing: bool) -> Result<(), String> {
            if self
                .people
                .binary_search_by_key(&person, |row| row.id)
                .ok()
                .is_none_or(|index| self.people[index].health == 0)
            {
                return Err("person must be capable".into());
            }
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            let owner = self.confinement.as_mut().ok_or("confinement is disabled")?;
            match owner
                .capture_responses
                .binary_search_by_key(&person, |row| row.person)
            {
                Ok(index) => owner.capture_responses[index].yield_willingly = willing,
                Err(index) => owner.capture_responses.insert(
                    index,
                    $crate::CaptureResponse {
                        person,
                        yield_willingly: willing,
                        pressure_threshold: None,
                    },
                ),
            }
            self.epoch = epoch;
            Ok(())
        }
        /// Spend a boundary on this person's own attempt to break their barrier.
        pub fn escape_confinement(&mut self, person: u64) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some(game_parallel_economy_interaction::PhysicalChoice::Escape { person }),
            )?;
            let escaped = draft
                .confinement
                .as_ref()
                .is_some_and(|owner| !owner.held(person));
            self.commit(draft)?;
            Ok(escaped)
        }
        /// Attempt to place a living person into an available cell.
        pub fn confine_person(
            &mut self,
            person: u64,
            target: u64,
            site: u64,
        ) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some(game_parallel_economy_interaction::PhysicalChoice::Confine {
                    person,
                    target,
                    site,
                }),
            )?;
            let confined = draft
                .confinement
                .as_ref()
                .is_some_and(|owner| owner.held(target));
            self.commit(draft)?;
            Ok(confined)
        }
        /// Physically open the cell containing the target.
        pub fn release_held_person(&mut self, person: u64, target: u64) -> Result<bool, String> {
            game_parallel_economy_campaign::preflight!(self);
            let draft = self.prepare_interaction(
                None,
                None,
                None,
                Some(
                    game_parallel_economy_interaction::PhysicalChoice::ReleaseHeld {
                        person,
                        target,
                    },
                ),
            )?;
            let released = draft
                .confinement
                .as_ref()
                .is_some_and(|owner| !owner.held(target));
            self.commit(draft)?;
            Ok(released)
        }
    };
}

#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
