//! Personal encounters and infectious disease over the economic world's people.
use game_parallel_disease::Disease;
use game_parallel_economy_person::Person;
use game_parallel_social::{Contact, Social};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
mod friendship;
mod mission_meetings;
mod openings;
mod pressure;
mod travel_social;
pub use game_parallel_economy_claims as claims;
pub use game_parallel_economy_treatment as treatment;
pub use game_parallel_economy_treatment::{configure_remedies, prepare_treatment};
pub use game_parallel_personality as personality;
pub use treatment::Policy as TreatmentPolicy;
pub fn participant(person: u64, willing: bool) -> game_parallel_social::Participant {
    game_parallel_social::Participant { person, willing }
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $ids:expr, $goods:expr, $view:expr, $physical:expr) => {
        if let Some(dynamics) = &$state.dynamics {
            dynamics.validate(&$ids, &$goods, &$view, $physical.tick())?;
        }
    };
}
/// Keep promise and meeting observations from this boundary together.
pub fn learn_social(
    opinions: &mut Option<game_parallel_opinions::Opinions>,
    dynamics: Option<&Dynamics>,
    tick: u64,
) {
    let Some((opinions, social)) = opinions
        .as_mut()
        .zip(dynamics.and_then(|d| d.social.as_ref()))
    else {
        return;
    };
    if (opinions.rules.encounter_gain == 0 && !opinions.tracks_affinity_encounters())
        || social.events.is_empty()
    {
        return;
    }
    let previous = std::mem::take(&mut opinions.events);
    opinions.advance_with_social(&[], &[], &social.events, tick);
    opinions.events.extend(previous);
}
pub fn next_opinions(
    opinions: &Option<game_parallel_opinions::Opinions>,
) -> Option<game_parallel_opinions::Opinions> {
    opinions.as_ref().map(|old| {
        let mut next = old.clone();
        next.events.clear();
        next
    })
}
/// Personal sensitivity records may each own private relationship cases.
pub fn reclaim_opinions(owner: Option<game_parallel_opinions::Opinions>) {
    if let Some(mut owner) = owner {
        let sensitivities = std::mem::take(&mut owner.sensitivities);
        if let Ok(rows) = std::sync::Arc::try_unwrap(sensitivities) {
            rows.into_par_iter().for_each(drop);
        }
        drop(owner);
    }
}

