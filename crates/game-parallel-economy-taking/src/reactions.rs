//! Each participant's interpretation of an observed attempt is personal data.
use crate::{Event, Outcome, Taking};
use game_parallel_opinions::{PersonalChange, SubjectiveReaction};
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
    fn get(self, outcome: Outcome) -> Option<SubjectiveReaction> {
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
}

impl Taking {
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
