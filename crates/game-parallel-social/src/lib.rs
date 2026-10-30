//! Local, consenting encounters and persistent, undirected familiarity.
pub mod adapter;
pub mod wellbeing;
use game_parallel_social_pressure::priority;
pub use game_parallel_social_pressure::{Contact, Encounter, Policy as PressurePolicy};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
pub use wellbeing::{Need, NeedRules, Needs};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Social {
    pub rules: Rules,
    pub participants: Vec<Participant>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pressures: Vec<PressurePolicy>,
    #[serde(default)]
    pub relationships: Vec<Relationship>,
    #[serde(default)]
    pub events: Vec<Encounter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs: Option<Needs>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub interval: u64,
    pub min_health: u32,
    pub max_hunger: u32,
    pub familiarity_gain: u32,
    pub familiarity_max: u32,
    /// Forget relationships after this many ticks without a meeting; omitted means unbounded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship_retention: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub person: u64,
    pub willing: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relationship {
    pub a: u64,
    pub b: u64,
    pub familiarity: u32,
    pub encounters: u64,
    pub last_met: u64,
}

pub struct Selection {
    pub events: Vec<Encounter>,
    pub needs: Option<Needs>,
}
impl Social {
    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), &'static str> {
        if self.rules.interval == 0
            || self.rules.familiarity_gain == 0
            || self.rules.familiarity_max == 0
        {
            return Err("invalid social rules");
        }
        let mut participants = HashSet::new();
        for p in &self.participants {
            if !people.contains(&p.person) || !participants.insert(p.person) {
                return Err("invalid social participant");
            }
        }
        game_parallel_social_pressure::validate(&self.pressures, &participants)?;
        if let Some(needs) = &self.needs {
            needs.validate(&participants)?;
        }
        let mut pairs = HashSet::new();
        for r in &self.relationships {
            if r.a >= r.b
                || !participants.contains(&r.a)
                || !participants.contains(&r.b)
                || !pairs.insert((r.a, r.b))
                || r.familiarity > self.rules.familiarity_max
                || r.last_met > now
            {
                return Err("invalid social relationship");
            }
        }
        if self.events.iter().any(|e| {
            e.tick > now
                || e.a >= e.b
                || !participants.contains(&e.a)
                || !participants.contains(&e.b)
                || e.pressured
                    .is_some_and(|person| person != e.a && person != e.b)
        }) {
            return Err("invalid social event");
        }
        Ok(())
    }
    fn validate_contacts(&self, contacts: &[Contact], now: u64) -> Result<(), &'static str> {
        if self.rules.interval == 0
            || self.rules.familiarity_gain == 0
            || self.rules.familiarity_max == 0
        {
            return Err("invalid social rules");
        }
        let mut people: Vec<_> = contacts.par_iter().map(|c| c.person).collect();
        people.par_sort_unstable();
        if people.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("duplicate contact person");
        }
        let mut participants: Vec<_> = self.participants.par_iter().map(|p| p.person).collect();
        participants.par_sort_unstable();
        if participants.par_windows(2).any(|p| p[0] == p[1])
            || participants
                .par_iter()
                .any(|p| people.binary_search(p).is_err())
        {
            return Err("invalid social participant");
        }
        if let Some(needs) = &self.needs {
            needs.validate_sorted(&participants)?;
        }
        if self.relationships.par_iter().any(|r| {
            r.a >= r.b
                || participants.binary_search(&r.a).is_err()
                || participants.binary_search(&r.b).is_err()
                || r.familiarity > self.rules.familiarity_max
                || r.last_met > now
        }) {
            return Err("invalid social relationship");
        }
        let mut pairs: Vec<_> = self.relationships.par_iter().map(|r| (r.a, r.b)).collect();
        if pairs.par_windows(2).any(|p| p[0] > p[1]) {
            pairs.par_sort_unstable();
        }
        if pairs.par_windows(2).any(|p| p[0] == p[1]) {
            return Err("invalid social relationship");
        }
        if self.events.par_iter().any(|e| {
            e.tick > now
                || e.a >= e.b
                || participants.binary_search(&e.a).is_err()
                || participants.binary_search(&e.b).is_err()
        }) {
            return Err("invalid social event");
        }
        Ok(())
    }
    /// Prepare one boundary from an immutable snapshot; errors publish nothing.
    pub fn prepare(
        &self,
        contacts: &[Contact],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        let now = tick.checked_sub(1).ok_or("invalid social tick")?;
        self.validate_contacts(contacts, now)?;
        self.prepare_validated(contacts, seed, tick)
    }
    /// The host guarantees identity and social-owner invariants at import and
    /// after each checked command. Transaction preparation can avoid revalidating
    /// the entire unchanged relationship graph every tick.
    pub fn prepare_validated(
        &self,
        contacts: &[Contact],
        seed: u64,
        tick: u64,
    ) -> Result<Self, &'static str> {
        let selection = self.select_validated(contacts, seed, tick, |_, _| true)?;
        self.prepare_selection_validated(selection, tick)
    }
    /// Publish a host-validated, disjoint selection of actual personal encounters.
    pub fn prepare_selection_validated(
        &self,
        selection: Selection,
        tick: u64,
    ) -> Result<Self, &'static str> {
        let mut next = Self {
            rules: self.rules.clone(),
            participants: self.participants.par_iter().cloned().collect(),
            pressures: self.pressures.par_iter().cloned().collect(),
            relationships: Vec::new(),
            events: Vec::new(),
            needs: selection.needs,
        };
        if !tick.is_multiple_of(self.rules.interval) && selection.events.is_empty() {
            next.relationships = self
                .relationships
                .par_iter()
                .filter(|relationship| {
                    self.rules.relationship_retention.is_none_or(|retention| {
                        tick.saturating_sub(relationship.last_met) <= retention
                    })
                })
                .cloned()
                .collect();
            return Ok(next);
        }
        let events = selection.events;
        let retained: Option<Vec<_>> = self.rules.relationship_retention.map(|retention| {
            self.relationships
                .par_iter()
                .filter(|relationship| tick.saturating_sub(relationship.last_met) <= retention)
                .cloned()
                .collect()
        });
        let current = retained.as_deref().unwrap_or(&self.relationships);
        let ordered = if current
            .par_windows(2)
            .any(|pair| (pair[0].a, pair[0].b) > (pair[1].a, pair[1].b))
        {
            let mut rows = current.to_vec();
            rows.par_sort_unstable_by_key(|r| (r.a, r.b));
            Some(rows)
        } else {
            None
        };
        let relationships = ordered.as_deref().unwrap_or(current);
        if relationships.par_iter().any(|r| {
            r.encounters == u64::MAX
                && events
                    .binary_search_by_key(&(r.a, r.b), |e| (e.a, e.b))
                    .is_ok()
        }) {
            return Err("social encounter counter overflow");
        }
        // Partition the sorted event stream into disjoint key ranges. Each
        // worker merges its existing relationships and new meetings directly,
        // so a crowded region does not funnel the graph through a serial map.
        next.relationships = if events.is_empty() {
            relationships.par_iter().cloned().collect()
        } else {
            events
                .par_chunks(4096)
                .enumerate()
                .flat_map_iter(|(chunk_index, chunk)| {
                    let start = if chunk_index == 0 {
                        0
                    } else {
                        relationships.partition_point(|r| (r.a, r.b) < (chunk[0].a, chunk[0].b))
                    };
                    let end = if (chunk_index + 1) * 4096 >= events.len() {
                        relationships.len()
                    } else {
                        let boundary = &events[(chunk_index + 1) * 4096];
                        relationships.partition_point(|r| (r.a, r.b) < (boundary.a, boundary.b))
                    };
                    let mut merged = Vec::with_capacity(end - start + chunk.len());
                    let mut old = start;
                    for event in chunk {
                        let key = (event.a, event.b);
                        while old < end && (relationships[old].a, relationships[old].b) < key {
                            merged.push(relationships[old].clone());
                            old += 1;
                        }
                        if old < end && (relationships[old].a, relationships[old].b) == key {
                            let mut relationship = relationships[old].clone();
                            relationship.familiarity = relationship
                                .familiarity
                                .saturating_add(self.rules.familiarity_gain)
                                .min(self.rules.familiarity_max);
                            relationship.encounters += 1;
                            relationship.last_met = tick;
                            merged.push(relationship);
                            old += 1;
                        } else {
                            merged.push(Relationship {
                                a: event.a,
                                b: event.b,
                                familiarity: self
                                    .rules
                                    .familiarity_gain
                                    .min(self.rules.familiarity_max),
                                encounters: 1,
                                last_met: tick,
                            });
                        }
                    }
                    merged.extend_from_slice(&relationships[old..end]);
                    merged
                })
                .collect()
        };
        next.events = events;
        Ok(next)
    }
    /// Select contacts and stage personal needs without cloning the relationship graph.
    /// The host must stage edge accounting separately before publishing this selection.
    /// Apply personal participation choices without changing social need accounting.
    pub fn select_validated(
        &self,
        contacts: &[Contact],
        seed: u64,
        tick: u64,
        allowed: impl Fn(u64, u32) -> bool + Sync,
    ) -> Result<Selection, &'static str> {
        tick.checked_sub(1).ok_or("invalid social tick")?;
        if !tick.is_multiple_of(self.rules.interval) {
            return Ok(Selection {
                events: Vec::new(),
                needs: self
                    .needs
                    .as_ref()
                    .map(|needs| needs.advance(&self.participants, contacts, &[])),
            });
        }
        let mut willing: Vec<_> = self
            .participants
            .par_iter()
            .filter(|p| p.willing)
            .map(|p| p.person)
            .collect();
        if willing.par_windows(2).any(|pair| pair[0] > pair[1]) {
            willing.par_sort_unstable();
        }
        let needs = self.needs.as_ref().map(Needs::index).unwrap_or_default();
        let ordered = if contacts
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            let mut rows = contacts.to_vec();
            rows.par_sort_unstable_by_key(|contact| contact.person);
            Some(rows)
        } else {
            None
        };
        let contacts = ordered.as_deref().unwrap_or(contacts);
        let willing = &willing;
        let needs = &needs;
        let allowed = &allowed;
        let mut local: Vec<_> = contacts
            .par_chunks(4096)
            .flat_map_iter(|chunk| {
                let mut willing_index = willing.partition_point(|&id| id < chunk[0].person);
                let mut need_index = needs.partition_point(|row| row.0 < chunk[0].person);
                chunk.iter().filter_map(move |c| {
                    while willing_index < willing.len() && willing[willing_index] < c.person {
                        willing_index += 1;
                    }
                    while need_index < needs.len() && needs[need_index].0 < c.person {
                        need_index += 1;
                    }
                    if c.busy
                        || c.health == 0
                        || c.health < self.rules.min_health
                        || c.hunger > self.rules.max_hunger
                        || willing.get(willing_index) != Some(&c.person)
                    {
                        return None;
                    }
                    let isolation = needs
                        .get(need_index)
                        .filter(|row| row.0 == c.person)
                        .map_or(0, |row| row.1);
                    if !allowed(c.person, isolation) {
                        return None;
                    }
                    c.region.map(|region| {
                        (
                            region,
                            std::cmp::Reverse(isolation),
                            priority(seed, tick, c.person),
                            c.person,
                        )
                    })
                })
            })
            .collect();
        local.par_sort_unstable();
        let mut events: Vec<Encounter> = local
            .par_chunk_by(|a, b| a.0 == b.0)
            .flat_map(|group| {
                group.par_chunks_exact(2).map(|pair| Encounter {
                    tick,
                    region: pair[0].0,
                    a: pair[0].3.min(pair[1].3),
                    b: pair[0].3.max(pair[1].3),
                    pressured: None,
                })
            })
            .collect();
        events.par_sort_unstable_by_key(|e| (e.a, e.b));
        let needs = self
            .needs
            .as_ref()
            .map(|needs| needs.advance(&self.participants, contacts, &events));
        Ok(Selection { events, needs })
    }
}
