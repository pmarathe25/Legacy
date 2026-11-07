//! Choose disjoint local encounters using each individual's directed affinity.
use game_opinions::Opinions;
use game_social::{Contact, Encounter, Social};
use rayon::prelude::*;

pub struct Context<'a> {
    pub social: &'a Social,
    pub contacts: &'a [Contact],
    pub opinions: Option<&'a Opinions>,
    pub needs: &'a [(u64, u32)],
    pub occupied: &'a [u64],
    pub seed: u64,
    pub tick: u64,
    pub can_speak: Option<&'a (dyn Fn(u64, u64) -> bool + Sync)>,
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
        let mut weights: Vec<_> = opinions
            .sensitivities
            .par_iter()
            .filter_map(|row| {
                row.social_affinity_weight
                    .map(|weight| (row.person, weight))
            })
            .collect();
        if weights.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            weights.par_sort_unstable_by_key(|row| row.0);
        }
        let mut cases: Vec<_> = opinions
            .sensitivities
            .par_iter()
            .flat_map_iter(|person| {
                person.relationship_cases.iter().filter_map(move |case| {
                    case.social_affinity_weight
                        .map(|weight| ((person.person, case.target), weight))
                })
            })
            .collect();
        if cases.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
            cases.par_sort_unstable_by_key(|row| row.0);
        }
        let score = |observer, target, affinity: i32| {
            let specific = cases
                .binary_search_by_key(&(observer, target), |row| row.0)
                .ok()
                .map(|index| cases[index].1);
            let general = || {
                weights
                    .binary_search_by_key(&observer, |row| row.0)
                    .ok()
                    .map(|index| weights[index].1)
            };
            i64::from(affinity) * i64::from(specific.or_else(general).unwrap_or(1))
        };
        let check_contact = |contact: Contact| {
            let id = contact.person;
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
        // Availability and personal willingness are constant for this
        // boundary. Evaluate them once per person when the opinion graph is
        // larger than the population, rather than once per incident edge.
        let eligible_contacts: Option<Vec<_>> = (opinions.records.len() >= self.contacts.len())
            .then(|| {
                self.contacts
                    .par_iter()
                    .copied()
                    .map(check_contact)
                    .collect()
            });
        let dense_base =
            self.contacts
                .first()
                .zip(self.contacts.last())
                .and_then(|(first, last)| {
                    (last.person.checked_sub(first.person)
                        == Some(self.contacts.len().saturating_sub(1) as u64)
                        && self
                            .contacts
                            .par_windows(2)
                            .all(|pair| pair[0].person < pair[1].person))
                    .then_some(first.person)
                });
        let eligible = |id: u64| {
            let index = if let Some(base) = dense_base {
                usize::try_from(id.checked_sub(base)?).ok()?
            } else {
                self.contacts.binary_search_by_key(&id, |c| c.person).ok()?
            };
            let contact = *self.contacts.get(index)?;
            if contact.person != id {
                return None;
            }
            eligible_contacts
                .as_ref()
                .map_or_else(|| check_contact(contact), |rows| rows[index])
        };
        let mut edges: Vec<_> = opinions
            .records
            .par_iter()
            .filter_map(|record| {
                if score(record.observer, record.target, record.affinity) <= 0 {
                    return None;
                }
                let a = record.observer.min(record.target);
                let b = record.observer.max(record.target);
                let left = eligible(a)?;
                let right = eligible(b)?;
                let region = left.region?;
                if right.region != Some(region)
                    && !self.can_speak.is_some_and(|access| access(a, b))
                {
                    return None;
                }
                let forward = score(a, b, index.affinity(a, b));
                let reverse = score(b, a, index.affinity(b, a));
                Some((
                    a,
                    b,
                    region,
                    forward,
                    reverse,
                    game_genetics::random(self.seed ^ a, b, self.tick),
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
                    also_pressured: None,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_opinions::{AffinityRules, Opinion, Rules as OpinionRules};
    use game_social::{Participant, Rules as SocialRules};

    #[test]
    fn availability_cache_preserves_directed_affinity_and_personal_refusal() {
        let contacts: Vec<_> = [1, 3, 9]
            .into_iter()
            .map(|person| Contact {
                person,
                region: Some(1),
                health: 100,
                hunger: 0,
                busy: false,
            })
            .collect();
        let mut social = Social {
            rules: SocialRules {
                interval: 1,
                min_health: 1,
                max_hunger: 10,
                familiarity_gain: 1,
                familiarity_max: 10,
                relationship_retention: None,
            },
            participants: contacts
                .iter()
                .map(|c| Participant {
                    person: c.person,
                    willing: true,
                })
                .collect(),
            pressures: vec![],
            relationships: vec![],
            events: vec![],
            needs: None,
        };
        let records = [(1, 3, 5), (3, 1, -3), (1, 9, 3), (9, 1, 1)]
            .into_iter()
            .map(|(observer, target, affinity)| Opinion {
                observer,
                target,
                trust: 0,
                fear: 0,
                obligation: 0,
                affinity,
            })
            .collect();
        let mut opinions = Opinions {
            rules: OpinionRules {
                minimum: -10,
                maximum: 10,
                initial: 0,
                fulfilled_gain: 0,
                default_loss: 0,
                theft_loss: 0,
                encounter_gain: 0,
                fear_per_damage: 0,
                fear_decay_per_tick: 0,
                gratitude_gain: 0,
                obligation_relief: 0,
                minimum_report_trust: None,
                affinity: Some(AffinityRules {
                    limit: 10,
                    encounter_change: 0,
                    pressured_encounter_change: 0,
                    help_change: 0,
                    harm_change_per_damage: 0,
                }),
                newborn_obligation_development: None,
            },
            sensitivities: Default::default(),
            records,
            events: vec![],
        };
        let run = |workers, social: &Social, opinions: &Opinions, contacts: &[Contact]| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(|| {
                    Context {
                        social,
                        contacts,
                        opinions: Some(opinions),
                        needs: &[],
                        occupied: &[],
                        seed: 7,
                        tick: 1,
                        can_speak: None,
                    }
                    .prepare(&|_, _| true)
                })
        };
        let first = run(1, &social, &opinions, &contacts);
        assert_eq!(
            first.iter().map(|e| (e.a, e.b)).collect::<Vec<_>>(),
            vec![(1, 3)]
        );
        opinions.records.reverse();
        assert_eq!(first, run(4, &social, &opinions, &contacts));
        social.participants[1].willing = false;
        assert_eq!(
            run(4, &social, &opinions, &contacts)
                .iter()
                .map(|e| (e.a, e.b))
                .collect::<Vec<_>>(),
            vec![(1, 9)]
        );
        let mut busy = contacts.clone();
        busy[0].busy = true;
        assert!(run(4, &social, &opinions, &busy).is_empty());
    }
}
