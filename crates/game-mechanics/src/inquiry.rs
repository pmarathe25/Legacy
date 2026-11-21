//! Conditional comparisons over one person's own recorded trials.
use crate::{Observation, Outcome};
use serde::{Deserialize, Serialize};

pub const MAX_RECENT_TRIALS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conditions {
    pub apparatus: u64,
    pub output_part: u64,
    pub input_force: u64,
    pub input_distance: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Comparison {
    pub conditions: Conditions,
    pub largest_load_moved: u64,
    pub smallest_load_stalled: u64,
    pub distinct_moved_loads: u32,
    pub distinct_stalled_loads: u32,
    pub first_evidence_millis: u128,
    pub latest_evidence_millis: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub activity: u32,
    pub trial_limit: usize,
    pub capacity: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comparisons: Vec<Comparison>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conclusion {
    pub person: u64,
    pub at_millis: u128,
    pub evidence: Comparison,
}

impl Comparison {
    pub fn contradicted(self) -> bool {
        self.largest_load_moved >= self.smallest_load_stalled
    }

    fn same_result(self, earlier: Self) -> bool {
        self.conditions == earlier.conditions
            && self.largest_load_moved == earlier.largest_load_moved
            && self.smallest_load_stalled == earlier.smallest_load_stalled
            && self.distinct_moved_loads == earlier.distinct_moved_loads
            && self.distinct_stalled_loads == earlier.distinct_stalled_loads
    }
}

impl Study {
    pub fn validate(&self, now: u128) -> bool {
        self.activity > 0
            && (2..=MAX_RECENT_TRIALS).contains(&self.trial_limit)
            && (1..=32).contains(&self.capacity)
            && self.comparisons.len() <= self.capacity
            && self
                .comparisons
                .windows(2)
                .all(|rows| rows[0].conditions < rows[1].conditions)
            && self.comparisons.iter().all(|row| {
                row.conditions.apparatus > 0
                    && row.conditions.output_part > 0
                    && row.conditions.input_force > 0
                    && row.conditions.input_distance > 0
                    && row.distinct_moved_loads > 0
                    && row.distinct_stalled_loads > 0
                    && u64::from(row.distinct_moved_loads) + u64::from(row.distinct_stalled_loads)
                        <= self.trial_limit as u64
                    && row.first_evidence_millis <= row.latest_evidence_millis
                    && row.latest_evidence_millis <= now
            })
    }

    pub fn reflect(
        &mut self,
        person: u64,
        observations: &[Observation],
        now: u128,
    ) -> Vec<Conclusion> {
        let comparisons = compare_loads(person, observations, self.trial_limit, now);
        if comparisons.is_empty() {
            return Vec::new();
        }
        let mut retained = self.comparisons.clone();
        for comparison in comparisons {
            match retained.binary_search_by_key(&comparison.conditions, |row| row.conditions) {
                Ok(index) => retained[index] = comparison,
                Err(index) => retained.insert(index, comparison),
            }
        }
        retained.sort_unstable_by_key(|row| {
            std::cmp::Reverse((row.latest_evidence_millis, row.conditions))
        });
        retained.truncate(self.capacity);
        retained.sort_unstable_by_key(|row| row.conditions);
        let conclusions = retained
            .iter()
            .filter_map(|&evidence| {
                let earlier = self
                    .comparisons
                    .binary_search_by_key(&evidence.conditions, |row| row.conditions)
                    .ok()
                    .map(|index| self.comparisons[index]);
                earlier
                    .is_none_or(|row| !evidence.same_result(row))
                    .then_some(Conclusion {
                        person,
                        at_millis: now,
                        evidence,
                    })
            })
            .collect();
        self.comparisons = retained;
        conclusions
    }
}

/// Estimates a possible load boundary after comparing like conditions. An
/// overlapping result preserves the counterexample instead of declaring a limit.
pub fn compare_loads(
    person: u64,
    observations: &[Observation],
    trial_limit: usize,
    now: u128,
) -> Vec<Comparison> {
    let mut trials: Vec<_> = observations
        .iter()
        .rev()
        .take(trial_limit.min(MAX_RECENT_TRIALS))
        .filter_map(|row| {
            let elapsed = row.at_millis?;
            let moved = match row.outcome {
                Outcome::Moved => true,
                Outcome::Stalled => false,
                Outcome::Broken | Outcome::Contended => return None,
            };
            (row.actor == person && elapsed <= now && row.input_force > 0 && row.input_distance > 0)
                .then_some((
                    Conditions {
                        apparatus: row.apparatus,
                        output_part: row.output_part,
                        input_force: row.input_force,
                        input_distance: row.input_distance,
                    },
                    moved,
                    row.load_force,
                    elapsed,
                ))
        })
        .collect();
    trials.sort_unstable();
    trials
        .chunk_by(|a, b| a.0 == b.0)
        .filter_map(|group| {
            let first_moved = group.partition_point(|row| !row.1);
            let (stalled, moved) = group.split_at(first_moved);
            let smallest_load_stalled = stalled.first()?.2;
            let largest_load_moved = moved.last()?.2;
            let dates = group.iter().map(|row| row.3);
            Some(Comparison {
                conditions: group[0].0,
                largest_load_moved,
                smallest_load_stalled,
                distinct_moved_loads: distinct_load_count(moved),
                distinct_stalled_loads: distinct_load_count(stalled),
                first_evidence_millis: dates.clone().min()?,
                latest_evidence_millis: dates.max()?,
            })
        })
        .collect()
}

fn distinct_load_count(group: &[(Conditions, bool, u64, u128)]) -> u32 {
    group.chunk_by(|a, b| a.2 == b.2).count() as u32
}
