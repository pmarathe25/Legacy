//! Bounded personal recall of experienced episodes.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Meeting,
    CreativePractice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cue {
    Meeting { other: u64, region: u64 },
    CreativePractice { activity: u32 },
}

impl Cue {
    fn kind(self) -> Kind {
        match self {
            Self::Meeting { .. } => Kind::Meeting,
            Self::CreativePractice { .. } => Kind::CreativePractice,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Observation {
    pub person: u64,
    pub at_millis: u128,
    pub cue: Cue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attention {
    pub kind: Kind,
    pub value: u32,
    pub retention_millis: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Consolidation {
    pub rest_millis: u64,
    pub extension_millis: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Episode {
    pub cue: Cue,
    pub observed_at: u128,
    pub accessible_until: u128,
    pub value: u32,
}

impl Episode {
    fn strength(self, now: u128) -> u32 {
        let lifetime = self.accessible_until - self.observed_at;
        let remaining = self.accessible_until.saturating_sub(now).min(lifetime);
        let scaled = remaining.saturating_mul(u128::from(self.value));
        scaled.div_ceil(lifetime) as u32
    }

    fn rank(self, now: u128) -> (u32, u128, Cue) {
        (self.strength(now), self.observed_at, self.cue)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mind {
    pub person: u64,
    pub capacity: usize,
    pub attention: Vec<Attention>,
    #[serde(default)]
    pub consolidation: Option<Consolidation>,
    #[serde(default)]
    pub reflection_millis: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resting_since: Option<u128>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memories: Vec<Episode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub minds: Vec<Mind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recall {
    pub cue: Cue,
    /// A faint trace may be accessible without a precise recollection of when.
    pub precise_time: Option<u128>,
    pub strength: u32,
}

impl Memory {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
    ) -> Result<(), &'static str> {
        if self
            .minds
            .par_windows(2)
            .any(|row| row[0].person >= row[1].person)
            || self.minds.par_iter().any(|mind| {
                !people.contains(&mind.person)
                    || !(1..=64).contains(&mind.capacity)
                    || mind.memories.len() > mind.capacity
                    || mind
                        .attention
                        .windows(2)
                        .any(|row| row[0].kind >= row[1].kind)
                    || mind.attention.iter().any(|row| row.retention_millis == 0)
                    || mind
                        .consolidation
                        .is_some_and(|row| row.rest_millis == 0 || row.extension_millis == 0)
                    || (mind.consolidation.is_none() && mind.resting_since.is_some())
                    || mind.memories.iter().any(|row| {
                        row.accessible_until <= row.observed_at
                            || row.value == 0
                            || match row.cue {
                                Cue::Meeting { other, region } => {
                                    other == mind.person
                                        || !people.contains(&other)
                                        || !regions.contains(&region)
                                }
                                Cue::CreativePractice { activity } => activity == 0,
                            }
                    })
            })
        {
            return Err("invalid personal episode memory");
        }
        Ok(())
    }

    pub fn validate_at(&self, now: u128) -> Result<(), &'static str> {
        if self.minds.par_iter().any(|mind| {
            mind.resting_since.is_some_and(|start| start > now)
                || mind.memories.iter().any(|row| row.observed_at > now)
        }) {
            return Err("personal episode memory is ahead of the simulation clock");
        }
        Ok(())
    }

    fn index(&self, person: u64) -> Option<usize> {
        self.minds
            .binary_search_by_key(&person, |row| row.person)
            .ok()
    }

    /// Record only traces that the individual attends to.
    pub fn remember(&mut self, observations: &mut [Observation], now: u128) {
        if self.minds.is_empty() || observations.is_empty() {
            return;
        }
        observations.par_sort_unstable_by_key(|row| (row.person, row.at_millis, row.cue));
        remember_minds(&mut self.minds, observations, now);
    }

    /// Reading or considering a cue leaves memory unchanged.
    pub fn recall(&self, person: u64, other: Option<u64>, now: u128) -> Vec<Recall> {
        let Some(index) = self.index(person) else {
            return Vec::new();
        };
        let mut rows: Vec<_> = self.minds[index]
            .memories
            .iter()
            .filter_map(|row| {
                if row.observed_at > now
                    || other.is_some_and(
                        |id| !matches!(row.cue, Cue::Meeting { other, .. } if other == id),
                    )
                {
                    return None;
                }
                let strength = row.strength(now);
                (strength > 0).then_some((
                    row.observed_at,
                    Recall {
                        cue: row.cue,
                        precise_time: (strength >= row.value.div_ceil(2))
                            .then_some(row.observed_at),
                        strength,
                    },
                ))
            })
            .collect();
        rows.sort_unstable_by_key(|row| Reverse((row.1.strength, row.0, row.1.cue)));
        rows.into_iter().map(|(_, recalled)| recalled).collect()
    }

    /// A deliberate reflection can reinforce the strongest episode for this cue.
    pub fn reflect(&mut self, person: u64, cue: Cue, now: u128) -> bool {
        let Some(index) = self.index(person) else {
            return false;
        };
        let mind = &mut self.minds[index];
        if mind.reflection_millis == 0 {
            return false;
        }
        let Some(row) = mind
            .memories
            .iter_mut()
            .filter(|row| row.cue == cue && row.observed_at <= now && row.accessible_until > now)
            .max_by_key(|row| (row.strength(now), row.observed_at))
        else {
            return false;
        };
        row.accessible_until = row
            .accessible_until
            .saturating_add(u128::from(mind.reflection_millis));
        true
    }

    pub fn rest(&mut self, person: u64, resting: bool, now: u128) {
        let Some(index) = self.index(person) else {
            return;
        };
        let mind = &mut self.minds[index];
        let Some(consolidation) = mind.consolidation else {
            return;
        };
        if resting {
            mind.resting_since.get_or_insert(now);
        } else if let Some(started) = mind.resting_since.take()
            && let Some(reached) = started.checked_add(u128::from(consolidation.rest_millis))
            && reached <= now
        {
            for row in mind
                .memories
                .iter_mut()
                .filter(|row| row.observed_at <= started && row.accessible_until >= reached)
            {
                row.accessible_until = row
                    .accessible_until
                    .saturating_add(u128::from(consolidation.extension_millis));
            }
        }
    }
}

fn remember_minds(minds: &mut [Mind], observations: &[Observation], now: u128) {
    if minds.is_empty() || observations.is_empty() {
        return;
    }
    if minds.len() > 32 {
        let (left, right) = minds.split_at_mut(minds.len() / 2);
        let pivot = observations.partition_point(|row| row.person < right[0].person);
        let (first, second) = observations.split_at(pivot);
        if first.is_empty() {
            remember_minds(right, second, now);
        } else if second.is_empty() {
            remember_minds(left, first, now);
        } else {
            rayon::join(
                || remember_minds(left, first, now),
                || remember_minds(right, second, now),
            );
        }
        return;
    }
    for mind in minds {
        let start = observations.partition_point(|row| row.person < mind.person);
        let end = observations.partition_point(|row| row.person <= mind.person);
        if start != end {
            mind.remember(&observations[start..end], now);
        }
    }
}

impl Mind {
    fn remember(&mut self, observations: &[Observation], now: u128) {
        self.memories.retain(|row| row.accessible_until > now);
        self.memories
            .sort_unstable_by_key(|row| Reverse(row.rank(now)));
        for row in observations {
            let Ok(index) = self
                .attention
                .binary_search_by_key(&row.cue.kind(), |record| record.kind)
            else {
                continue;
            };
            let attention = self.attention[index];
            let until = row
                .at_millis
                .saturating_add(u128::from(attention.retention_millis));
            if row.at_millis > now
                || attention.value == 0
                || until <= now
                || self
                    .memories
                    .iter()
                    .any(|episode| episode.observed_at == row.at_millis && episode.cue == row.cue)
            {
                continue;
            }
            let episode = Episode {
                cue: row.cue,
                observed_at: row.at_millis,
                accessible_until: until,
                value: attention.value,
            };
            let rank = episode.rank(now);
            let position = self.memories.partition_point(|row| row.rank(now) > rank);
            if position < self.capacity {
                if self.memories.len() == self.capacity {
                    self.memories.pop();
                }
                self.memories.insert(position, episode);
            }
        }
    }
}
