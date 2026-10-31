//! Seeded individuals derived from authored population profiles.
use super::{Aspiration, FoodResponseConfig, Resident};
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
    #[serde(default)]
    farm_percent: Option<u8>,
    #[serde(default)]
    food_search_percent: Option<u8>,
    #[serde(default)]
    social: Option<SocialProfile>,
    #[serde(default)]
    claims: Option<ClaimProfile>,
    #[serde(default)]
    help: Option<HelpProfile>,
    #[serde(default)]
    work_support: Option<WorkSupport>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    pub population: Vec<Profile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SocialProfile {
    willing_percent: u8,
    min_sociability: u16,
    max_sociability: u16,
    min_diligence: u16,
    max_diligence: u16,
    #[serde(default)]
    pressure_percent: u8,
    #[serde(default)]
    fear_at_least: Option<u64>,
    #[serde(default)]
    obligation_at_least: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimProfile {
    assert_outputs_percent: u8,
    disregard_percent: u8,
    #[serde(default)]
    assert_built_sites_percent: u8,
    #[serde(default)]
    desperate_hunger: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkSupport {
    aspiration_percent: u8,
    #[serde(default)]
    learning_percent: u8,
    #[serde(default)]
    learning_max: usize,
    priority: i32,
    batch: u64,
    review_interval: u64,
    ask_percent: u8,
    ask_interval: u64,
    request_lifetime: u64,
    effort: u64,
    min_help_value: u64,
    max_help_value: u64,
    #[serde(default)]
    affinity_weight: i64,
    #[serde(default)]
    disregard_percent: u8,
}

pub struct WorkRows {
    pub aspirations: Vec<Aspiration>,
    pub learning: Vec<game_parallel_economy::Learning>,
    pub preferences: Vec<game_parallel_autowork::requests::Preference>,
    pub askers: Vec<game_parallel_autowork::requests::Asker>,
}

pub fn work_support(seed: u64, profiles: &[Profile]) -> Result<WorkRows, String> {
    for profile in profiles {
        if let Some(rule) = &profile.work_support
            && (rule.aspiration_percent > 100
                || rule.learning_percent > 100
                || (rule.learning_percent > 0 && !(1..=32).contains(&rule.learning_max))
                || rule.ask_percent > 100
                || rule.disregard_percent > 100
                || rule.batch == 0
                || rule.review_interval == 0
                || rule.ask_interval == 0
                || rule.request_lifetime == 0
                || rule.min_help_value > rule.max_help_value
                || rule.max_help_value - rule.min_help_value == u64::MAX
                || (rule.aspiration_percent > 0 && profile.recipe.is_none()))
        {
            return Err("invalid generated work-support preference".into());
        }
    }
    let rows: Vec<_> = profiles
        .par_iter()
        .filter_map(|profile| profile.work_support.as_ref().map(|rule| (profile, rule)))
        .flat_map(|(profile, rule)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let person = profile.start_id + offset;
                let random = |attribute| {
                    game_parallel_genetics::random(seed ^ 0x776f726b737570, person, attribute)
                };
                let aspiration = profile
                    .recipe
                    .as_ref()
                    .filter(|_| random(0) % 100 < u64::from(rule.aspiration_percent))
                    .map(|recipe| Aspiration {
                        person,
                        recipe: recipe.clone(),
                        priority: rule.priority,
                        batch: rule.batch,
                        review_interval: rule.review_interval,
                    });
                let preference = game_parallel_autowork::requests::Preference {
                    person,
                    effort: rule.effort,
                    help_value: rule.min_help_value
                        + random(1) % (rule.max_help_value - rule.min_help_value + 1),
                    affinity_weight: rule.affinity_weight,
                    disregard_claims: random(2) % 100 < u64::from(rule.disregard_percent),
                };
                let asker = (random(3) % 100 < u64::from(rule.ask_percent)).then_some(
                    game_parallel_autowork::requests::Asker {
                        person,
                        interval: rule.ask_interval,
                        lifetime: rule.request_lifetime,
                    },
                );
                let learning = (random(4) % 100 < u64::from(rule.learning_percent)).then_some(
                    game_parallel_economy::Learning {
                        person,
                        max_aspirations: rule.learning_max,
                        priority: rule.priority,
                        batch: rule.batch,
                        review_interval: rule.review_interval,
                    },
                );
                (aspiration, preference, asker, learning)
            })
        })
        .collect();
    Ok(WorkRows {
        aspirations: rows.par_iter().filter_map(|row| row.0.clone()).collect(),
        preferences: rows.par_iter().map(|row| row.1.clone()).collect(),
        askers: rows.par_iter().filter_map(|row| row.2.clone()).collect(),
        learning: rows.into_par_iter().filter_map(|row| row.3).collect(),
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HelpProfile {
    good: String,
    quantity: u64,
    recipient_hunger_at_least: u32,
    willing_percent: u8,
    #[serde(default)]
    minimum_trust: Option<i32>,
    #[serde(default)]
    affinity_threshold: Option<i32>,
    #[serde(default)]
    pressure_threshold: Option<u64>,
    #[serde(default)]
    learning: Option<HelpLearning>,
    #[serde(default)]
    acquaintance_asking: Option<AcquaintanceProfile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AcquaintanceProfile {
    percent: u8,
    min_hunger: u32,
    retry_ticks: u64,
    request_lifetime: u64,
    #[serde(default)]
    min_familiarity: u32,
    #[serde(default)]
    min_affinity: Option<i32>,
    #[serde(default)]
    prefer_affinity: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HelpLearning {
    percent: u8,
    threshold: u32,
    capacity: u32,
    #[serde(default)]
    reciprocate_hunger: Option<u32>,
    #[serde(default)]
    ask_helper: Option<game_parallel_economy::HelperAsking>,
}

pub struct HelpRows {
    pub responses: Vec<FoodResponseConfig>,
    pub learning: Vec<game_parallel_economy::SharingLearning>,
    pub acquaintance_asking: Vec<game_parallel_economy::AcquaintanceAsking>,
}

pub fn help(seed: u64, profiles: &[Profile]) -> Result<HelpRows, String> {
    for rule in profiles.iter().filter_map(|profile| profile.help.as_ref()) {
        if rule.good.is_empty()
            || rule.quantity == 0
            || rule.willing_percent > 100
            || rule.affinity_threshold.is_some_and(|n| n <= 0)
            || rule.pressure_threshold == Some(0)
            || rule.learning.as_ref().is_some_and(|learning| {
                learning.percent > 100
                    || learning.threshold == 0
                    || learning.capacity < learning.threshold
                    || learning
                        .ask_helper
                        .as_ref()
                        .is_some_and(|a| a.retry_ticks == 0 || a.request_lifetime == 0)
            })
            || rule.acquaintance_asking.as_ref().is_some_and(|asking| {
                asking.percent > 100 || asking.retry_ticks == 0 || asking.request_lifetime == 0
            })
        {
            return Err("invalid generated help preference".into());
        }
    }
    let rows: Vec<_> = profiles
        .par_iter()
        .filter_map(|profile| profile.help.as_ref().map(|rule| (profile, rule)))
        .flat_map(|(profile, rule)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let person = profile.start_id + offset;
                let random = |attribute| {
                    game_parallel_genetics::random(seed ^ 0x68656c70, person, attribute)
                };
                let response = FoodResponseConfig {
                    person,
                    good: rule.good.clone(),
                    quantity: rule.quantity,
                    recipient_hunger_at_least: rule.recipient_hunger_at_least,
                    willing: random(0) % 100 < u64::from(rule.willing_percent),
                    minimum_trust: rule.minimum_trust,
                    affinity_threshold: rule.affinity_threshold,
                    pressure_threshold: rule.pressure_threshold,
                };
                let learning = rule
                    .learning
                    .as_ref()
                    .filter(|learning| random(1) % 100 < u64::from(learning.percent))
                    .map(|learning| game_parallel_economy::SharingLearning {
                        person,
                        enabled: true,
                        threshold: learning.threshold,
                        capacity: learning.capacity,
                        experiences: 0,
                        last_helper: None,
                        reciprocate_hunger: learning.reciprocate_hunger,
                        ask_helper: learning.ask_helper.clone(),
                    });
                let asking = rule
                    .acquaintance_asking
                    .as_ref()
                    .filter(|asking| random(2) % 100 < u64::from(asking.percent))
                    .map(|asking| game_parallel_economy::AcquaintanceAsking {
                        person,
                        enabled: true,
                        min_hunger: asking.min_hunger,
                        retry_ticks: asking.retry_ticks,
                        request_lifetime: asking.request_lifetime,
                        min_familiarity: asking.min_familiarity,
                        min_affinity: asking.min_affinity,
                        prefer_affinity: asking.prefer_affinity,
                        next_review: 0,
                        last_target: None,
                    });
                (response, learning, asking)
            })
        })
        .collect();
    Ok(HelpRows {
        responses: rows.par_iter().map(|row| row.0.clone()).collect(),
        learning: rows.par_iter().filter_map(|row| row.1.clone()).collect(),
        acquaintance_asking: rows.into_par_iter().filter_map(|row| row.2).collect(),
    })
}

pub fn claims(
    seed: u64,
    profiles: &[Profile],
) -> Result<Vec<game_parallel_economy_dynamics::claims::Preference>, String> {
    for rule in profiles
        .iter()
        .filter_map(|profile| profile.claims.as_ref())
    {
        if rule.assert_outputs_percent > 100
            || rule.disregard_percent > 100
            || rule.assert_built_sites_percent > 100
        {
            return Err("invalid generated claim preference".into());
        }
    }
    Ok(profiles
        .par_iter()
        .filter_map(|profile| profile.claims.as_ref().map(|rule| (profile, rule)))
        .flat_map(|(profile, rule)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let person = profile.start_id + offset;
                let random = |attribute| {
                    game_parallel_genetics::random(seed ^ 0x636c61696d73, person, attribute)
                };
                game_parallel_economy_dynamics::claims::Preference {
                    person,
                    disregard_others: random(0) % 100 < u64::from(rule.disregard_percent),
                    assert_outputs: random(1) % 100 < u64::from(rule.assert_outputs_percent),
                    assert_built_sites: random(2) % 100
                        < u64::from(rule.assert_built_sites_percent),
                    desperate_hunger: rule.desperate_hunger,
                }
            })
        })
        .collect())
}

