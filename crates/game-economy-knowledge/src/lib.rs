//! Bounded personal observation and reporting over actual dated stock.
use game_economy_person::Person;
use game_families::Families;
use game_knowledge::Observation;
pub use game_knowledge::{Knowledge, Mind, Misreport, PersonReport, Record, Reporting, SeenPerson};
use game_lots::{Place, View};
use game_opinions::Opinions;
use game_production_model::Good;
use game_social::Social;
use rayon::prelude::*;
use std::collections::HashSet;

fn regions(view: &View) -> HashSet<u64> {
    view.containers
        .par_iter()
        .filter_map(|c| match c.place {
            Place::Stock { region, .. } => Some(region),
            _ => None,
        })
        .collect()
}
pub fn validate(
    owner: Option<&Knowledge>,
    people: &HashSet<u64>,
    view: &View,
    tick: u64,
) -> Result<(), String> {
    if let Some(owner) = owner {
        owner
            .validate(people, &regions(view), tick)
            .map_err(str::to_owned)?;
    }
    Ok(())
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $ids:expr, $view:expr, $physical:expr) => {
        $crate::validate($state.knowledge.as_ref(), &$ids, &$view, $physical.tick())?
    };
}
pub fn advance(
    owner: Option<&Knowledge>,
    people: &[Person],
    goods: &[Good],
    view: &View,
    social: Option<&Social>,
    opinions: Option<&Opinions>,
    now: u64,
) -> Result<Option<Knowledge>, String> {
    let Some(owner) = owner else {
        return Ok(None);
    };
    let edible: HashSet<_> = goods
        .iter()
        .filter(|g| g.food.is_some())
        .map(|g| g.id)
        .collect();
    let stocks: Vec<_> = view
        .containers
        .par_iter()
        .filter_map(|c| match c.place {
            Place::Stock { region, good } => Some((
                region,
                if edible.contains(&good) {
                    c.quantity(good)
                } else {
                    0
                },
            )),
            _ => None,
        })
        .collect();
    let food: Vec<_> = stocks
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| {
            let total: u128 = group.par_iter().map(|row| u128::from(row.1)).sum();
            (group[0].0, total.min(u128::from(u32::MAX)) as u32)
        })
        .collect();
    let observations: Vec<_> = people
        .par_iter()
        .map(|p| {
            let local = p
                .region
                .map(|region| {
                    food.binary_search_by_key(&region, |r| r.0)
                        .ok()
                        .map(|i| (region, food[i].1))
                        .ok_or("unknown observation region")
                })
                .transpose()?;
            Ok(Observation {
                person: p.id,
                health: p.health,
                local,
            })
        })
        .collect::<Result<_, &str>>()
        .map_err(str::to_owned)?;
    let trust = opinions.and_then(Opinions::report_filter);
    let pressure = owner
        .minds
        .par_iter()
        .any(|mind| {
            mind.reporting
                .is_some_and(|policy| !policy.willing && policy.pressure_threshold.is_some())
                || mind
                    .people_reporting
                    .is_some_and(|policy| !policy.willing && policy.pressure_threshold.is_some())
        })
        .then(|| opinions.map(Opinions::index))
        .flatten();
    let events = social.map_or(&[][..], |s| s.events.as_slice());
    owner
        .prepare_validated_with_pressure(
            &observations,
            &regions(view),
            events,
            now,
            |receiver, sender| {
                trust
                    .as_ref()
                    .is_none_or(|filter| filter.accepts(receiver, sender))
            },
            |speaker, listener| {
                pressure.as_ref().map_or(0, |index| {
                    let (fear, obligation) = index.feelings(speaker, listener);
                    fear.saturating_add(obligation)
                })
            },
        )
        .map(Some)
        .map_err(str::to_owned)
}
pub fn inherit(knowledge: &mut Option<Knowledge>, family: &Families, births: &[u64]) {
    let Some(owner) = knowledge
        .as_mut()
        .filter(|_| family.rules.enroll_births && !births.is_empty())
    else {
        return;
    };
    owner
        .minds
        .par_extend(births.par_iter().map(|&person| Mind {
            person,
            reporting: None,
            remember_people: false,
            people_reporting: None,
            people_reports: vec![],
            people_seen: vec![],
            misreport: None,
            records: vec![],
        }));
}
/// Reclaim independent personal record allocations as part of the committed boundary.
pub fn reclaim(owner: Option<Knowledge>) {
    if let Some(owner) = owner {
        owner.minds.into_par_iter().for_each(drop);
    }
}
/// Compare only each living person's direct local observation with their own earlier report.
pub fn discrepancies(before: &Knowledge, after: &Knowledge, people: &[Person]) -> Vec<(u64, u64)> {
    let dense = people.first().zip(people.last()).and_then(|(first, last)| {
        (last.id.checked_sub(first.id) == Some((people.len() - 1) as u64)).then_some(first.id)
    });
    before
        .minds
        .par_iter()
        .zip(&after.minds)
        .filter_map(|(old, next)| {
            let index = if let Some(first) = dense {
                old.person
                    .checked_sub(first)
                    .and_then(|i| usize::try_from(i).ok())
                    .filter(|&i| i < people.len())
            } else {
                people.binary_search_by_key(&old.person, |p| p.id).ok()
            }?;
            let person = &people[index];
            if person.health == 0 {
                return None;
            }
            let region = person.region?;
            let current = next.records.iter().find(|r| {
                r.region == region && r.learned_from.is_none() && r.witness == person.id
            })?;
            game_knowledge::report_discrepancy(
                &old.records,
                Observation {
                    person: person.id,
                    health: person.health,
                    local: Some((region, current.food)),
                },
            )
        })
        .collect()
}
#[macro_export]
macro_rules! prepare_world {
    ($world:expr, $dynamics:expr) => {{
        if $world.knowledge.is_some() {
            let view = $world.physical.stock_view($world.tick())?;
            $crate::advance(
                $world.knowledge.as_ref(),
                &$world.people,
                $world.physical.goods(),
                &view,
                $dynamics.as_ref().and_then(|d| d.social.as_ref()),
                $world.opinions.as_ref(),
                $world.tick(),
            )?
        } else {
            None
        }
    }};
}
#[macro_export]
macro_rules! knowledge_methods {
    () => {
        /// Set only this person's inclination to remember new local encounters.
        pub fn set_remember_people(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("unknown or deceased person".into());
            }
            let owner = self.knowledge.as_mut().ok_or("knowledge is disabled")?;
            let mind = owner
                .minds
                .iter_mut()
                .find(|mind| mind.person == person)
                .ok_or("person has no mind")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            mind.remember_people = enabled;
            self.epoch = epoch;
            Ok(())
        }
        pub fn known_people(&self, person: u64) -> Result<Vec<$crate::SeenPerson>, String> {
            self.knowledge
                .as_ref()
                .ok_or("knowledge is disabled")?
                .minds
                .iter()
                .find(|mind| mind.person == person)
                .map(|mind| mind.people_seen.clone())
                .ok_or("person has no mind".into())
        }
        /// Choose independently whether to share firsthand encounter memories.
        pub fn set_people_reporting(
            &mut self,
            person: u64,
            choice: Option<$crate::Reporting>,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("unknown or deceased person".into());
            }
            let mind = self
                .knowledge
                .as_mut()
                .ok_or("knowledge is disabled")?
                .minds
                .iter_mut()
                .find(|mind| mind.person == person)
                .ok_or("person has no mind")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            mind.people_reporting = choice;
            self.epoch = epoch;
            Ok(())
        }
        pub fn people_reports(&self, person: u64) -> Result<Vec<$crate::PersonReport>, String> {
            self.knowledge
                .as_ref()
                .ok_or("knowledge is disabled")?
                .minds
                .iter()
                .find(|mind| mind.person == person)
                .map(|mind| mind.people_reports.clone())
                .ok_or("person has no mind".into())
        }
        /// Set this person's own willingness and pressure response for future reports.
        pub fn set_food_reporting(
            &mut self,
            person: u64,
            choice: Option<$crate::Reporting>,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("unknown or deceased person".into());
            }
            let owner = self.knowledge.as_mut().ok_or("knowledge is disabled")?;
            let mind = owner
                .minds
                .iter_mut()
                .find(|m| m.person == person)
                .ok_or("person has no mind")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            mind.reporting = choice;
            self.epoch = epoch;
            Ok(())
        }
        pub fn food_reporting(&self, person: u64) -> Result<Option<$crate::Reporting>, String> {
            self.knowledge
                .as_ref()
                .ok_or("knowledge is disabled")?
                .minds
                .iter()
                .find(|mind| mind.person == person)
                .map(|mind| mind.reporting)
                .ok_or("person has no mind".into())
        }
        /// Change only this person's future statements; their own memories stay intact.
        pub fn set_food_misreport(
            &mut self,
            person: u64,
            choice: Option<$crate::Misreport>,
        ) -> Result<(), String> {
            if !self.people.iter().any(|p| p.id == person && p.health > 0) {
                return Err("unknown or deceased person".into());
            }
            if choice.and_then(|c| c.target).is_some_and(|target| {
                target == person || !self.people.iter().any(|p| p.id == target)
            }) {
                return Err("invalid report target".into());
            }
            let owner = self.knowledge.as_mut().ok_or("knowledge is disabled")?;
            let mind = owner
                .minds
                .iter_mut()
                .find(|m| m.person == person)
                .ok_or("person has no mind")?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            mind.misreport = choice;
            self.epoch = epoch;
            Ok(())
        }
        pub fn food_misreport(&self, person: u64) -> Result<Option<$crate::Misreport>, String> {
            self.knowledge
                .as_ref()
                .ok_or("knowledge is disabled")?
                .minds
                .iter()
                .find(|m| m.person == person)
                .map(|m| m.misreport)
                .ok_or("person has no mind".into())
        }
        pub fn known_food(&self, person: u64) -> Result<Vec<$crate::Record>, String> {
            self.knowledge
                .as_ref()
                .ok_or("knowledge is disabled")?
                .minds
                .iter()
                .find(|m| m.person == person)
                .map(|mind| mind.records.clone())
                .ok_or("person has no mind".into())
        }
        /// Access a past mind only while its life is actively invoked and the player has control.
        pub fn invoked_knowledge(&self, life: u64) -> Result<Vec<$crate::Record>, String> {
            let campaign = self.campaign.as_ref().ok_or("campaign is disabled")?;
            let current = campaign
                .lineage
                .current
                .ok_or("there is no current ruler")?;
            if campaign.lineage.controlled_person(self.tick()) != Some(current) {
                return Err("player control is suspended".into());
            }
            let invocation = campaign
                .lineage
                .invocation
                .as_ref()
                .ok_or("invocation is disabled")?;
            if !invocation.active.contains(&life) {
                return Err("past life is not invoked".into());
            }
            self.known_food(life)
        }
    };
}
