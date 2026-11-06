//! Match independent conversational opportunities across physical openings.
use game_parallel_social::{Contact, Encounter, Social};
use rayon::prelude::*;

pub struct Context<'a> {
    pub social: &'a Social,
    pub contacts: &'a [Contact],
    pub needs: &'a [(u64, u32)],
    pub occupied: &'a [u64],
    pub seed: u64,
    pub tick: u64,
    /// Real region and whether the person is behind an open barrier.
    pub location: &'a (dyn Fn(u64) -> Option<(u64, bool)> + Sync),
}

impl Context<'_> {
    pub fn prepare(&self, voluntary: &(impl Fn(u64, u32) -> bool + Sync)) -> Vec<Encounter> {
        if !self.tick.is_multiple_of(self.social.rules.interval) {
            return Vec::new();
        }
        let mut willing: Vec<_> = self
            .social
            .participants
            .par_iter()
            .filter(|person| person.willing)
            .map(|person| person.person)
            .collect();
        if willing.par_windows(2).any(|pair| pair[0] > pair[1]) {
            willing.par_sort_unstable();
        }
        let mut candidates: Vec<_> = self
            .contacts
            .par_iter()
            .filter_map(|contact| {
                if contact.busy
                    || contact.health == 0
                    || contact.health < self.social.rules.min_health
                    || contact.hunger > self.social.rules.max_hunger
                    || willing.binary_search(&contact.person).is_err()
                    || self.occupied.binary_search(&contact.person).is_ok()
                {
                    return None;
                }
                let isolation = self
                    .needs
                    .binary_search_by_key(&contact.person, |row| row.0)
                    .ok()
                    .map_or(0, |index| self.needs[index].1);
                if !voluntary(contact.person, isolation) {
                    return None;
                }
                let (region, inside) = (self.location)(contact.person)?;
                Some((
                    region,
                    inside,
                    std::cmp::Reverse(isolation),
                    game_parallel_genetics::random(self.seed ^ region, contact.person, self.tick),
                    contact.person,
                    contact.region?,
                ))
            })
            .collect();
        if !candidates.par_iter().any(|candidate| candidate.1) {
            return Vec::new();
        }
        candidates.par_sort_unstable();
        candidates
            .par_chunk_by(|a, b| a.0 == b.0)
            .flat_map(|group| {
                let split = group.partition_point(|row| !row.1);
                let pairs = split.min(group.len() - split);
                (0..pairs).into_par_iter().map(move |i| {
                    let outside = group[i];
                    let inside = group[split + i];
                    Encounter {
                        tick: self.tick,
                        region: outside.5,
                        a: outside.4.min(inside.4),
                        b: outside.4.max(inside.4),
                        pressured: None,
                    }
                })
            })
            .collect()
    }
}