pub struct SocialRows {
    pub participants: Vec<game_parallel_social::Participant>,
    pub personalities: Vec<game_parallel_economy_dynamics::personality::Person>,
    pub pressures: Vec<game_parallel_social::PressurePolicy>,
}

pub fn social(seed: u64, profiles: &[Profile]) -> Result<SocialRows, String> {
    for rule in profiles
        .iter()
        .filter_map(|profile| profile.social.as_ref())
    {
        if rule.willing_percent > 100
            || rule.pressure_percent > 100
            || rule.min_sociability > rule.max_sociability
            || rule.max_sociability > 1000
            || rule.min_diligence > rule.max_diligence
            || rule.max_diligence > 1000
            || rule.fear_at_least == Some(0)
            || rule.obligation_at_least == Some(0)
            || (rule.pressure_percent > 0
                && rule.fear_at_least.is_none()
                && rule.obligation_at_least.is_none())
        {
            return Err("invalid generated social profile".into());
        }
    }
    let rows: Vec<_> = profiles
        .par_iter()
        .filter_map(|profile| profile.social.as_ref().map(|rule| (profile, rule)))
        .flat_map(|(profile, rule)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let person = profile.start_id + offset;
                let random = |attribute| {
                    game_parallel_genetics::random(seed ^ 0x736f6369616c, person, attribute)
                };
                let willing = random(0) % 100 < u64::from(rule.willing_percent);
                let personality = game_parallel_economy_dynamics::personality::Person {
                    person,
                    sociability: rule.min_sociability
                        + (random(1) % u64::from(rule.max_sociability - rule.min_sociability + 1))
                            as u16,
                    diligence: rule.min_diligence
                        + (random(2) % u64::from(rule.max_diligence - rule.min_diligence + 1))
                            as u16,
                };
                let pressure = (random(3) % 100 < u64::from(rule.pressure_percent)).then_some(
                    game_parallel_social::PressurePolicy {
                        person,
                        fear_at_least: rule.fear_at_least,
                        obligation_at_least: rule.obligation_at_least,
                        enabled: true,
                    },
                );
                (
                    game_parallel_economy_dynamics::participant(person, willing),
                    personality,
                    pressure,
                )
            })
        })
        .collect();
    Ok(SocialRows {
        participants: rows.par_iter().map(|row| row.0.clone()).collect(),
        personalities: rows.par_iter().map(|row| row.1.clone()).collect(),
        pressures: rows.into_par_iter().filter_map(|row| row.2).collect(),
    })
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

