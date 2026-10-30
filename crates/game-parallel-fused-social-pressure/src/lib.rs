//! Stable local social attempts; private fear influences responses, not target selection.
use game_parallel::{Encounter, SocialParticipant, SocialPressurePolicy, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;

#[derive(Clone, Copy)]
struct Candidate {
    person: u64,
    region: u64,
    willing: bool,
    policy: Option<usize>,
}

pub fn prepare(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    busy: &[u64],
    tick: u64,
) -> Vec<Encounter> {
    let (Some(social), Some(opinions)) = (&world.social, &world.opinions) else {
        return Vec::new();
    };
    if !tick.is_multiple_of(social.rules.interval) {
        return Vec::new();
    }
    let (minimum_fear, minimum_obligation) = rayon::join(
        || {
            social
                .pressures
                .par_iter()
                .filter(|policy| policy.enabled)
                .filter_map(|policy| policy.fear_at_least)
                .min()
        },
        || {
            social
                .pressures
                .par_iter()
                .filter(|policy| policy.enabled)
                .filter_map(|policy| policy.obligation_at_least)
                .min()
        },
    );
    if minimum_fear.is_none() && minimum_obligation.is_none() {
        return Vec::new();
    }
    let mut feelings: Vec<_> = opinions
        .records
        .par_iter()
        .filter(|row| {
            minimum_fear.is_some_and(|min| row.fear >= min)
                || minimum_obligation.is_some_and(|min| row.obligation >= min)
        })
        .collect();
    if feelings.is_empty() {
        return Vec::new();
    }
    if feelings
        .par_windows(2)
        .any(|pair| (pair[0].observer, pair[0].target) > (pair[1].observer, pair[1].target))
    {
        feelings.par_sort_unstable_by_key(|row| (row.observer, row.target));
    }
    let participants = sorted_participants(&social.participants);
    let policies = sorted_policies(&social.pressures);
    let needs = social
        .needs
        .as_ref()
        .map(|needs| needs.index())
        .unwrap_or_default();
    let personality = world.personality.as_ref().map(|owner| owner.index());
    positions
        .par_chunk_by(|a, b| a.0 / 4096 == b.0 / 4096)
        .flat_map_iter(|group| {
            let mut events = Vec::new();
            let mut eligible = Vec::new();
            let first = group[0].0;
            let shard = first / 4096;
            let mut participant = participants.partition_point(|row| row.person < first);
            let mut need = needs.partition_point(|row| row.0 < first);
            let mut occupied = busy.partition_point(|&person| person < first);
            let mut policy = policies.partition_point(|row| row.person < first);
            for &(id, block, slot) in group {
                let block = &blocks[block];
                while occupied < busy.len() && busy[occupied] < id {
                    occupied += 1;
                }
                if block.journey.is_some()
                    || block.health[slot] < social.rules.min_health
                    || block.health[slot] == 0
                    || block.hunger[slot] > social.rules.max_hunger
                    || block.work[slot] != NONE
                    || busy.get(occupied) == Some(&id)
                {
                    continue;
                }
                while participant < participants.len() && participants[participant].person < id {
                    participant += 1;
                }
                let Some(person) = participants.get(participant).filter(|row| row.person == id)
                else {
                    continue;
                };
                while need < needs.len() && needs[need].0 < id {
                    need += 1;
                }
                let isolation = needs
                    .get(need)
                    .filter(|row| row.0 == id)
                    .map_or(0, |row| row.1);
                let willing = person.willing
                    && personality
                        .as_ref()
                        .is_none_or(|traits| traits.social(id, isolation, world.seed, tick));
                while policy < policies.len() && policies[policy].person < id {
                    policy += 1;
                }
                let policy = policies
                    .get(policy)
                    .filter(|row| row.person == id)
                    .map(|_| policy);
                eligible.push(Candidate {
                    person: id,
                    region: world.regions[block.region].id,
                    willing,
                    policy,
                });
            }
            eligible.sort_unstable_by_key(|row| (row.region, row.person));
            for local in eligible.chunk_by(|a, b| a.region == b.region) {
                if local.len() < 2 {
                    continue;
                }
                let region = local[0].region;
                let mut responses = vec![None; local.len()];
                let offset = (game_parallel_genetics::random(world.seed ^ region, shard, tick)
                    % local.len() as u64) as usize;
                for start in (0..local.len()).step_by(16) {
                    let len = 16.min(local.len() - start);
                    let mut order = [0usize; 16];
                    for (index, slot) in order.iter_mut().enumerate().take(len) {
                        *slot = (offset + start + index) % local.len();
                    }
                    for i in (1..len).rev() {
                        let j = (game_parallel_genetics::random(
                            world.seed ^ shard ^ region,
                            tick,
                            (start + i) as u64,
                        ) % (i + 1) as u64) as usize;
                        order.swap(i, j);
                    }
                    for pair in order[..len].as_chunks::<2>().0 {
                        let (a, b) = (local[pair[0]], local[pair[1]]);
                        if a.willing != b.willing {
                            let (requester, target) = if a.willing {
                                (a.person, pair[1])
                            } else {
                                (b.person, pair[0])
                            };
                            responses[target] = Some(requester);
                        }
                    }
                }
                for (target, requester) in local.iter().zip(responses) {
                    let Some(requester) = requester else {
                        continue;
                    };
                    let Some(policy) = target.policy else {
                        continue;
                    };
                    let Ok(index) = feelings
                        .binary_search_by_key(&(target.person, requester), |row| {
                            (row.observer, row.target)
                        })
                    else {
                        continue;
                    };
                    let feeling = feelings[index];
                    if policies[policy].responds(feeling.fear, feeling.obligation) {
                        events.push(Encounter {
                            tick,
                            region,
                            a: requester.min(target.person),
                            b: requester.max(target.person),
                            pressured: Some(target.person),
                        });
                    }
                }
            }
            events
        })
        .collect()
}

fn sorted_participants(rows: &[SocialParticipant]) -> std::borrow::Cow<'_, [SocialParticipant]> {
    let mut result = std::borrow::Cow::Borrowed(rows);
    if rows
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        result.to_mut().par_sort_unstable_by_key(|row| row.person);
    }
    result
}
fn sorted_policies(rows: &[SocialPressurePolicy]) -> std::borrow::Cow<'_, [SocialPressurePolicy]> {
    let mut result = std::borrow::Cow::Borrowed(rows);
    if rows
        .par_windows(2)
        .any(|pair| pair[0].person > pair[1].person)
    {
        result.to_mut().par_sort_unstable_by_key(|row| row.person);
    }
    result
}
