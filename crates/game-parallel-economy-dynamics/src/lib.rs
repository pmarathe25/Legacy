//! Personal encounters and infectious disease over the economic world's people.
use game_parallel_disease::Disease;
use game_parallel_economy_person::Person;
use game_parallel_social::{Contact, Social};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
mod friendship;
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
    ($world:expr, $person:expr, $busy:expr, $tick:expr, $treatment:expr, $physical:expr, $care:expr, $interaction:expr, $medical:expr, $combat:expr) => {{
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
                    ($world.seed, $tick, $world.travel.as_ref()),
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
        clock: (
            u64,
            u64,
            Option<&game_parallel_economy_person::travel::Travel>,
        ),
    ) -> Result<Self, String> {
        let (seed, tick, travel) = clock;
        let (exposure, requested, combat) = encounters;
        let mut occupied: Vec<_> = exposure.par_iter().flat_map_iter(|e| [e.a, e.b]).collect();
        occupied.par_sort_unstable();
        let locality = self
            .social
            .as_ref()
            .and_then(|_| travel_social::Locality::new(old, travel));
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
                    seed,
                    tick,
                    requested.as_ref(),
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
        let combined: Option<Vec<_>> = (!exposure.is_empty()).then(|| {
            meetings
                .par_iter()
                .chain(exposure.par_iter())
                .cloned()
                .collect()
        });
        let disease = self
            .disease
            .as_ref()
            .map(|disease| {
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
        if let Some(disease) = &disease {
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