pub fn farming(seed: u64, profiles: &[Profile]) -> Vec<game_parallel_economy::FarmPolicy> {
    profiles
        .par_iter()
        .filter_map(|profile| profile.farm_percent.map(|rate| (profile, rate)))
        .flat_map(|(profile, rate)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let person = profile.start_id + offset;
                game_parallel_economy::FarmPolicy {
                    person,
                    willing: game_parallel_genetics::random(seed, person, 3) % 100
                        < u64::from(rate),
                    pressured_by: None,
                    pressure_threshold: None,
                }
            })
        })
        .collect()
}

pub fn food_search(
    seed: u64,
    profiles: &[Profile],
) -> Vec<game_parallel_economy::FoodSearchPreference> {
    profiles
        .par_iter()
        .filter_map(|profile| profile.food_search_percent.map(|rate| (profile, rate)))
        .flat_map(|(profile, rate)| {
            (0..profile.count).into_par_iter().map(move |offset| {
                let person = profile.start_id + offset;
                game_parallel_economy::FoodSearchPreference {
                    person,
                    enabled: game_parallel_genetics::random(seed, person, 4) % 100
                        < u64::from(rate),
                    next_review: 0,
                    target: None,
                }
            })
        })
        .collect()
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
    affinity_site_salvage: Option<i32>,
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
                    affinity_site_salvage: rule.affinity_site_salvage,
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
            || profile.farm_percent.is_some_and(|rate| rate > 100)
            || profile.food_search_percent.is_some_and(|rate| rate > 100)
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
            farm_percent: None,
            food_search_percent: None,
            social: None,
            claims: None,
            help: None,
            work_support: None,
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
        let mut invalid_farming = profile();
        invalid_farming.farm_percent = Some(101);
        assert!(generate(1, &regions, &[], &[invalid_farming]).is_err());
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

    #[test]
    fn pressure_profiles_need_a_personal_fear_or_obligation_threshold() {
        let mut people = profile();
        people.social = Some(SocialProfile {
            willing_percent: 0,
            min_sociability: 0,
            max_sociability: 1000,
            min_diligence: 0,
            max_diligence: 1000,
            pressure_percent: 100,
            fear_at_least: None,
            obligation_at_least: None,
        });
        assert!(social(1, &[people]).is_err());
    }

    #[test]
    fn claim_profile_rejects_invalid_probabilities() {
        let mut people = profile();
        people.claims = Some(ClaimProfile {
            assert_outputs_percent: 101,
            disregard_percent: 0,
            assert_built_sites_percent: 0,
            desperate_hunger: None,
        });
        assert!(claims(1, &[people]).is_err());
    }

    #[test]
    fn work_support_rejects_aspirations_without_a_recipe() {
        let mut people = profile();
        people.work_support = Some(WorkSupport {
            aspiration_percent: 100,
            learning_percent: 0,
            learning_max: 0,
            priority: 1,
            batch: 1,
            review_interval: 1,
            ask_percent: 50,
            ask_interval: 1,
            request_lifetime: 2,
            effort: 5,
            min_help_value: 0,
            max_help_value: 10,
            affinity_weight: 0,
            disregard_percent: 0,
        });
        assert!(work_support(1, &[people]).is_err());
    }
}
