//! Seeded individuals derived from authored population profiles.
use super::Resident;
use rayon::prelude::*;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    start_id: u64,
    count: u64,
    regions: Vec<u64>,
    health: u32,
    hunger: u32,
    work_percent: u8,
    meal_percent: u8,
    #[serde(default)]
    recipe: Option<String>,
    #[serde(default)]
    goods: BTreeMap<String, u64>,
}

pub fn generate(
    seed: u64,
    regions: &HashSet<u64>,
    authored: &[Resident],
    profiles: &[Profile],
) -> Result<Vec<Resident>, String> {
    let mut ranges = Vec::with_capacity(profiles.len());
    for profile in profiles {
        if profile.start_id == 0
            || profile.count == 0
            || profile.regions.is_empty()
            || profile
                .regions
                .iter()
                .any(|region| !regions.contains(region))
            || profile.work_percent > 100
            || profile.meal_percent > 100
            || usize::try_from(profile.count).is_err()
        {
            return Err("invalid generated population profile".into());
        }
        let end = profile
            .start_id
            .checked_add(profile.count - 1)
            .ok_or("generated population identity overflow")?;
        ranges.push((profile.start_id, end));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 >= pair[1].0)
        || authored.iter().any(|person| {
            let i = ranges.partition_point(|range| range.0 <= person.id);
            i > 0 && person.id <= ranges[i - 1].1
        })
    {
        return Err("generated population identities overlap".into());
    }
    Ok(profiles
        .par_iter()
        .flat_map(|profile| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let id = profile.start_id + offset;
                let sample = |attribute| game_parallel_genetics::random(seed, id, attribute);
                Resident {
                    id,
                    region: profile.regions[(sample(0) % profile.regions.len() as u64) as usize],
                    health: profile.health,
                    hunger: profile.hunger,
                    willing_to_work: sample(1) % 100 < u64::from(profile.work_percent),
                    seek_meals: sample(2) % 100 < u64::from(profile.meal_percent),
                    controlled: false,
                    recipe: profile.recipe.clone(),
                    goods: profile.goods.clone(),
                }
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> Profile {
        Profile {
            start_id: 10,
            count: 100,
            regions: vec![1, 2],
            health: 100,
            hunger: 0,
            work_percent: 50,
            meal_percent: 50,
            recipe: None,
            goods: BTreeMap::new(),
        }
    }

    #[test]
    fn profiles_reject_overlapping_identities_and_unknown_regions() {
        let regions = HashSet::from([1, 2]);
        let mut other = profile();
        other.start_id = 109;
        assert!(generate(1, &regions, &[], &[profile(), other]).is_err());
        let mut unknown = profile();
        unknown.regions.push(3);
        assert!(generate(1, &regions, &[], &[unknown]).is_err());
        let mut overflow = profile();
        overflow.start_id = u64::MAX;
        assert!(generate(1, &regions, &[], &[overflow]).is_err());
        let authored = Resident {
            id: 10,
            region: 1,
            health: 100,
            hunger: 0,
            willing_to_work: false,
            seek_meals: false,
            controlled: true,
            recipe: None,
            goods: BTreeMap::new(),
        };
        assert!(generate(1, &regions, &[authored], &[profile()]).is_err());
    }

    #[test]
    fn different_seeds_change_individual_choices_without_changing_identities() {
        let regions = HashSet::from([1, 2]);
        let people = |seed| {
            generate(seed, &regions, &[], &[profile()])
                .unwrap()
                .into_iter()
                .map(|p| (p.id, p.region, p.willing_to_work, p.seek_meals))
                .collect::<Vec<_>>()
        };
        let first = people(1);
        let second = people(2);
        assert_eq!(
            first.iter().map(|p| p.0).collect::<Vec<_>>(),
            second.iter().map(|p| p.0).collect::<Vec<_>>()
        );
        assert_ne!(first, second);
    }
}
