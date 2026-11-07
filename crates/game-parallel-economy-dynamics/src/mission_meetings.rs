//! Deliberate meeting objectives remain individual, local social choices.
use game_parallel_economy_person::Person;
use game_parallel_missions::{Missions, Objective, Phase};
use game_parallel_opinions::Opinions;
use game_parallel_social::{Contact, Encounter, Social};
use rayon::prelude::*;

fn dense_base(first: Option<u64>, last: Option<u64>, len: usize) -> Option<u64> {
    first.zip(last).and_then(|(first, last)| {
        (last.checked_sub(first) == u64::try_from(len.saturating_sub(1)).ok()).then_some(first)
    })
}

fn dense_offset(base: u64, person: u64) -> Option<usize> {
    person
        .checked_sub(base)
        .and_then(|value| usize::try_from(value).ok())
}

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
        if participants
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            participants.par_sort_unstable_by_key(|row| row.person);
        }
        let mut policies: Vec<_> = self.social.pressures.par_iter().collect();
        if policies
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            policies.par_sort_unstable_by_key(|row| row.person);
        }
        let feelings = self
            .opinions
            .filter(|_| !policies.is_empty())
            .map(Opinions::index);
        let contact_base = dense_base(
            self.contacts.first().map(|row| row.person),
            self.contacts.last().map(|row| row.person),
            self.contacts.len(),
        );
        let participant_base = dense_base(
            participants.first().map(|row| row.person),
            participants.last().map(|row| row.person),
            participants.len(),
        );
        let people_base = dense_base(
            self.people.first().map(|row| row.id),
            self.people.last().map(|row| row.id),
            self.people.len(),
        );
        let find_contact = |id| {
            let index = contact_base.map_or_else(
                || {
                    self.contacts
                        .binary_search_by_key(&id, |row| row.person)
                        .ok()
                },
                |base| dense_offset(base, id),
            )?;
            self.contacts
                .get(index)
                .copied()
                .filter(|row| row.person == id)
        };
        let find_participant = |id| {
            let index = participant_base.map_or_else(
                || {
                    participants
                        .binary_search_by_key(&id, |row| row.person)
                        .ok()
                },
                |base| dense_offset(base, id),
            )?;
            participants
                .get(index)
                .copied()
                .filter(|row| row.person == id)
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
                let index = people_base.map_or_else(
                    || {
                        self.people
                            .binary_search_by_key(&mission.person, |row| row.id)
                            .ok()
                    },
                    |base| dense_offset(base, mission.person),
                )?;
                let person = self
                    .people
                    .get(index)
                    .filter(|row| row.id == mission.person)?;
                if person.region != Some(mission.destination) {
                    return None;
                }
                let actor = find_contact(mission.person)?;
                let target_contact = find_contact(target)?;
                let region = actor.region?;
                if !eligible(actor)
                    || !eligible(target_contact)
                    || (target_contact.region != Some(region)
                        && !self
                            .speak
                            .is_some_and(|can_speak| can_speak(actor.person, target)))
                {
                    return None;
                }
                let responds = |person, other| {
                    policies
                        .binary_search_by_key(&person, |row| row.person)
                        .ok()
                        .is_some_and(|index| {
                            feelings.as_ref().is_some_and(|opinions| {
                                let (fear, obligation) = opinions.feelings(person, other);
                                policies[index].responds(fear, obligation)
                            })
                        })
                };
                let actor_willing = find_participant(actor.person)?.willing;
                let target_willing = find_participant(target)?.willing;
                let actor_pressured = !actor_willing && responds(actor.person, target);
                let target_pressured = !target_willing && responds(target, actor.person);
                if !(actor_willing || actor_pressured) || !(target_willing || target_pressured) {
                    return None;
                }
                let rank =
                    game_parallel_genetics::random(self.seed ^ actor.person, target, self.tick);
                Some((
                    rank,
                    actor.person,
                    target,
                    region,
                    actor_pressured,
                    target_pressured,
                ))
            })
            .collect();
        if candidates.is_empty() {
            return vec![];
        }
        let event =
            |actor: u64, target: u64, region, actor_pressured: bool, target_pressured: bool| {
                Encounter {
                    tick: self.tick,
                    region,
                    a: actor.min(target),
                    b: actor.max(target),
                    pressured: actor_pressured
                        .then_some(actor)
                        .or_else(|| target_pressured.then_some(target)),
                    also_pressured: (actor_pressured && target_pressured).then_some(target),
                }
            };
        let shared_target = candidates[0].2;
        if candidates.par_iter().all(|row| row.2 == shared_target) {
            // Each active mission has a distinct actor, and no one can target
            // themselves. Only the target's deterministic choice is contested.
            let &(_, actor, target, region, actor_pressured, target_pressured) = candidates
                .par_iter()
                .min_by_key(|row| (row.0, row.1, row.2))
                .expect("nonempty candidates");
            return vec![event(
                actor,
                target,
                region,
                actor_pressured,
                target_pressured,
            )];
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
            .filter_map(
                |(index, (_, actor, target, region, actor_pressured, target_pressured))| {
                    let winner = |person| {
                        winners
                            .binary_search_by_key(&person, |row| row.0)
                            .ok()
                            .is_some_and(|i| winners[i].1 == index)
                    };
                    (winner(actor) && winner(target))
                        .then(|| event(actor, target, region, actor_pressured, target_pressured))
                },
            )
            .collect()
    }
}
