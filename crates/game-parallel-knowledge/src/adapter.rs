//! Adapt physical observations and encounter evidence to the knowledge owner.
use crate::{Knowledge, Observation};
use game_parallel_people::Region;
use game_parallel_social::Social;
use game_parallel_transport::Traveler;
use rayon::prelude::*;
use std::collections::HashSet;
pub fn prepare(
    knowledge: Option<&Knowledge>,
    regions: &[Region],
    travelers: &[Traveler],
    social: Option<&Social>,
    tick: u64,
) -> Result<Option<Knowledge>, &'static str> {
    let Some(knowledge) = knowledge else {
        return Ok(None);
    };
    let observations: Vec<_> = regions
        .par_iter()
        .flat_map(|region| {
            region.people.par_iter().map(|p| Observation {
                person: p.id,
                health: p.health,
                local: Some((region.id, region.food)),
            })
        })
        .chain(travelers.par_iter().map(|t| Observation {
            person: t.person.id,
            health: t.person.health,
            local: None,
        }))
        .collect();
    let region_ids: HashSet<_> = regions.iter().map(|r| r.id).collect();
    if region_ids.len() != regions.len() {
        return Err("duplicate region id");
    }
    let encounters = social.map_or(&[][..], |s| s.events.as_slice());
    knowledge
        .prepare(&observations, &region_ids, encounters, tick)
        .map(Some)
}
/// Prepare knowledge from indexed people and start-of-tick regional food.
pub fn prepare_indexed(
    knowledge: &Knowledge,
    regions: &[Region],
    food: &[u32],
    people: &[(u64, Option<u64>, u32, u32)],
    social: Option<&Social>,
    tick: u64,
) -> Result<Knowledge, &'static str> {
    use std::collections::HashMap;
    let stocks: HashMap<_, _> = regions
        .iter()
        .zip(food)
        .map(|(region, &food)| (region.id, food))
        .collect();
    if stocks.len() != regions.len() || food.len() != regions.len() {
        return Err("invalid regional food index");
    }
    let observations: Vec<_> = people
        .par_iter()
        .map(
            |&(person, region, health, _)| -> Result<Observation, &'static str> {
                let local = region
                    .map(|region| {
                        stocks
                            .get(&region)
                            .copied()
                            .map(|food| (region, food))
                            .ok_or("unknown observation region")
                    })
                    .transpose()?;
                Ok(Observation {
                    person,
                    health,
                    local,
                })
            },
        )
        .collect::<Result<_, _>>()?;
    let ids = stocks.keys().copied().collect();
    let encounters = social.map_or(&[][..], |s| s.events.as_slice());
    knowledge.prepare_validated(&observations, &ids, encounters, tick)
}
