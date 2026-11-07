//! Individual guarding choices against physical barriers.
use super::{Confinement, Event, EventKind, ordered_index};
use game_economy_person::Person;
use game_opinions::Opinions;
use game_work_sites::Facilities;
use rayon::prelude::*;

pub struct Context<'a> {
    pub owner: &'a Confinement,
    pub people: &'a [Person],
    pub sites: &'a Facilities,
    pub busy: &'a [u64],
    pub opinions: Option<&'a Opinions>,
    pub manual: Option<(u64, u64)>,
    pub tick: u64,
}

pub struct Plan {
    pub actors: Vec<u64>,
    /// Sorted `(site, total opposing effort)` pairs.
    pub strengths: Vec<(u64, u128)>,
    /// The subset of guard effort whose owners choose to resist openings.
    pub opening_strengths: Vec<(u64, u128)>,
    /// Guard effort whose owners only resist escapes.
    pub escape_only_strengths: Vec<(u64, u128)>,
    pub events: Vec<Event>,
}

impl Context<'_> {
    pub fn prepare(self) -> Result<Plan, String> {
        let find = |id| ordered_index(self.people, id, |person| person.id).map(|i| &self.people[i]);
        let site = |id| {
            self.sites
                .sites
                .binary_search_by_key(&id, |site| site.id)
                .ok()
                .map(|i| &self.sites.sites[i])
        };
        let intact = |id| {
            self.owner
                .cells
                .binary_search_by_key(&id, |cell| cell.site)
                .ok()
                .is_some_and(|i| self.owner.cells[i].resistance > 0)
        };
        let effort = |id| {
            ordered_index(&self.owner.escape, id, |row| row.person)
                .map_or(self.owner.default_effort, |i| self.owner.escape[i].effort)
        };
        let need_opinions = self
            .owner
            .guard_policies
            .par_iter()
            .any(|rule| rule.pressure_threshold.is_some());
        let opinions = need_opinions
            .then(|| self.opinions.map(Opinions::index))
            .flatten();
        let mut choices: Vec<_> = self
            .owner
            .guard_policies
            .par_iter()
            .filter_map(|policy| {
                let person = find(policy.person)?;
                let location = site(policy.site)?;
                let pressured = policy
                    .pressured_by
                    .zip(policy.pressure_threshold)
                    .is_some_and(|(subject, threshold)| {
                        opinions.as_ref().is_some_and(|index| {
                            let (fear, obligation) = index.feelings(person.id, subject);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                (!person.controlled
                    && person.health > 0
                    && person.region == Some(location.region)
                    && !self.owner.held(person.id)
                    && self.busy.binary_search(&person.id).is_err()
                    && intact(policy.site)
                    && (policy.willing || pressured))
                    .then_some((
                        policy.site,
                        person.id,
                        effort(person.id),
                        policy.oppose_opening,
                    ))
            })
            .collect();
        if let Some((person_id, site_id)) = self.manual {
            let person = find(person_id).ok_or("unknown guard")?;
            let location = site(site_id).ok_or("unknown cell site")?;
            if !person.controlled
                || person.health == 0
                || self.owner.held(person_id)
                || person.region != Some(location.region)
                || !intact(site_id)
            {
                return Err("guarding requires a capable local person and intact barrier".into());
            }
            choices.push((site_id, person_id, effort(person_id), true));
        }
        if choices
            .par_windows(2)
            .any(|w| (w[0].0, w[0].1) > (w[1].0, w[1].1))
        {
            choices.par_sort_unstable_by_key(|row| (row.0, row.1));
        }
        let strengths = choices
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                (
                    group[0].0,
                    group
                        .par_iter()
                        .map(|row| u128::from(row.2))
                        .reduce(|| 0, u128::saturating_add),
                )
            })
            .collect();
        let opening_strengths = choices
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                (
                    group[0].0,
                    group
                        .par_iter()
                        .filter(|row| row.3)
                        .map(|row| u128::from(row.2))
                        .reduce(|| 0, u128::saturating_add),
                )
            })
            .filter(|row| row.1 > 0)
            .collect();
        let escape_only_strengths = choices
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| {
                (
                    group[0].0,
                    group
                        .par_iter()
                        .filter(|row| !row.3)
                        .map(|row| u128::from(row.2))
                        .reduce(|| 0, u128::saturating_add),
                )
            })
            .filter(|row| row.1 > 0)
            .collect();
        let mut actors: Vec<_> = choices.par_iter().map(|row| row.1).collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        let events = choices
            .into_par_iter()
            .map(|(site, person, _, _)| Event {
                tick: self.tick,
                person,
                actor: Some(person),
                site,
                kind: EventKind::Guarded,
            })
            .collect();
        Ok(Plan {
            actors,
            strengths,
            opening_strengths,
            escape_only_strengths,
            events,
        })
    }
}
