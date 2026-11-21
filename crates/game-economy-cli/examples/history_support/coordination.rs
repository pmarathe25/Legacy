//! Track people around whom actual cooperation concentrates in the archive.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct History {
    people: BTreeMap<u64, Series>,
}

#[derive(Clone, Copy)]
pub(super) enum Evidence {
    Crew,
    Delivery,
}

struct Series {
    first_year: u64,
    latest_year: u64,
    active_years: u64,
    collaborators: BTreeSet<u64>,
    peak: (usize, u64),
    crew_choices: u64,
    delivery_ties: u64,
}

impl Series {
    fn new(year: u64) -> Self {
        Self {
            first_year: year,
            latest_year: year,
            active_years: 1,
            collaborators: BTreeSet::new(),
            peak: (0, year),
            crew_choices: 0,
            delivery_ties: 0,
        }
    }

    fn observe(&mut self, collaborator: u64, year: u64, evidence: Evidence) {
        if year > self.latest_year {
            self.latest_year = year;
            self.active_years += 1;
            self.collaborators.clear();
        }
        self.collaborators.insert(collaborator);
        if self.collaborators.len() > self.peak.0 {
            self.peak = (self.collaborators.len(), year);
        }
        match evidence {
            Evidence::Crew => self.crew_choices += 1,
            Evidence::Delivery => self.delivery_ties += 1,
        }
    }
}

impl History {
    pub(super) fn observe(
        &mut self,
        person: u64,
        collaborator: u64,
        year: u64,
        evidence: Evidence,
    ) {
        if person != 0 && collaborator != 0 && person != collaborator {
            self.people
                .entry(person)
                .or_insert_with(|| Series::new(year))
                .observe(collaborator, year, evidence);
        }
    }

    pub(super) fn summary(&self) -> Vec<Value> {
        let mut rows: Vec<_> = self
            .people
            .iter()
            .filter(|(_, row)| row.active_years >= 2 && row.peak.0 >= 2)
            .collect();
        rows.sort_unstable_by_key(|&(person, row)| {
            (
                std::cmp::Reverse(row.active_years),
                std::cmp::Reverse(row.peak.0),
                *person,
            )
        });
        rows.into_iter().take(10).map(|(&person, row)| json!({
            "person": person, "first_year": row.first_year, "latest_year": row.latest_year,
            "active_years": row.active_years, "peak_year": row.peak.1, "peak_collaborators": row.peak.0,
            "crew_choices": row.crew_choices, "annual_delivery_ties": row.delivery_ties,
        })).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn years_peaks_and_latest_evidence_come_from_distinct_collaborators() {
        let mut history = History::default();
        history.observe(1, 2, 2, Evidence::Crew);
        history.observe(1, 2, 2, Evidence::Delivery);
        history.observe(1, 3, 2, Evidence::Crew);
        history.observe(1, 1, 3, Evidence::Crew);
        history.observe(5, 6, 3, Evidence::Crew);
        history.observe(5, 7, 3, Evidence::Crew);
        history.observe(1, 4, 4, Evidence::Delivery);
        assert_eq!(
            history.summary(),
            [json!({ "person": 1, "first_year": 2,
                "latest_year": 4, "active_years": 2, "peak_year": 2, "peak_collaborators": 2,
                "crew_choices": 2, "annual_delivery_ties": 2,
            })]
        );
    }
}
