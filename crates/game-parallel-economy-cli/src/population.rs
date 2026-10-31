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
    #[serde(default)]
    sensitivity: Option<Sensitivity>,
    #[serde(default)]
    family: Option<Founder>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    pub population: Vec<Profile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Founder {
    min_age: u64,
    max_age: u64,
    min_allele: u16,
    max_allele: u16,
    reproduction_percent: u8,
    #[serde(default)]
    reconsider_partnership_percent: u8,
}

/// Generate individual life histories without founding any partnerships.
pub fn founders(
    seed: u64,
    profiles: &[Profile],
) -> Result<Vec<game_parallel_families::Member>, String> {
    for profile in profiles {
        let rule = profile
            .family
            .as_ref()
            .ok_or("generated population needs a family profile")?;
        if rule.min_age > rule.max_age
            || rule.max_age - rule.min_age == u64::MAX
            || rule.min_allele > rule.max_allele
            || rule.max_allele > 1000
            || rule.reproduction_percent > 100
            || rule.reconsider_partnership_percent > 100
        {
            return Err("invalid generated founder profile".into());
        }
    }
    Ok(profiles
        .par_iter()
        .flat_map(|profile| {
            let rule = profile.family.as_ref().expect("validated founder profile");
            (0..profile.count).into_par_iter().map(move |offset| {
                let id = profile.start_id + offset;
                let random = |attribute| {
                    game_parallel_genetics::random(seed ^ 0x66616d696c79, id, attribute)
                };
                let mut alleles = [[0; 2]; 4];
                for (locus, pair) in alleles.iter_mut().enumerate() {
                    for (side, allele) in pair.iter_mut().enumerate() {
                        *allele = rule.min_allele
                            + (random(10 + (locus * 2 + side) as u64)
                                % u64::from(rule.max_allele - rule.min_allele + 1))
                                as u16;
                    }
                }
                game_parallel_families::Member {
                    id,
                    born: None,
                    age: rule.min_age + random(0) % (rule.max_age - rule.min_age + 1),
                    lifespan_loss: 0,
                    genome: game_parallel_genetics::Genome {
                        sex: if random(1) % 2 == 0 {
                            game_parallel_genetics::Sex::Female
                        } else {
                            game_parallel_genetics::Sex::Male
                        },
                        alleles,
                    },
                    parents: Default::default(),
                    partner: None,
                    pregnancy: None,
                    next_conception: 0,
                    alive: profile.health > 0,
                    reproduction_allowed: random(2) % 100 < u64::from(rule.reproduction_percent),
                    reconsider_partnership: random(3) % 100
                        < u64::from(rule.reconsider_partnership_percent),
                }
            })
        })
        .collect())
}

pub fn has_founders(profiles: &[Profile]) -> bool {
    profiles.iter().any(|profile| profile.family.is_some())
}

/// Authored individual dispositions, without copied relationships or obligations.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Sensitivity {
    fear_per_damage: Option<u64>,
    gratitude_gain: Option<u64>,
    promise_obligation_gain: Option<u64>,
    obligation_relief: Option<u64>,
    theft_loss: Option<u32>,
    confrontation: Option<game_parallel_opinions::Confrontation>,
    affinity_help: Option<i32>,
    affinity_harm_per_damage: Option<i32>,
    affinity_encounter: Option<i32>,
    affinity_pressured_encounter: Option<i32>,
    affinity_site_use: Option<i32>,
    affinity_site_destruction: Option<i32>,
    obligation_development: Option<game_parallel_opinions::ObligationDevelopment>,
}
pub fn sensitivities(profiles: &[Profile]) -> Vec<game_parallel_opinions::Sensitivity> {
    profiles
        .par_iter()
        .filter_map(|profile| profile.sensitivity.as_ref().map(|rule| (profile, rule)))
        .flat_map(|(profile, rule)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                game_parallel_opinions::Sensitivity {
                    person: profile.start_id + offset,
                    fear_per_damage: rule.fear_per_damage,
                    gratitude_gain: rule.gratitude_gain,
                    promise_obligation_gain: rule.promise_obligation_gain,
                    obligation_relief: rule.obligation_relief,
                    theft_loss: rule.theft_loss,
                    confrontation: rule.confrontation.clone(),
                    affinity_help: rule.affinity_help,
                    affinity_harm_per_damage: rule.affinity_harm_per_damage,
                    affinity_encounter: rule.affinity_encounter,
                    affinity_pressured_encounter: rule.affinity_pressured_encounter,
                    affinity_site_use: rule.affinity_site_use,
                    affinity_site_destruction: rule.affinity_site_destruction,
                    relationship_cases: Vec::new(),
                    obligation_development: rule.obligation_development.clone(),
                }
            })
        })
        .collect()
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
            sensitivity: None,
            family: None,
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

    #[test]
    fn founder_parameters_are_checked_and_reproductive_choices_are_individual() {
        let mut people = profile();
        people.family = Some(Founder {
            min_age: 18,
            max_age: 25,
            min_allele: 400,
            max_allele: 1000,
            reproduction_percent: 0,
            reconsider_partnership_percent: 100,
        });
        let first = founders(7, &[people]).unwrap();
        assert_eq!(first.len(), 100);
        assert!(
            first
                .iter()
                .all(|person| !person.reproduction_allowed && person.reconsider_partnership)
        );
        assert!(
            first
                .iter()
                .all(|person| person.partner.is_none() && person.parents.is_empty())
        );
        let mut bad = profile();
        bad.family = Some(Founder {
            min_age: 26,
            max_age: 25,
            min_allele: 0,
            max_allele: 1000,
            reproduction_percent: 50,
            reconsider_partnership_percent: 0,
        });
        assert!(founders(7, &[bad]).is_err());
    }
}
