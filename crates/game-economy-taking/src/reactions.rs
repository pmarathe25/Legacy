//! Each participant's interpretation of an observed attempt is personal data.
use crate::{Event, Outcome, Taking};
use game_opinions::{PersonalChange, SubjectiveReaction};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interpretation {
    #[serde(default)]
    pub transferred: Option<SubjectiveReaction>,
    #[serde(default)]
    pub unavailable: Option<SubjectiveReaction>,
    #[serde(default)]
    pub resisted: Option<SubjectiveReaction>,
    #[serde(default)]
    pub conflict: Option<SubjectiveReaction>,
}
impl Interpretation {
    pub(crate) fn get(self, outcome: Outcome) -> Option<SubjectiveReaction> {
        match outcome {
            Outcome::Transferred => self.transferred,
            Outcome::Unavailable => self.unavailable,
            Outcome::Resisted => self.resisted,
            Outcome::Conflict => self.conflict,
        }
    }
}

/// An optional person-specific override falls back per outcome to the general case.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reaction {
    pub person: u64,
    #[serde(default)]
    pub other: Option<u64>,
    #[serde(default)]
    pub as_actor: Interpretation,
    #[serde(default)]
    pub as_target: Interpretation,
}

/// One person's attention to another person's actions, with independent feelings
/// toward each participant. A rule is not evidence that an event was observed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Witness {
    pub person: u64,
    pub actor: u64,
    #[serde(default)]
    pub target: Option<u64>,
    #[serde(default)]
    pub notices: bool,
    #[serde(default)]
    pub toward_actor: Interpretation,
    #[serde(default)]
    pub toward_target: Interpretation,
    /// The most recent taking this person actually noticed by this actor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<ObservedTaking>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedTaking {
    pub tick: u64,
    pub target: u64,
    pub good: u32,
    pub quantity: u64,
    pub outcome: Outcome,
}

impl Taking {
    /// Remember only directly noticed events, even if no relationship rule is enabled.
    pub fn observe_witnesses(
        &mut self,
        events: &[Event],
        can_observe: &(impl Fn(u64, u64) -> bool + Sync),
    ) -> Vec<PersonalChange> {
        let changes = self.witness_changes(events, can_observe);
        if self.witnesses.is_empty() || events.is_empty() {
            return changes;
        }
        let mut by_actor: Vec<_> = events.par_iter().collect();
        if by_actor.par_windows(2).any(|p| p[0].actor > p[1].actor) {
            by_actor.par_sort_unstable_by_key(|event| event.actor);
        }
        let mut updates: Vec<_> = self
            .witnesses
            .par_iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                if !rule.notices || !can_observe(rule.person, rule.actor) {
                    return None;
                }
                let first = by_actor.partition_point(|event| event.actor < rule.actor);
                let last = by_actor.partition_point(|event| event.actor <= rule.actor);
                let event = by_actor[first..last]
                    .iter()
                    .filter(|event| {
                        (rule.person != event.target || event.noticed)
                            && rule.target.is_none_or(|target| target == event.target)
                    })
                    .max_by_key(|event| {
                        (
                            event.tick,
                            event.target,
                            event.good,
                            event.quantity,
                            match event.outcome {
                                Outcome::Transferred => 0,
                                Outcome::Unavailable => 1,
                                Outcome::Resisted => 2,
                                Outcome::Conflict => 3,
                            },
                        )
                    })?;
                Some((
                    index,
                    ObservedTaking {
                        tick: event.tick,
                        target: event.target,
                        good: event.good,
                        quantity: event.quantity,
                        outcome: event.outcome,
                    },
                ))
            })
            .collect();
        if !updates.is_empty() {
            if updates.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
                updates.par_sort_unstable_by_key(|row| row.0);
            }
            self.witnesses_mut()
                .par_iter_mut()
                .enumerate()
                .for_each(|(index, rule)| {
                    if let Ok(found) = updates.binary_search_by_key(&index, |row| row.0) {
                        let observation = updates[found].1;
                        if rule
                            .last_seen
                            .is_none_or(|old| old.tick <= observation.tick)
                        {
                            rule.last_seen = Some(observation);
                        }
                    }
                });
        }
        changes
    }
    /// Only co-located, personally attentive witnesses receive signed changes.
    /// Callers supply their own visibility rule for physical boundaries.
    pub fn witness_changes(
        &self,
        events: &[Event],
        can_observe: &(impl Fn(u64, u64) -> bool + Sync),
    ) -> Vec<PersonalChange> {
        if self.witnesses.is_empty() || events.is_empty() {
            return Vec::new();
        }
        let mut by_actor: Vec<_> = events.par_iter().collect();
        if by_actor.par_windows(2).any(|p| p[0].actor > p[1].actor) {
            by_actor.par_sort_unstable_by_key(|event| event.actor);
        }
        self.witnesses
            .par_iter()
            .filter(|rule| rule.notices)
            .flat_map_iter(|rule| {
                let first = by_actor.partition_point(|event| event.actor < rule.actor);
                let last = by_actor.partition_point(|event| event.actor <= rule.actor);
                by_actor[first..last].iter().flat_map(move |event| {
                    let visible = rule.target.is_none_or(|target| target == event.target)
                        && rule.person != event.target
                        && rule.person != event.actor
                        && can_observe(rule.person, event.actor);
                    let change = |target, interpretation: Interpretation| {
                        let value = interpretation.get(event.outcome)?;
                        Some(PersonalChange {
                            observer: rule.person,
                            target,
                            trust: value.trust,
                            affinity: value.affinity,
                            fear: value.fear,
                            obligation: value.obligation,
                        })
                    };
                    [
                        visible
                            .then(|| change(event.actor, rule.toward_actor))
                            .flatten(),
                        visible
                            .then(|| change(event.target, rule.toward_target))
                            .flatten(),
                    ]
                    .into_iter()
                    .flatten()
                })
            })
            .collect()
    }
    /// Generate directed signed changes only from each participant's own experience.
    pub fn personal_changes(&self, events: &[Event]) -> Vec<PersonalChange> {
        if self.reactions.is_empty() || events.is_empty() {
            return Vec::new();
        }
        let mut rules: Vec<_> = self.reactions.par_iter().collect();
        if rules
            .par_windows(2)
            .any(|p| (p[0].person, p[0].other) > (p[1].person, p[1].other))
        {
            rules.par_sort_unstable_by_key(|p| (p.person, p.other));
        }
        let interpret = |person, other, actor, outcome| {
            let find = |key| {
                rules
                    .binary_search_by_key(&(person, key), |r| (r.person, r.other))
                    .ok()
                    .map(|i| rules[i])
            };
            let get = |rule: &Reaction| {
                if actor {
                    rule.as_actor.get(outcome)
                } else {
                    rule.as_target.get(outcome)
                }
            };
            let value = find(Some(other))
                .and_then(get)
                .or_else(|| find(None).and_then(get))?;
            Some(PersonalChange {
                observer: person,
                target: other,
                trust: value.trust,
                affinity: value.affinity,
                fear: value.fear,
                obligation: value.obligation,
            })
        };
        events
            .par_iter()
            .flat_map_iter(|event| {
                let actor = interpret(event.actor, event.target, true, event.outcome);
                let target = event
                    .noticed
                    .then(|| interpret(event.target, event.actor, false, event.outcome))
                    .flatten();
                [actor, target].into_iter().flatten()
            })
            .collect()
    }
}
