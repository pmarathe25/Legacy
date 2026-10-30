//! Independent local decisions to leave an institution over a policy disagreement.
use game_parallel::World;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;

pub fn prepare(world: &World, people: &[PersonSnapshot], busy: &[u64]) -> Vec<(u64, u64)> {
    let (Some(civic), Some(institutions)) = (&world.civic_auto, &world.institutions) else {
        return Vec::new();
    };
    let mut policies: Vec<_> = civic
        .policies
        .par_iter()
        .filter(|policy| policy.enabled && policy.leave_on_trade_disagreement)
        .collect();
    if policies.is_empty() {
        return Vec::new();
    }
    policies.par_sort_unstable_by_key(|policy| policy.person);
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let needs_trust = policies
        .par_iter()
        .any(|policy| policy.trade_trust_target.is_some());
    let opinions = world
        .opinions
        .as_ref()
        .filter(|_| needs_trust)
        .map(|owner| owner.index());
    let policies = &policies;
    let busy = &busy;
    let opinions = &opinions;
    let mut exits: Vec<_> = institutions
        .bodies
        .par_iter()
        .flat_map(|body| {
            body.members.par_iter().filter_map(move |&person| {
                let policy = policies
                    .binary_search_by_key(&person, |policy| policy.person)
                    .ok()
                    .map(|i| policies[i])?;
                let (_, region, health, _) = people[*person_index(people, person).as_ref()?];
                if health == 0 || region != Some(body.region) || busy.binary_search(&person).is_ok()
                {
                    return None;
                }
                let preferred = policy
                    .trade_trust_target
                    .map_or(policy.prefer_open, |target| {
                        Some(
                            opinions
                                .as_ref()
                                .map_or(0, |index| index.trust(person, target))
                                >= policy.trade_trust_threshold,
                        )
                    })?;
                (preferred != body.trade_open).then_some((body.id, person))
            })
        })
        .collect();
    exits.par_sort_unstable();
    exits
}
