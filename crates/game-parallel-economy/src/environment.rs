//! Regional exposure against actual people and currently available shelter.
use game_parallel_economy_person::{Person, travel::Travel};
use game_parallel_environment::Environment;
use game_parallel_lots::{Place, View};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub fn validate(
    environment: Option<&Environment>,
    view: &View,
    goods: &HashSet<u32>,
) -> Result<(), String> {
    let Some(environment) = environment else {
        return Ok(());
    };
    let regions: HashSet<_> = view
        .containers
        .par_iter()
        .filter_map(|row| match row.place {
            Place::Stock { region, .. } => Some(region),
            _ => None,
        })
        .collect();
    environment.validate(&regions)?;
    if environment.regions.iter().any(|h| {
        h.shelter_good.is_some_and(|good| {
            !goods.contains(&good)
                || view
                    .container(Place::Stock {
                        region: h.region,
                        good,
                    })
                    .is_none()
        })
    }) {
        return Err("invalid shelter good".into());
    }
    Ok(())
}

pub fn apply(
    environment: Option<&Environment>,
    view: &View,
    travel: Option<&Travel>,
    people: &mut [Person],
    tick: u64,
) {
    let Some(environment) = environment else {
        return;
    };
    let journeys: HashMap<_, _> = travel
        .map_or(&[][..], |t| t.journeys.as_slice())
        .par_iter()
        .map(|j| (j.person, j.source))
        .collect();
    let mut residents: Vec<_> = people
        .par_iter()
        .filter_map(|p| (p.health > 0).then_some((p.region?, p.id)))
        .collect();
    if residents.par_windows(2).any(|p| p[0] > p[1]) {
        residents.par_sort_unstable();
    }
    let hazards: HashMap<_, _> = environment
        .regions
        .iter()
        .map(|h| {
            let first = residents.partition_point(|row| row.0 < h.region);
            let last = residents.partition_point(|row| row.0 <= h.region);
            let extra = h
                .shelter_good
                .and_then(|good| {
                    view.container(Place::Stock {
                        region: h.region,
                        good,
                    })
                    .map(|stock| stock.quantity(good))
                })
                .unwrap_or(0);
            let capacity = u64::from(h.shelter)
                .saturating_add(extra)
                .min((last - first) as u64) as usize;
            (h.region, (h, first, last, capacity))
        })
        .collect();
    let season = environment.season(tick);
    people.par_iter_mut().for_each(|person| {
        if person.health == 0 {
            return;
        }
        let region = person.region.or_else(|| journeys.get(&person.id).copied());
        let Some((hazard, first, last, capacity)) = region.and_then(|r| hazards.get(&r)).copied()
        else {
            return;
        };
        let count = last - first;
        let sheltered = person.region.is_some()
            && count > 0
            && residents
                .binary_search(&(hazard.region, person.id))
                .ok()
                .is_some_and(|index| {
                    let offset = (tick.saturating_sub(1) % count as u64) as usize;
                    (index - first + count - offset) % count < capacity
                });
        let damage = hazard.damage.saturating_sub(if sheltered {
            hazard.shelter_reduction
        } else {
            0
        });
        let damage = season.map_or(damage, |s| s.exposure(damage));
        person.health = person.health.saturating_sub(damage);
    });
}