#[macro_export]
macro_rules! social_choice_method {
    () => {
        /// Change this person's own inclination to seek voluntary meetings.
        pub fn set_social_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
            if !self
                .people
                .iter()
                .any(|row| row.id == person && row.health > 0)
            {
                return Err("person must be alive".into());
            }
            let participant = self
                .dynamics
                .as_mut()
                .and_then(|owner| owner.social.as_mut())
                .and_then(|social| {
                    social
                        .participants
                        .iter_mut()
                        .find(|row| row.person == person)
                })
                .ok_or("person has no social preference")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            participant.willing = willing;
            self.epoch = epoch;
            Ok(())
        }
        /// Change this person's own inclination to yield to pressure.
        pub fn set_social_pressure(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            if !self
                .people
                .iter()
                .any(|row| row.id == person && row.health > 0)
            {
                return Err("person must be alive".into());
            }
            let policy = self
                .dynamics
                .as_mut()
                .and_then(|owner| owner.social.as_mut())
                .and_then(|social| social.pressures.iter_mut().find(|row| row.person == person))
                .ok_or("person has no social pressure preference")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            policy.enabled = enabled;
            self.epoch = epoch;
            Ok(())
        }
        /// Change this person's own inclination to take their medicine.
        pub fn set_treatment_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
            if !self
                .people
                .iter()
                .any(|row| row.id == person && row.health > 0)
            {
                return Err("person must be alive".into());
            }
            let policy = self
                .dynamics
                .as_mut()
                .and_then(|owner| owner.treatment.iter_mut().find(|row| row.person == person))
                .ok_or("person has no treatment preference")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            policy.willing = willing;
            self.epoch = epoch;
            Ok(())
        }
    };
}
#[macro_export]
macro_rules! prepare_dynamics {
    ($world:expr, $person:expr, $busy:expr, $tick:expr, $treatment:expr, $physical:expr, $care:expr, $interaction:expr, $medical:expr, $combat:expr, $partition:expr, $contact:expr, $speak:expr, $opening:expr) => {{
        let exposure = $world
            .dynamics
            .as_ref()
            .filter(|owner| owner.disease.is_some())
            .map_or_else(Vec::new, |_| {
                game_parallel_economy_exposure::prepare(
                    $physical.events(),
                    $care.as_ref(),
                    &$world.people,
                    $world.seed,
                    $interaction
                        .as_ref()
                        .filter(|p| p.encounter.is_some())
                        .map_or(&[][..], |p| p.actors.as_slice()),
                    &$medical.contacts($physical.events()),
                    $world.travel.as_ref(),
                )
            });
        let combat_contacts = if $world
            .dynamics
            .as_ref()
            .and_then(|owner| owner.disease.as_ref())
            .is_some_and(|disease| {
                !disease.cases.is_empty()
                    && disease
                        .strains
                        .par_iter()
                        .any(|strain| strain.combat_transmission_per_million > 0)
            }) {
            $combat
                .as_ref()
                .map_or_else(Vec::new, |combat| combat.contacts())
        } else {
            Vec::new()
        };
        let mut next = $world
            .dynamics
            .as_ref()
            .map(|owner| {
                owner.prepare(
                    &$world.people,
                    &mut $person.people,
                    &$busy,
                    $world.opinions.as_ref(),
                    (
                        &exposure,
                        $interaction
                            .as_ref()
                            .and_then(|plan| plan.encounter.as_ref()),
                        &combat_contacts,
                    ),
                    $crate::Clock {
                        seed: $world.seed,
                        tick: $tick,
                        travel: $world.travel.as_ref(),
                        partition: $partition,
                        contact: $contact,
                        speak: $speak,
                        opening: $opening,
                        missions: $world.missions.as_ref(),
                    },
                )
            })
            .transpose()?;
        $crate::treatment::apply(
            next.as_mut().and_then(|d| d.disease.as_mut()),
            &$treatment,
            $physical.events(),
        );
        next
    }};
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dynamics {
    #[serde(default)]
    pub social: Option<Social>,
    #[serde(default)]
    pub personality: Option<personality::Personality>,
    #[serde(default)]
    pub disease: Option<Disease>,
    #[serde(default)]
    pub treatment: Vec<TreatmentPolicy>,
    #[serde(default)]
    pub claims: Option<claims::Claims>,
}
pub struct Clock<'a> {
    pub seed: u64,
    pub tick: u64,
    pub travel: Option<&'a game_parallel_economy_person::travel::Travel>,
    pub partition: Option<&'a (dyn Fn(u64) -> (u64, u64) + Sync)>,
    pub contact: Option<&'a (dyn Fn(u64, u64) -> bool + Sync)>,
    pub speak: Option<&'a (dyn Fn(u64, u64) -> bool + Sync)>,
    pub opening: Option<&'a (dyn Fn(u64) -> Option<(u64, bool)> + Sync)>,
    pub missions: Option<&'a game_parallel_missions::Missions>,
}
impl Dynamics {
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        goods: &HashSet<u32>,
        view: &game_parallel_lots::View,
        tick: u64,
    ) -> Result<(), String> {
        if let Some(social) = &self.social {
            social.validate(people, tick).map_err(str::to_owned)?;
        }
        if let Some(personality) = &self.personality {
            personality.validate(people).map_err(str::to_owned)?;
        }
        if let Some(disease) = &self.disease {
            disease.validate(people, tick)?;
            if disease
                .treatments
                .iter()
                .any(|t| !goods.contains(&t.medicine))
            {
                return Err("unknown medicine".into());
            }
        }
        treatment::validate(&self.treatment, self.disease.as_ref(), people, goods)?;
        if let Some(claims) = &self.claims {
            claims.validate(people, view, tick)?;
        }
        Ok(())
    }
    /// Contacts use tick-start locations and choices; damage publishes with the same draft.
    pub fn prepare(
        &self,
        old: &[Person],
        next: &mut [Person],
        busy: &[u64],
        opinions: Option<&game_parallel_opinions::Opinions>,
        encounters: (
            &[game_parallel_social::Encounter],
            Option<&game_parallel_social::Encounter>,
            &[(u64, u64)],
        ),
        clock: Clock<'_>,
    ) -> Result<Self, String> {
        let Clock {
            seed,
            tick,
            travel,
            partition,
            contact,
            speak,
            opening,
            missions,
        } = clock;
        let (exposure, requested, combat) = encounters;
        let mut occupied: Vec<_> = exposure.par_iter().flat_map_iter(|e| [e.a, e.b]).collect();
        occupied.par_sort_unstable();
        let locality = self
            .social
            .as_ref()
            .and_then(|_| travel_social::Locality::new(old, travel, partition));
        let contacts: Vec<_> = old
            .par_iter()
            .map(|p| Contact {
                person: p.id,
                region: locality
                    .as_ref()
                    .map_or(p.region, |locations| locations.key(p)),
                health: p.health,
                hunger: p.hunger,
                busy: p.controlled
                    || busy.binary_search(&p.id).is_ok()
                    || occupied.binary_search(&p.id).is_ok(),
            })
            .collect();
        let requested = requested.map(|encounter| {
            let mut encounter = encounter.clone();
            if locality.is_some() {
                encounter.region = old
                    .binary_search_by_key(&encounter.a, |p| p.id)
                    .ok()
                    .and_then(|i| contacts[i].region)
                    .expect("validated meeting location");
            }
            encounter
        });
        let mut social = self
            .social
            .as_ref()
            .map(|social| {
                pressure::prepare(
                    social,
                    &contacts,
                    opinions,
                    self.personality.as_ref(),
                    (seed, tick),
                    requested.as_ref(),
                    pressure::Access {
                        speak,
                        opening,
                        missions,
                        people: old,
                    },
                )
            })
            .transpose()
            .map_err(str::to_owned)?;
        if let (Some(locality), Some(social)) = (&locality, &mut social) {
            social
                .events
                .par_iter_mut()
                .for_each(|event| locality.restore(event));
        }
        let disease = self
            .disease
            .as_ref()
            .map(|disease| {
                // With no source case, no encounter or strike can introduce an
                // infection. The host has already validated the staged owners.
                if disease.cases.is_empty() {
                    return Ok(game_parallel_disease::Draft {
                        state: Disease {
                            strains: disease.strains.clone(),
                            treatments: disease.treatments.clone(),
                            cases: Vec::new(),
                            events: Vec::new(),
                        },
                        damage: Vec::new(),
                    });
                }
                let disease_people: Vec<_> = old
                    .par_iter()
                    .map(|p| game_parallel_disease::Person {
                        id: p.id,
                        health: p.health,
                    })
                    .collect();
                let meetings = social
                    .as_ref()
                    .map_or(&[][..], |social| social.events.as_slice());
                // Talking through a barrier is social contact, not the direct
                // physical exposure represented by this disease route.
                let filtered = contact
                    .filter(|can_touch| {
                        meetings
                            .par_iter()
                            .any(|event| !can_touch(event.a, event.b))
                    })
                    .map(|can_touch| {
                        meetings
                            .par_iter()
                            .filter(|event| can_touch(event.a, event.b))
                            .cloned()
                            .collect::<Vec<_>>()
                    });
                let meetings = filtered.as_deref().unwrap_or(meetings);
                let combined: Option<Vec<_>> = (!exposure.is_empty()).then(|| {
                    meetings
                        .par_iter()
                        .chain(exposure.par_iter())
                        .cloned()
                        .collect()
                });
                disease.prepare_validated_with_combat(
                    &disease_people,
                    combined.as_deref().unwrap_or(meetings),
                    combat,
                    seed,
                    tick - 1,
                )
            })
            .transpose()
            .map_err(str::to_owned)?;
        if let Some(disease) = &disease
            && !disease.damage.is_empty()
        {
            let mut damage = disease.damage.clone();
            damage.par_sort_unstable_by_key(|row| row.person);
            next.par_iter_mut().for_each(|person| {
                if let Ok(index) = damage.binary_search_by_key(&person.id, |row| row.person) {
                    person.health = person.health.saturating_sub(damage[index].amount);
                }
            });
        }
        Ok(Self {
            social,
            personality: self.personality.clone(),
            disease: disease.map(|draft| draft.state),
            treatment: self.treatment.clone(),
            claims: self.claims.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use game_parallel_disease::{Case, Event, EventKind, Strain};

    #[test]
    fn empty_disease_fast_path_matches_normal_preparation_and_clears_old_events() {
        let first = Person {
            id: 1,
            region: Some(1),
            health: 100,
            hunger: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        };
        let people = vec![first.clone(), Person { id: 2, ..first }];
        let encounters = [game_parallel_social::Encounter {
            tick: 1,
            region: 1,
            a: 1,
            b: 2,
            pressured: None,
            also_pressured: None,
        }];
        for infected in [false, true] {
            let disease = Disease {
                strains: vec![Strain {
                    id: 1,
                    name: "fever".into(),
                    incubation: 0,
                    infectious_ticks: 3,
                    immunity_ticks: 2,
                    damage_per_tick: 3,
                    transmission_per_million: 1_000_000,
                    combat_transmission_per_million: 0,
                    birth_transmission_per_million: 0,
                }],
                treatments: vec![],
                cases: if infected {
                    vec![Case {
                        person: 1,
                        strain: 1,
                        infected_at: 0,
                        source: None,
                    }]
                } else {
                    vec![]
                },
                events: vec![Event {
                    tick: 0,
                    person: 1,
                    strain: 1,
                    source: None,
                    kind: EventKind::Infected,
                }],
            };
            let expected = disease
                .prepare_validated_with_combat(
                    &[
                        game_parallel_disease::Person { id: 1, health: 100 },
                        game_parallel_disease::Person { id: 2, health: 100 },
                    ],
                    &encounters,
                    &[],
                    7,
                    0,
                )
                .unwrap();
            let dynamics = Dynamics {
                social: None,
                personality: None,
                disease: Some(disease),
                treatment: vec![],
                claims: None,
            };
            let mut next_people = people.clone();
            let result = dynamics
                .prepare(
                    &people,
                    &mut next_people,
                    &[],
                    None,
                    (&encounters, None, &[]),
                    Clock {
                        seed: 7,
                        tick: 1,
                        travel: None,
                        partition: None,
                        contact: None,
                        speak: None,
                        opening: None,
                        missions: None,
                    },
                )
                .unwrap();
            assert_eq!(result.disease.unwrap(), expected.state);
            assert_eq!(next_people[0].health, if infected { 97 } else { 100 });
            assert_eq!(next_people[1].health, 100);
        }
    }
}
