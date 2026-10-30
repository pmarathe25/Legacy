//! Bounded personal observation and reporting over actual dated stock.
use game_parallel_economy_person::Person;
use game_parallel_families::Families;
use game_parallel_knowledge::Observation;
pub use game_parallel_knowledge::{Knowledge, Mind, Record};
use game_parallel_lots::{Place, View};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::Good;
use game_parallel_social::Social;
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
    let trust = opinions
        .filter(|o| o.rules.minimum_report_trust.is_some())
        .map(|o| (o.index(), o.rules.minimum_report_trust.unwrap()));
    let events = social.map_or(&[][..], |s| s.events.as_slice());
    owner
        .prepare_validated_with(
            &observations,
            &regions(view),
            events,
            now,
            |receiver, sender| {
                trust
                    .as_ref()
                    .is_none_or(|(index, threshold)| index.trust(receiver, sender) >= *threshold)
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
            records: vec![],
        }));
}
#[macro_export]
macro_rules! prepare_world {
    ($world:expr, $dynamics:expr) => {{
        if $world.knowledge.is_some() {
            let view = $world.physical.view($world.tick())?;
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
