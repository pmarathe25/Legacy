//! Physical use of claimed inputs, with observations limited to present claimants.
use game_parallel::{PropertyTheft, StartWork, World};
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::HashMap;

pub fn prepare(
    world: &World,
    people: &[PersonSnapshot],
    jobs: &[StartWork],
    tick: u64,
) -> Vec<PropertyTheft> {
    let Some(property) = &world.property else {
        return Vec::new();
    };
    if property.claims.is_empty() || jobs.is_empty() {
        return Vec::new();
    }
    let production = world.production.as_ref().expect("planned production");
    let recipes: HashMap<_, _> = production.recipes.iter().map(|row| (row.id, row)).collect();
    let claims: HashMap<_, _> = property
        .claims
        .iter()
        .map(|row| ((row.region, row.good), row.owner))
        .collect();
    let rights = property.index();
    jobs.par_iter()
        .flat_map_iter(|job| {
            let rights = &rights;
            let claims = &claims;
            recipes[&job.recipe].inputs.iter().filter_map(move |input| {
                if rights.allows(job.worker, job.region, input.good) {
                    return None;
                }
                let owner = claims[&(job.region, input.good)];
                let witnessed = person_index(people, owner)
                    .is_some_and(|i| people[i].1 == Some(job.region) && people[i].2 > 0);
                Some(PropertyTheft {
                    tick,
                    actor: job.worker,
                    owner,
                    region: job.region,
                    good: input.good,
                    quantity: input
                        .quantity
                        .checked_mul(u64::from(job.units))
                        .expect("planned quantity"),
                    witnessed,
                })
            })
        })
        .collect()
}

pub fn publish(world: &mut World, incidents: Vec<PropertyTheft>) {
    if incidents.is_empty() {
        return;
    }
    if let Some(opinions) = &mut world.opinions {
        let witnessed: Vec<_> = incidents
            .par_iter()
            .filter(|row| row.witnessed)
            .map(|row| (row.owner, row.actor))
            .collect();
        opinions.witness_thefts(&witnessed, world.tick);
    }
    world
        .property
        .as_mut()
        .expect("prepared property")
        .thefts
        .par_extend(incidents);
}
