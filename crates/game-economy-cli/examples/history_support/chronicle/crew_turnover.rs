//! Changes in who people followed, derived from accepted crew starts.
use super::CrewParticipation;
use serde::Serialize;
use std::cmp::{Ordering, Reverse};
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct Transition {
    previous_year: u64,
    year: u64,
    previous_lead: u64,
    lead: u64,
    helpers: Vec<u64>,
}

#[derive(Default)]
struct Assignment {
    lead: u64,
    starts: u64,
    tied: bool,
}

impl Assignment {
    fn consider(&mut self, lead: u64, starts: u64) {
        match starts.cmp(&self.starts) {
            Ordering::Less => {}
            Ordering::Equal => self.tied = true,
            Ordering::Greater => {
                *self = Self {
                    lead,
                    starts,
                    tied: false,
                }
            }
        }
    }
}

pub(super) fn transitions(crews: &BTreeMap<u64, CrewParticipation>) -> Vec<Transition> {
    let mut years: BTreeMap<u64, BTreeMap<u64, Assignment>> = BTreeMap::new();
    for (&lead, crew) in crews {
        for (&year, evidence) in &crew.years {
            let helpers = years.entry(year).or_default();
            for (&helper, &starts) in &evidence.helpers {
                helpers.entry(helper).or_default().consider(lead, starts);
            }
        }
    }
    let mut years = years.into_iter();
    let Some((mut previous_year, mut previous)) = years.next() else {
        return Vec::new();
    };
    let mut changes = Vec::new();
    for (year, helpers) in years {
        if previous_year.checked_add(1) == Some(year) {
            let mut pairs: BTreeMap<(u64, u64), Vec<u64>> = BTreeMap::new();
            for (&helper, assignment) in &helpers {
                if let Some(earlier) = previous.get(&helper)
                    && !earlier.tied
                    && !assignment.tied
                    && earlier.lead != assignment.lead
                {
                    pairs
                        .entry((earlier.lead, assignment.lead))
                        .or_default()
                        .push(helper);
                }
            }
            changes.extend(
                pairs
                    .into_iter()
                    .filter_map(|((previous_lead, lead), helpers)| {
                        (helpers.len() > 1).then_some(Transition {
                            previous_year,
                            year,
                            previous_lead,
                            lead,
                            helpers,
                        })
                    }),
            );
        }
        previous_year = year;
        previous = helpers;
    }
    changes.sort_unstable_by_key(|row| {
        (
            Reverse(row.helpers.len()),
            row.year,
            row.previous_lead,
            row.lead,
        )
    });
    changes.truncate(20);
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_assignments_track_the_same_workers_and_skip_ties() {
        let mut crews = BTreeMap::<_, CrewParticipation>::new();
        for (lead, year, helper) in [
            (10, 1, 1),
            (10, 1, 1),
            (12, 1, 1),
            (10, 1, 2),
            (10, 1, 3),
            (11, 2, 1),
            (11, 2, 2),
            (11, 2, 3),
            (12, 2, 3),
            (12, 4, 1),
            (12, 4, 2),
            (12, 4, 3),
        ] {
            crews.entry(lead).or_default().record(year, helper);
        }
        assert_eq!(
            transitions(&crews),
            [Transition {
                previous_year: 1,
                year: 2,
                previous_lead: 10,
                lead: 11,
                helpers: vec![1, 2],
            }]
        );
    }
}
