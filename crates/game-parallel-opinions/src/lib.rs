//! Directed personal trust, updated by directly experienced promises.
use game_parallel_agreements::{Contract, Event as AgreementEvent, EventKind};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
mod harm;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opinions {
    pub rules: Rules,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sensitivities: Vec<Sensitivity>,
    #[serde(default)]
    pub records: Vec<Opinion>,
    #[serde(default)]
    pub events: Vec<Event>,
}
pub use game_parallel_opinions_model::{
    AffinityRules, Confrontation, Event, ObligationDevelopment, Opinion, RelationshipCase, Rules,
    Sensitivity,
};

pub struct Index<'a> {
    rules: &'a Rules,
    records: Vec<&'a Opinion>,
}
impl Index<'_> {
    pub fn assessment(&self, observer: u64, target: u64) -> (i32, u64) {
        self.records
            .binary_search_by_key(&(observer, target), |row| (row.observer, row.target))
            .ok()
            .map_or((self.rules.initial, 0), |i| {
                (self.records[i].trust, self.records[i].fear)
            })
    }
    pub fn feelings(&self, observer: u64, target: u64) -> (u64, u64) {
        self.records
            .binary_search_by_key(&(observer, target), |row| (row.observer, row.target))
            .ok()
            .map_or((0, 0), |i| {
                (self.records[i].fear, self.records[i].obligation)
            })
    }
    pub fn fear(&self, observer: u64, target: u64) -> u64 {
        self.feelings(observer, target).0
    }
    pub fn obligation(&self, observer: u64, target: u64) -> u64 {
        self.feelings(observer, target).1
    }
    pub fn affinity(&self, observer: u64, target: u64) -> i32 {
        self.records
            .binary_search_by_key(&(observer, target), |row| (row.observer, row.target))
            .ok()
            .map_or(0, |i| self.records[i].affinity)
    }
    pub fn trust(&self, observer: u64, target: u64) -> i32 {
        self.records
            .binary_search_by_key(&(observer, target), |row| (row.observer, row.target))
            .ok()
            .map_or(self.rules.initial, |i| self.records[i].trust)
    }
}
impl Opinions {
    /// Time updates each person's existing fear independently of trust and obligations.
    pub fn decay_fear(&mut self) {
        let decay = self.rules.fear_decay_per_tick;
        if decay > 0 && self.records.par_iter().any(|row| row.fear > 0) {
            self.records
                .par_iter_mut()
                .for_each(|row| row.fear = row.fear.saturating_sub(decay));
        }
    }
    /// Apply a simultaneous set of directly witnessed takings by directed pair.
    pub fn witness_thefts(&mut self, incidents: &[(u64, u64)], tick: u64) {
        harm::theft(self, incidents, tick);
    }
    pub fn tracks_affinity_help(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::Help)
    }
    pub fn tracks_affinity_encounters(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::Encounter)
            || harm::tracks_affinity(self, harm::AffinityKind::PressuredEncounter)
    }
    pub fn tracks_site_affinity(&self) -> bool {
        harm::tracks_affinity(self, harm::AffinityKind::SiteUse)
            || harm::tracks_affinity(self, harm::AffinityKind::SiteDestruction)
    }
    /// Update only from each observer's signed interpretation of witnessed site actions.
    pub fn learn_site_affinity(&mut self, events: &[(u64, u64, bool)]) {
        if !self.tracks_site_affinity() {
            return;
        }
        let use_events: Vec<_> = events
            .par_iter()
            .filter(|e| !e.2)
            .map(|e| (e.0, e.1, 1))
            .collect();
        let destruction: Vec<_> = events
            .par_iter()
            .filter(|e| e.2)
            .map(|e| (e.0, e.1, 1))
            .collect();
        harm::affinity(self, &use_events, harm::AffinityKind::SiteUse);
        harm::affinity(self, &destruction, harm::AffinityKind::SiteDestruction);
    }
    pub fn tracks_harm(&self) -> bool {
        self.rules.fear_per_damage > 0
            || harm::tracks_affinity(self, harm::AffinityKind::Harm)
            || self
                .sensitivities
                .par_iter()
                .any(|row| row.fear_per_damage.is_some_and(|value| value > 0))
    }
    pub fn tracks_help(&self) -> bool {
        self.rules.gratitude_gain > 0
            || self.tracks_affinity_help()
            || self.rules.obligation_relief > 0
            || self.sensitivities.par_iter().any(|row| {
                row.gratitude_gain.is_some_and(|value| value > 0)
                    || row.obligation_relief.is_some_and(|value| value > 0)
                    || row.relationship_cases.iter().any(|case| {
                        case.obligation_gain.is_some_and(|n| n > 0)
                            || case.obligation_relief.is_some_and(|n| n > 0)
                    })
            })
    }
    pub fn tracks_promise_obligation(&self) -> bool {
        self.sensitivities.par_iter().any(|person| {
            person.promise_obligation_gain.is_some_and(|n| n > 0)
                || person
                    .relationship_cases
                    .iter()
                    .any(|case| case.promise_obligation_gain.is_some_and(|n| n > 0))
        })
    }
    /// A promise changes feelings only where the maker's own disposition says it does.
    pub fn learn_promises(&mut self, promises: &[(u64, u64)]) {
        if !promises.is_empty() && self.tracks_promise_obligation() {
            harm::promises(self, promises);
        }
    }
    /// Attach only an authored developmental disposition to actual newborns.
    pub fn enroll_newborn_development(&mut self, newborns: &[u64]) {
        let Some(rule) = self.rules.newborn_obligation_development.as_ref() else {
            return;
        };
        if newborns.is_empty() {
            return;
        }
        let mut existing: Vec<_> = self.sensitivities.par_iter().map(|p| p.person).collect();
        existing.par_sort_unstable();
        let mut newborns = newborns.to_vec();
        newborns.par_sort_unstable();
        let added: Vec<_> = newborns
            .par_chunk_by(|a, b| a == b)
            .map(|group| group[0])
            .filter(|id| existing.binary_search(id).is_err())
            .map(|person| Sensitivity {
                person,
                fear_per_damage: None,
                gratitude_gain: None,
                promise_obligation_gain: None,
                obligation_relief: None,
                theft_loss: None,
                confrontation: None,
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                affinity_site_use: None,
                affinity_site_destruction: None,
                relationship_cases: vec![],
                obligation_development: Some(rule.clone()),
            })
            .collect();
        self.sensitivities.par_extend(added);
    }
    pub fn tracks_obligation_development(&self) -> bool {
        self.sensitivities
            .par_iter()
            .any(|p| p.obligation_development.is_some())
    }
    /// Each configured child interprets actual caregiving or absence according to their own disposition.
    pub fn develop_obligation(&mut self, experiences: &[(u64, u64, bool)]) {
        if experiences.is_empty() || !self.tracks_obligation_development() {
            return;
        }
        let mut experiences = experiences.to_vec();
        if experiences.par_windows(2).any(|rows| rows[0].0 > rows[1].0) {
            experiences.par_sort_unstable_by_key(|e| e.0);
        }
        let dense = experiences
            .first()
            .zip(experiences.last())
            .and_then(|(first, last)| {
                (last.0.checked_sub(first.0) == Some((experiences.len() - 1) as u64)
                    && experiences
                        .par_windows(2)
                        .all(|rows| rows[0].0.checked_add(1) == Some(rows[1].0)))
                .then_some(first.0)
            });
        let default = self.rules.gratitude_gain;
        self.sensitivities.par_iter_mut().for_each(|person| {
            let Some(rule) = &person.obligation_development else {
                return;
            };
            let Some(index) = dense
                .and_then(|first| person.person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&i| i < experiences.len())
                .or_else(|| {
                    dense
                        .is_none()
                        .then(|| {
                            experiences
                                .binary_search_by_key(&person.person, |e| e.0)
                                .ok()
                        })
                        .flatten()
                })
            else {
                return;
            };
            let (_, age, cared) = experiences[index];
            if age >= rule.until_age {
                return;
            }
            let delta = if cared {
                rule.cared_change
            } else {
                rule.unattended_change
            };
            let current = i128::from(person.gratitude_gain.unwrap_or(default));
            person.gratitude_gain = Some(
                (current + i128::from(delta))
                    .clamp(i128::from(rule.minimum), i128::from(rule.maximum))
                    as u64,
            );
            let promise_delta = if cared {
                rule.cared_promise_change
            } else {
                rule.unattended_promise_change
            };
            if let Some(delta) = promise_delta {
                let current = i128::from(person.promise_obligation_gain.unwrap_or(0));
                person.promise_obligation_gain = Some(
                    (current + i128::from(delta))
                        .clamp(i128::from(rule.minimum), i128::from(rule.maximum))
                        as u64,
                );
            }
        });
    }
    pub fn reset_for_generation(&mut self) {
        self.records.clear();
        self.events.clear();
        self.sensitivities.clear();
    }
    pub fn learn_harm(&mut self, harm: &[(u64, u64, u32)]) {
        harm::apply(self, harm);
        if harm::tracks_affinity(self, harm::AffinityKind::Harm) {
            harm::affinity(
                self,
                &harm
                    .par_iter()
                    .map(|&(a, b, n)| (a, b, u64::from(n)))
                    .collect::<Vec<_>>(),
                harm::AffinityKind::Harm,
            );
        }
    }
    pub fn learn_help(&mut self, help: &[(u64, u64)]) {
        harm::help(self, help);
        self.learn_affinity_help(help);
    }
    /// A personal interpretation of receiving a gift, whether or not it relieved hunger.
    pub fn learn_affinity_help(&mut self, help: &[(u64, u64)]) {
        if self.tracks_affinity_help() {
            harm::affinity(
                self,
                &help.par_iter().map(|&(a, b)| (a, b, 1)).collect::<Vec<_>>(),
                harm::AffinityKind::Help,
            );
        }
    }
    pub fn validate(&self, people: &HashSet<u64>, now: u64) -> Result<(), &'static str> {
        let r = &self.rules;
        if r.newborn_obligation_development
            .as_ref()
            .is_some_and(|rule| rule.until_age == 0 || rule.minimum > rule.maximum)
            || r.affinity.as_ref().is_some_and(|a| a.limit <= 0)
            || r.minimum > r.maximum
            || r.initial < r.minimum
            || r.initial > r.maximum
            || r.minimum_report_trust
                .is_some_and(|threshold| threshold < r.minimum || threshold > r.maximum)
        {
            return Err("invalid trust rules");
        }
        let mut ids: Vec<_> = self
            .records
            .par_iter()
            .map(|row| (row.observer, row.target))
            .collect();
        ids.par_sort_unstable();
        let mut sensitivities: Vec<_> = self
            .sensitivities
            .par_iter()
            .map(|row| row.person)
            .collect();
        sensitivities.par_sort_unstable();
        if sensitivities.par_windows(2).any(|row| row[0] == row[1])
            || sensitivities
                .par_iter()
                .any(|person| !people.contains(person))
            || ids.par_windows(2).any(|p| p[0] == p[1])
            || self.sensitivities.par_iter().any(|row| {
                let mut targets = HashSet::new();
                row.relationship_cases.iter().any(|case| {
                    case.target == row.person
                        || !people.contains(&case.target)
                        || !targets.insert(case.target)
                }) || row
                    .obligation_development
                    .as_ref()
                    .is_some_and(|rule| rule.until_age == 0 || rule.minimum > rule.maximum)
                    || row.confrontation.as_ref().is_some_and(|policy| {
                        policy.interval == 0
                            || policy.trust_at_most < r.minimum
                            || policy.trust_at_most > r.maximum
                    })
            })
            || self.records.par_iter().any(|row| {
                row.observer == row.target
                    || !people.contains(&row.observer)
                    || !people.contains(&row.target)
                    || row.trust < r.minimum
                    || row.trust > r.maximum
                    || r.affinity
                        .as_ref()
                        .is_some_and(|a| row.affinity < -a.limit || row.affinity > a.limit)
            })
            || self.events.par_iter().any(|event| {
                event.tick > now
                    || event.observer == event.target
                    || !people.contains(&event.observer)
                    || !people.contains(&event.target)
                    || event.before < r.minimum
                    || event.before > r.maximum
                    || event.after < r.minimum
                    || event.after > r.maximum
            })
        {
            return Err("invalid trust state");
        }
        Ok(())
    }
    /// Record a directly witnessed violation without changing anyone else's opinion.
    pub fn witness_theft(&mut self, observer: u64, target: u64, tick: u64) {
        self.witness_thefts(&[(observer, target)], tick);
    }
    pub fn index(&self) -> Index<'_> {
        let mut records: Vec<_> = self.records.par_iter().collect();
        if records
            .par_windows(2)
            .any(|p| (p[0].observer, p[0].target) > (p[1].observer, p[1].target))
        {
            records.par_sort_unstable_by_key(|r| (r.observer, r.target));
        }
        Index {
            rules: &self.rules,
            records,
        }
    }
    /// The host validates identities and contract/event links on import; all effects
    /// in this boundary are combined before the bounded trust update.
    pub fn advance_validated(
        &mut self,
        contracts: &[Contract],
        events: &[AgreementEvent],
        tick: u64,
    ) {
        self.advance_with_social(contracts, events, &[], tick);
    }
    /// Combine agreement outcomes and encounters without rewarding pressured participants.
    pub fn advance_with_social(
        &mut self,
        contracts: &[Contract],
        events: &[AgreementEvent],
        encounters: &[game_parallel_social::Encounter],
        tick: u64,
    ) {
        if self.tracks_affinity_encounters() && !encounters.is_empty() {
            let unpressured: Vec<_> = encounters
                .par_iter()
                .flat_map_iter(|event| {
                    [(event.a, event.b, 1), (event.b, event.a, 1)]
                        .into_iter()
                        .filter(|row| event.pressured != Some(row.0))
                })
                .collect();
            let pressured: Vec<_> = encounters
                .par_iter()
                .filter_map(|event| {
                    event.pressured.map(|person| {
                        (person, if person == event.a { event.b } else { event.a }, 1)
                    })
                })
                .collect();
            harm::affinity(self, &unpressured, harm::AffinityKind::Encounter);
            harm::affinity(self, &pressured, harm::AffinityKind::PressuredEncounter);
        }
        if (self.rules.encounter_gain == 0 || encounters.is_empty())
            && events
                .par_iter()
                .all(|event| !matches!(event.kind, EventKind::Fulfilled | EventKind::Defaulted))
        {
            self.events.clear();
            return;
        }
        let ordered = if contracts.par_windows(2).any(|p| p[0].id > p[1].id) {
            let mut rows: Vec<_> = contracts.par_iter().collect();
            rows.par_sort_unstable_by_key(|c| c.id);
            Some(rows)
        } else {
            None
        };
        let dense = if ordered.is_none() {
            contracts.first().and_then(|first| {
                contracts.last().and_then(|last| {
                    (last.id.checked_sub(first.id) == Some((contracts.len() - 1) as u64))
                        .then_some(first.id)
                })
            })
        } else {
            None
        };
        let find = |id: u64| -> &Contract {
            if let Some(rows) = &ordered {
                rows[rows
                    .binary_search_by_key(&id, |row| row.id)
                    .expect("validated agreement event")]
            } else {
                let index = dense
                    .and_then(|first| id.checked_sub(first))
                    .and_then(|n| usize::try_from(n).ok())
                    .filter(|&i| i < contracts.len())
                    .or_else(|| {
                        if dense.is_none() {
                            contracts.binary_search_by_key(&id, |c| c.id).ok()
                        } else {
                            None
                        }
                    })
                    .expect("validated agreement event");
                &contracts[index]
            }
        };
        let mut changes: Vec<_> = events
            .par_iter()
            .filter_map(|event| {
                let delta = match event.kind {
                    EventKind::Fulfilled => i128::from(self.rules.fulfilled_gain),
                    EventKind::Defaulted => -i128::from(self.rules.default_loss),
                    _ => return None,
                };
                let contract = find(event.contract);
                Some(((contract.creditor, contract.debtor), delta))
            })
            .collect();
        if self.rules.encounter_gain > 0 {
            let gain = i128::from(self.rules.encounter_gain);
            changes.par_extend(encounters.par_iter().flat_map_iter(|event| {
                [((event.a, event.b), gain), ((event.b, event.a), gain)]
                    .into_iter()
                    .filter(|(pair, _)| event.pressured != Some(pair.0))
            }));
        }
        if changes.is_empty() {
            self.events.clear();
            return;
        }
        if changes.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            changes.par_sort_unstable_by_key(|change| change.0);
        }
        let updates: Vec<_> = if changes.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
            changes
                .par_chunk_by(|a, b| a.0 == b.0)
                .map(|group| {
                    (
                        group[0].0,
                        if group.len() >= 2048 {
                            group.par_iter().map(|row| row.1).sum::<i128>()
                        } else {
                            group.iter().map(|row| row.1).sum::<i128>()
                        },
                    )
                })
                .collect()
        } else {
            changes
        };
        if self.records.is_empty() {
            let rules = &self.rules;
            self.events = updates
                .par_iter()
                .map(|&((observer, target), delta)| Event {
                    tick,
                    observer,
                    target,
                    before: rules.initial,
                    after: (i128::from(rules.initial) + delta)
                        .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                        as i32,
                })
                .collect();
            self.records = self
                .events
                .par_iter()
                .map(|event| Opinion {
                    observer: event.observer,
                    target: event.target,
                    trust: event.after,
                    fear: 0,
                    obligation: 0,
                    affinity: 0,
                })
                .collect();
            return;
        }
        let mut old = std::mem::take(&mut self.records);
        if old
            .par_windows(2)
            .any(|p| (p[0].observer, p[0].target) > (p[1].observer, p[1].target))
        {
            old.par_sort_unstable_by_key(|row| (row.observer, row.target));
        }
        let rules = &self.rules;
        let merged: Vec<_> = updates
            .par_chunks(1024)
            .enumerate()
            .flat_map_iter(|(chunk_index, chunk)| {
                let start = if chunk_index == 0 {
                    0
                } else {
                    old.partition_point(|r| (r.observer, r.target) < chunk[0].0)
                };
                let end = if (chunk_index + 1) * 1024 >= updates.len() {
                    old.len()
                } else {
                    old.partition_point(|r| {
                        (r.observer, r.target) < updates[(chunk_index + 1) * 1024].0
                    })
                };
                let mut cursor = start;
                let mut rows = Vec::with_capacity(end - start + chunk.len());
                for &((observer, target), delta) in chunk {
                    while cursor < end
                        && (old[cursor].observer, old[cursor].target) < (observer, target)
                    {
                        rows.push((old[cursor].clone(), None));
                        cursor += 1;
                    }
                    let (before, fear, obligation, affinity) = if cursor < end
                        && (old[cursor].observer, old[cursor].target) == (observer, target)
                    {
                        let previous = (
                            old[cursor].trust,
                            old[cursor].fear,
                            old[cursor].obligation,
                            old[cursor].affinity,
                        );
                        cursor += 1;
                        previous
                    } else {
                        (rules.initial, 0, 0, 0)
                    };
                    let after = (i128::from(before) + delta)
                        .clamp(i128::from(rules.minimum), i128::from(rules.maximum))
                        as i32;
                    rows.push((
                        Opinion {
                            observer,
                            target,
                            trust: after,
                            fear,
                            obligation,
                            affinity,
                        },
                        Some(Event {
                            tick,
                            observer,
                            target,
                            before,
                            after,
                        }),
                    ));
                }
                rows.extend(old[cursor..end].iter().cloned().map(|row| (row, None)));
                rows
            })
            .collect();
        self.events = merged.par_iter().filter_map(|row| row.1.clone()).collect();
        self.records = merged.into_par_iter().map(|row| row.0).collect();
    }
}

/// Clear transient receipts even when no agreement owner is enabled.
pub fn advance_optional(
    opinions: &mut Option<Opinions>,
    agreements: Option<&game_parallel_agreements::Agreements>,
    encounters: &[game_parallel_social::Encounter],
    tick: u64,
) {
    if let Some(opinions) = opinions {
        let (contracts, events) = agreements.map_or((&[][..], &[][..]), |owner| {
            (owner.contracts.as_slice(), owner.events.as_slice())
        });
        opinions.advance_with_social(contracts, events, encounters, tick);
        opinions.decay_fear();
    }
}

#[cfg(test)]
#[path = "../tests/support/owner.rs"]
mod tests;
