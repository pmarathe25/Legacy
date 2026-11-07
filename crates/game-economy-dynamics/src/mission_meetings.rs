//! Deliberate meeting objectives remain individual, local social choices.
use game_economy_person::Person;
use game_missions::{Missions, Objective, Phase};
use game_opinions::Opinions;
use game_social::{Contact, Encounter, Social};
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
    pub inquiry_meetings: &'a [(u64, u64)],
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
        let missions = self
            .missions
            .map_or(&[][..], |owner| owner.entries.as_slice());
        if !self.tick.is_multiple_of(self.social.rules.interval)
            || (self.inquiry_meetings.is_empty()
                && !missions.par_iter().any(|mission| {
                    !mission.objective_met
                        && matches!(mission.objective, Some(Objective::Meet { .. }))
                        && matches!(
                            mission.phase,
                            Phase::Outbound | Phase::Visiting | Phase::Returning
                        )
                }))
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
        let make_candidate = |actor_id: u64, target: u64| {
            if actor_id == target {
                return None;
            }
            let actor = find_contact(actor_id)?;
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
            let rank = game_genetics::random(self.seed ^ actor.person, target, self.tick);
            Some((
                rank,
                actor.person,
                target,
                region,
                actor_pressured,
                target_pressured,
            ))
        };
        let mut candidates: Vec<_> = missions
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
                make_candidate(mission.person, target)
            })
            .collect();
        candidates.par_extend(
            self.inquiry_meetings
                .par_iter()
                .filter_map(|&(actor, target)| make_candidate(actor, target)),
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use game_social::{Participant, Rules};

    #[test]
    fn deliberate_inquiry_meetings_are_disjoint_and_worker_invariant() {
        let count = 10_000u64;
        let contacts: Vec<_> = (1..=count + 1)
            .map(|person| Contact {
                person,
                region: Some(1),
                health: 100,
                hunger: 0,
                busy: false,
            })
            .collect();
        let social = Social {
            rules: Rules {
                interval: 1,
                min_health: 1,
                max_hunger: 10,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: contacts
                .iter()
                .map(|row| Participant {
                    person: row.person,
                    willing: true,
                })
                .collect(),
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        };
        let run = |workers, intents: &[(u64, u64)]| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    Context {
                        missions: None,
                        inquiry_meetings: intents,
                        people: &[],
                        contacts: &contacts,
                        social: &social,
                        opinions: None,
                        occupied: &[],
                        speak: None,
                        seed: 7,
                        tick: 1,
                    }
                    .select()
                })
        };
        let disjoint: Vec<_> = (0..count / 2).map(|i| (i * 2 + 1, i * 2 + 2)).collect();
        let mut reversed = disjoint.clone();
        reversed.reverse();
        let first = run(1, &disjoint);
        assert_eq!(first.len(), count as usize / 2);
        let canonical = |mut rows: Vec<Encounter>| {
            rows.sort_unstable_by_key(|e| (e.a, e.b));
            rows
        };
        assert_eq!(canonical(first.clone()), canonical(run(4, &reversed)));
        assert_eq!(canonical(first), canonical(run(6, &disjoint)));
        let shared: Vec<_> = (1..=count).map(|person| (person, count + 1)).collect();
        let mut shared_reverse = shared.clone();
        shared_reverse.reverse();
        let first = run(1, &shared);
        assert_eq!(first.len(), 1);
        assert_eq!(first, run(4, &shared_reverse));
        assert_eq!(first, run(6, &shared));
        let mut refusing = social.clone();
        refusing.participants[1].willing = false;
        let refused = Context {
            missions: None,
            inquiry_meetings: &[(1, 2)],
            people: &[],
            contacts: &contacts,
            social: &refusing,
            opinions: None,
            occupied: &[],
            speak: None,
            seed: 7,
            tick: 1,
        }
        .select();
        assert!(refused.is_empty());
        let mut busy = contacts.clone();
        busy[1].busy = true;
        let unavailable = Context {
            missions: None,
            inquiry_meetings: &[(1, 2)],
            people: &[],
            contacts: &busy,
            social: &social,
            opinions: None,
            occupied: &[],
            speak: None,
            seed: 7,
            tick: 1,
        }
        .select();
        assert!(unavailable.is_empty());
    }
}
