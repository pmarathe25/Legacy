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

impl Taking {
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
