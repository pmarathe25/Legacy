//! Seed owner state from the generated world's authored layout.
mod settings;
use game_parallel::{
    Agriculture, CropPhase, Farm, Field, Knowledge, KnowledgeRecord, Mind, Region, Social,
    SocialParticipant,
};
use rayon::prelude::*;
pub use settings::Settings;

/// Seed founding reports from the initial ring, not live remote observations.
pub fn knowledge(owner: &mut Knowledge, people: &[u64], source: (u64, u32, u32, u32, Option<u64>)) {
    let (seed, regions, food, percent, controlled) = source;
    let region_count = u64::from(regions.max(1));
    owner.minds = people
        .par_iter()
        .map(|&person| {
            let region = (person.saturating_sub(1) % region_count + 1) % region_count + 1;
            let witness = if Some(region) == controlled {
                region + region_count
            } else {
                region
            };
            let knows_report = Some(person) != controlled
                && witness <= people.len() as u64
                && game_parallel_genetics::random(seed ^ 0x7265706f7274, person, 0) % 100
                    < u64::from(percent);
            Mind {
                person,
                records: knows_report
                    .then_some(KnowledgeRecord {
                        region,
                        food,
                        observed_at: 0,
                        witness,
                        learned_from: Some(witness),
                    })
                    .into_iter()
                    .collect(),
            }
        })
        .collect();
}
/// Populate deterministic field identities for the generated, one-based region layout.
pub fn farms(owner: &mut Agriculture, regions: &[Region], fields_per_region: u32) {
    owner.farms = regions
        .par_iter()
        .map(|region| Farm {
            region: region.id,
            fields: (0..u64::from(fields_per_region))
                .map(|i| Field {
                    id: (region.id - 1) * u64::from(fields_per_region) + i + 1,
                    phase: CropPhase::Fallow,
                    work: 0,
                })
                .collect(),
        })
        .collect();
}

/// Initialize participation while clearing template-specific relationships.
pub fn social(owner: &mut Social, people: &[u64]) {
    owner.pressures.clear();
    owner.participants = people
        .par_iter()
        .map(|&person| SocialParticipant {
            person,
            willing: true,
        })
        .collect();
    owner.relationships.clear();
    owner.events.clear();
    if let Some(needs) = &mut owner.needs {
        needs.people.clear();
    }
}

/// Seed individual interpretations; nothing here prevents a physical action.
pub fn claim_tolerance(
    owner: &mut game_parallel::Opinions,
    people: &[u64],
    settings: &Settings,
    controlled: Option<u64>,
) {
    owner.sensitivities = people
        .par_iter()
        .filter_map(|&person| {
            let tolerant =
                game_parallel_genetics::random(settings.seed ^ 0x746f6c6572616e63, person, 0) % 100
                    < u64::from(settings.claim_tolerance_percent);
            let confront = Some(person) != controlled
                && game_parallel_genetics::random(settings.seed ^ 0x636f6e66726f6e74, person, 0)
                    % 100
                    < u64::from(settings.confrontation_percent);
            (tolerant || confront).then_some(game_parallel::OpinionSensitivity {
                person,
                fear_per_damage: None,
                gratitude_gain: None,
                obligation_relief: None,
                theft_loss: tolerant.then_some(0),
                confrontation: confront.then_some(game_parallel::ConfrontationPreference {
                    trust_at_most: settings.confrontation_threshold,
                    fear_at_most: settings.confrontation_fear_limit,
                    interval: settings.confrontation_interval,
                    enabled: true,
                }),
                affinity_help: None,
                affinity_harm_per_damage: None,
                affinity_encounter: None,
                affinity_pressured_encounter: None,
                relationship_cases: vec![],
                obligation_development: None,
                promise_obligation_gain: None,
            })
        })
        .collect();
}

/// Seed independent dispositions, leaving every response to the individual.
pub fn work_requests(
    people: &[u64],
    settings: &Settings,
    controlled: Option<u64>,
) -> Option<game_parallel::work_requests::Requests> {
    use game_parallel::work_requests::{Asker, Preference, Requests};
    if settings.work_requesting_percent == 0
        && settings.work_helpful_percent == 0
        && settings.work_disregard_percent == 0
    {
        return None;
    }
    let askers = people
        .par_iter()
        .filter(|&&person| {
            Some(person) != controlled
                && game_parallel_genetics::random(settings.seed ^ 0x776f726b61736b, person, 0) % 100
                    < u64::from(settings.work_requesting_percent)
        })
        .map(|&person| Asker {
            person,
            interval: settings.work_request_interval,
            lifetime: settings.work_request_lifetime,
        })
        .collect();
    let preferences = people
        .par_iter()
        .filter(|&&person| Some(person) != controlled)
        .map(|&person| Preference {
            person,
            effort: settings.work_effort,
            disregard_claims: game_parallel_genetics::random(
                settings.seed ^ 0x636c61696d,
                person,
                0,
            ) % 100
                < u64::from(settings.work_disregard_percent),
            help_value: if game_parallel_genetics::random(
                settings.seed ^ 0x776f726b68656c70,
                person,
                0,
            ) % 100
                < u64::from(settings.work_helpful_percent)
            {
                settings.work_effort
            } else {
                0
            },
        })
        .collect();
    Some(Requests {
        next_id: 1,
        memory_limit: settings.work_memory_limit,
        work_links: Vec::new(),
        memories: Vec::new(),
        preferences,
        askers,
        pending: Vec::new(),
        events: Vec::new(),
    })
}

/// Generate independent personal asking and helping dispositions, with no group preset.
pub fn market(
    owner: &mut game_parallel::Market,
    people: &[u64],
    traders: &[(u64, u32)],
    config: (u64, u32, u32, Option<u64>),
) {
    let (seed, requesting, helpful, controlled) = config;
    let mut trader_ids: Vec<_> = traders.par_iter().map(|row| row.0).collect();
    trader_ids.par_sort_unstable();
    let helpfulness = |person| {
        if game_parallel_genetics::random(seed ^ 0x68656c70, person, 0) % 100 < u64::from(helpful) {
            2
        } else {
            0
        }
    };
    let mut policies: Vec<_> = traders
        .par_iter()
        .map(|&(person, good)| game_parallel::TradePolicy {
            person,
            enabled: true,
            propose: true,
            request_help: false,
            min_gain: 1,
            help_value: helpfulness(person),
            influences: vec![],
            values: vec![
                game_parallel::TradeValue {
                    good: 1,
                    value: if good == 1 { 1 } else { 2 },
                },
                game_parallel::TradeValue {
                    good: 2,
                    value: if good == 2 { 1 } else { 2 },
                },
            ],
        })
        .collect();
    policies.par_extend(
        people
            .par_iter()
            .filter(|&&person| {
                Some(person) != controlled
                    && trader_ids.binary_search(&person).is_err()
                    && game_parallel_genetics::random(seed ^ 0x61736b, person, 0) % 100
                        < u64::from(requesting)
            })
            .map(|&person| game_parallel::TradePolicy {
                person,
                enabled: true,
                propose: false,
                request_help: true,
                min_gain: 1,
                help_value: helpfulness(person),
                influences: vec![],
                values: vec![
                    game_parallel::TradeValue { good: 1, value: 2 },
                    game_parallel::TradeValue { good: 2, value: 2 },
                ],
            }),
    );
    policies.par_sort_unstable_by_key(|policy| policy.person);
    owner.policies = policies;
    owner.offers.clear();
    owner.events.clear();
    owner.next_id = 1;
}
