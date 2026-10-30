//! Build social contacts from physical owners at the tick boundary.
use crate::{Contact, Social};
use game_parallel_people::Region;
use game_parallel_transport::Traveler;
use rayon::prelude::*;
pub fn prepare(
    social: Option<&Social>,
    regions: &[Region],
    travelers: &[Traveler],
    seed: u64,
    next: u64,
    busy: &[u64],
) -> Result<Option<Social>, &'static str> {
    let Some(social) = social else {
        return Ok(None);
    };
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let contacts: Vec<_> = regions
        .par_iter()
        .flat_map(|region| {
            region.people.par_iter().map(|p| Contact {
                busy: busy.binary_search(&p.id).is_ok(),
                person: p.id,
                region: Some(region.id),
                health: p.health,
                hunger: p.hunger,
            })
        })
        .chain(travelers.par_iter().map(|t| Contact {
            busy: busy.binary_search(&t.person.id).is_ok(),
            person: t.person.id,
            region: None,
            health: t.person.health,
            hunger: t.person.hunger,
        }))
        .collect();
    social.prepare(&contacts, seed, next).map(Some)
}
/// Prepare social encounters from a validated shared person snapshot.
pub fn prepare_indexed(
    social: &Social,
    people: &[(u64, Option<u64>, u32, u32)],
    busy: &[u64],
    seed: u64,
    next: u64,
) -> Result<Social, &'static str> {
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let contacts: Vec<_> = people
        .par_iter()
        .map(|&(person, region, health, hunger)| Contact {
            person,
            region,
            health,
            hunger,
            busy: busy.binary_search(&person).is_ok(),
        })
        .collect();
    social.prepare_validated(&contacts, seed, next)
}
