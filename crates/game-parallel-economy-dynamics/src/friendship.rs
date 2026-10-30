//! Choose disjoint local encounters using each individual's directed affinity.
use game_parallel_opinions::Opinions;
use game_parallel_social::{Contact, Encounter, Social};
use rayon::prelude::*;

pub struct Context<'a> {
    pub social: &'a Social,
    pub contacts: &'a [Contact],
    pub opinions: Option<&'a Opinions>,
    pub needs: &'a [(u64, u32)],
    pub occupied: &'a [u64],
    pub seed: u64,
    pub tick: u64,
}
impl Context<'_> {
    pub fn prepare(&self, voluntary: &(impl Fn(u64, u32) -> bool + Sync)) -> Vec<Encounter> {
        let Some(opinions) = self.opinions.filter(|o| {
            o.rules.affinity.is_some()
                && !o.records.is_empty()
                && self.tick.is_multiple_of(self.social.rules.interval)
        }) else {
            return Vec::new();
        };
        let mut participants: Vec<_> = self
            .social
            .participants
            .par_iter()
            .filter(|p| p.willing)
            .map(|p| p.person)
            .collect();
        participants.par_sort_unstable();
        let index = opinions.index();
        let eligible = |id: u64| {
            let contact = self
                .contacts
                .binary_search_by_key(&id, |c| c.person)
                .ok()
                .map(|i| self.contacts[i])?;
            let isolation = self
                .needs
                .binary_search_by_key(&id, |row| row.0)
                .ok()
                .map_or(0, |i| self.needs[i].1);
            (!contact.busy
                && contact.health > 0
                && contact.health >= self.social.rules.min_health
                && contact.hunger <= self.social.rules.max_hunger
                && participants.binary_search(&id).is_ok()
                && self.occupied.binary_search(&id).is_err()
                && voluntary(id, isolation))
            .then_some(contact)
        };
        let mut edges: Vec<_> = opinions
            .records
            .par_iter()
            .filter_map(|record| {
                if record.affinity <= 0 {
                    return None;
                }
                let a = record.observer.min(record.target);
                let b = record.observer.max(record.target);
                let left = eligible(a)?;
                let right = eligible(b)?;
                let region = left.region?;
                if right.region != Some(region) {
                    return None;
                }
                let forward = index.affinity(a, b);
                let reverse = index.affinity(b, a);
                Some((
                    a,
                    b,
                    region,
                    forward,
                    reverse,
                    game_parallel_genetics::random(self.seed ^ a, b, self.tick),
                ))
            })
            .collect();
        edges.par_sort_unstable();
        edges = edges
            .par_chunk_by(|a, b| (a.0, a.1) == (b.0, b.1))
            .map(|g| g[0])
            .collect();
        let mut choices: Vec<_> = edges
            .par_iter()
            .enumerate()
            .flat_map_iter(|(i, e)| {
                [
                    (e.0, std::cmp::Reverse(e.3), e.5, i),
                    (e.1, std::cmp::Reverse(e.4), e.5, i),
                ]
            })
            .collect();
        choices.par_sort_unstable();
        let choices: Vec<_> = choices
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|g| (g[0].0, g[0].3))
            .collect();
        edges
            .par_iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let a = choices.binary_search_by_key(&e.0, |c| c.0).ok()?;
                let b = choices.binary_search_by_key(&e.1, |c| c.0).ok()?;
                (choices[a].1 == i && choices[b].1 == i).then_some(Encounter {
                    tick: self.tick,
                    region: e.2,
                    a: e.0,
                    b: e.1,
                    pressured: None,
                })
            })
            .collect()
    }
}
