//! Individual travelers may choose to obtain missing goods from local stock.
use crate::{MissionPhase, Missions, Objective};
use game_parallel_economy_claims::{Index as Claims, Use};
use game_parallel_economy_person::Person;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_opinions::Opinions;
use rayon::prelude::*;

pub struct Context<'a> {
    pub missions: Option<&'a Missions>,
    pub people: &'a [Person],
    pub view: &'a View,
    pub opinions: Option<&'a Opinions>,
    pub claims: Option<&'a Claims<'a>>,
    pub busy: &'a [u64],
    pub max_hunger: u32,
}
pub struct Plan {
    pub actions: Vec<Transaction>,
    pub actors: Vec<u64>,
    pub uses: Vec<Use>,
}

impl Context<'_> {
    pub fn prepare(&self, base: u64) -> Result<Plan, String> {
        let entries = self
            .missions
            .map_or(&[][..], |missions| missions.entries.as_slice());
        if !entries.par_iter().any(|mission| {
            !mission.objective_met
                && mission
                    .delivery_preference
                    .and_then(|p| p.acquisition)
                    .is_some_and(|choice| choice.willing || choice.pressure_threshold.is_some())
        }) {
            return Ok(Plan {
                actions: vec![],
                actors: vec![],
                uses: vec![],
            });
        }
        let needs_opinions = entries.par_iter().any(|mission| {
            mission
                .delivery_preference
                .and_then(|preference| preference.acquisition)
                .is_some_and(|choice| choice.pressure_threshold.is_some())
        });
        let opinions = self
            .opinions
            .filter(|_| needs_opinions)
            .map(Opinions::index);
        let mut rows: Vec<_> = entries
            .par_iter()
            .filter_map(|mission| {
                if mission.objective_met
                    || matches!(
                        mission.phase,
                        MissionPhase::Completed | MissionPhase::Failed
                    )
                {
                    return None;
                }
                let preference = mission.delivery_preference?;
                let choice = preference.acquisition?;
                let Some(Objective::Deliver {
                    person: recipient,
                    good,
                    quantity,
                    delivered,
                }) = mission.objective
                else {
                    return None;
                };
                let person = self
                    .people
                    .binary_search_by_key(&mission.person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])?;
                let region = person.region?;
                if person.controlled
                    || person.health == 0
                    || person.hunger > self.max_hunger
                    || self.busy.binary_search(&person.id).is_ok()
                    || (mission.phase == MissionPhase::Returning && region == mission.origin)
                {
                    return None;
                }
                let pressured = choice.pressure_threshold.is_some_and(|threshold| {
                    opinions.as_ref().is_some_and(|index| {
                        let (fear, obligation) = index.feelings(person.id, recipient);
                        fear.saturating_add(obligation) >= threshold
                    })
                });
                if !(choice.willing || pressured)
                    || (!choice.disregard_claims
                        && self
                            .claims
                            .is_some_and(|index| !index.allows(person.id, region, good)))
                {
                    return None;
                }
                let holder = self.view.container(Place::Person(person.id))?;
                let stock = self.view.container(Place::Stock { region, good })?;
                let needed = quantity
                    .saturating_sub(delivered)
                    .saturating_add(preference.reserve)
                    .saturating_sub(holder.quantity(good));
                let quantity = needed
                    .min(stock.quantity(good))
                    .min(holder.capacity.saturating_sub(holder.used));
                (quantity > 0).then_some((person.id, region, good, quantity))
            })
            .collect();
        rows.par_sort_unstable_by_key(|row| row.0);
        base.checked_add(rows.len() as u64)
            .ok_or("delivery acquisition action overflow")?;
        let actions = rows
            .par_iter()
            .enumerate()
            .map(|(index, &(person, region, good, quantity))| {
                let id = base + index as u64;
                Transaction {
                    id,
                    movements: vec![Movement {
                        id,
                        from: Place::Stock { region, good },
                        to: Some(Place::Person(person)),
                        good,
                        quantity,
                    }],
                    creations: vec![],
                }
            })
            .collect();
        let actors = rows.par_iter().map(|row| row.0).collect();
        let uses = rows
            .par_iter()
            .enumerate()
            .map(|(index, &(person, region, good, quantity))| Use {
                action: base + index as u64,
                person,
                region,
                good,
                quantity,
                job: false,
            })
            .collect();
        Ok(Plan {
            actions,
            actors,
            uses,
        })
    }
}
