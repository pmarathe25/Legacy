//! Deliberate meeting objectives remain individual, local social choices.
use game_parallel_economy_person::Person;
use game_parallel_missions::{Missions, Objective, Phase};
use game_parallel_opinions::Opinions;
use game_parallel_social::{Contact, Encounter, Social};
use rayon::prelude::*;

pub struct Context<'a> {
    pub missions: Option<&'a Missions>,
    pub people: &'a [Person],
    pub contacts: &'a [Contact],
    pub social: &'a Social,
    pub opinions: Option<&'a Opinions>,
    pub occupied: &'a [u64],
    pub speak: Option<&'a (dyn Fn(u64, u64) -> bool + Sync)>,
    pub seed: u64,
    pub tick: u64,
}

impl Context<'_> {
    pub fn select(&self) -> Vec<Encounter> {
        let Some(missions) = self.missions else {
            return vec![];
        };
        if !self.tick.is_multiple_of(self.social.rules.interval)
            || !missions.entries.par_iter().any(|mission| {
                !mission.objective_met
                    && matches!(mission.objective, Some(Objective::Meet { .. }))
                    && matches!(
                        mission.phase,
                        Phase::Outbound | Phase::Visiting | Phase::Returning
                    )
            })
        {
            return vec![];
        }
        let mut participants: Vec<_> = self.social.participants.par_iter().collect();
        participants.par_sort_unstable_by_key(|row| row.person);
        let mut policies: Vec<_> = self.social.pressures.par_iter().collect();
        policies.par_sort_unstable_by_key(|row| row.person);
        let feelings = self.opinions.map(Opinions::index);
        let find_contact = |id| {
            self.contacts
                .binary_search_by_key(&id, |row| row.person)
                .ok()
                .map(|index| self.contacts[index])
        };
        let find_participant = |id| {
            participants
                .binary_search_by_key(&id, |row| row.person)
                .ok()
                .map(|index| participants[index])
        };
        let eligible = |contact: Contact| {
            !contact.busy
                && contact.health >= self.social.rules.min_health.max(1)
                && contact.hunger <= self.social.rules.max_hunger
                && self.occupied.binary_search(&contact.person).is_err()
        };
        let candidates: Vec<_> = missions
            .entries
            .par_iter()
            .filter_map(|mission| {
                let Some(Objective::Meet { person: target }) = mission.objective else {
                    return None;
                };
                if mission.objective_met
                    || !matches!(
                        mission.phase,
                        Phase::Outbound | Phase::Visiting | Phase::Returning
                    )
                {
                    return None;
                }
                let person = self
                    .people
                    .binary_search_by_key(&mission.person, |row| row.id)
                    .ok()
                    .map(|index| &self.people[index])?;
                if person.region != Some(mission.destination) {
                    return None;
                }
                let actor = find_contact(mission.person)?;
                let target_contact = find_contact(target)?;
                let region = actor.region?;
                if !eligible(actor)
                    || !eligible(target_contact)
                    || !find_participant(actor.person)?.willing
                    || (target_contact.region != Some(region)
                        && !self
                            .speak
                            .is_some_and(|can_speak| can_speak(actor.person, target)))
                {
                    return None;
                }
                let willing = find_participant(target)?.willing;
                let pressured = !willing
                    && policies
                        .binary_search_by_key(&target, |row| row.person)
                        .ok()
                        .is_some_and(|index| {
                            feelings.as_ref().is_some_and(|opinions| {
                                let (fear, obligation) = opinions.feelings(target, actor.person);
                                policies[index].responds(fear, obligation)
                            })
                        });
                if !willing && !pressured {
                    return None;
                }
                let rank =
                    game_parallel_genetics::random(self.seed ^ actor.person, target, self.tick);
                Some((rank, actor.person, target, region, pressured))
            })
            .collect();
        if candidates.is_empty() {
            return vec![];
        }
        // A proposal wins only if both people choose the same highest-priority
        // incident. This gives disjoint meetings without a serial matching pass.
        let mut endpoints: Vec<_> = candidates
            .par_iter()
            .enumerate()
            .flat_map_iter(|(index, row)| {
                [
                    (row.1, row.0, row.1, row.2, index),
                    (row.2, row.0, row.1, row.2, index),
                ]
            })
            .collect();
        endpoints.par_sort_unstable();
        let winners: Vec<_> = endpoints
            .par_chunk_by(|a, b| a.0 == b.0)
            .map(|group| (group[0].0, group[0].4))
            .collect();
        candidates
            .into_par_iter()
            .enumerate()
            .filter_map(|(index, (_, actor, target, region, pressured))| {
                let winner = |person| {
                    winners
                        .binary_search_by_key(&person, |row| row.0)
                        .ok()
                        .is_some_and(|i| winners[i].1 == index)
                };
                (winner(actor) && winner(target)).then_some(Encounter {
                    tick: self.tick,
                    region,
                    a: actor.min(target),
                    b: actor.max(target),
                    pressured: pressured.then_some(target),
                })
            })
            .collect()
    }
}
