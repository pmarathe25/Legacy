//! Local people may propose a swap based on their own values and visible possessions.
use game_parallel_goods_cargo::{GoodsCargo, GoodsHolding};
use game_parallel_market::{Market, Offer, Policy};
use game_parallel_production_model::{Amount, PersonSnapshot};
use rayon::prelude::*;
use std::cmp::Reverse;
pub struct Draft {
    offers: Vec<Offer>,
    next_id: u64,
    pub actors: Vec<u64>,
}
impl Draft {
    pub fn commit(self, market: &mut Market) {
        market.offers.extend(self.offers);
        market.next_id = self.next_id;
    }
}
fn choose(a: u64, b: u64, policy: &Policy, holdings: &[&GoodsHolding], tick: u64) -> Option<Offer> {
    let find = |person| {
        holdings
            .binary_search_by_key(&person, |holding| holding.person)
            .ok()
            .map(|index| holdings[index])
    };
    let own = find(a)?;
    let other = find(b)?;
    let mut best: Option<(u64, Reverse<u32>, Reverse<u32>)> = None;
    for given in &own.items {
        for wanted in &other.items {
            if given.good == wanted.good {
                continue;
            }
            let Some(give) = policy
                .values
                .iter()
                .find(|value| value.good == given.good)
                .map(|value| value.value)
            else {
                continue;
            };
            let Some(receive) = policy
                .values
                .iter()
                .find(|value| value.good == wanted.good)
                .map(|value| value.value)
            else {
                continue;
            };
            let Some(gain) = receive.checked_sub(give) else {
                continue;
            };
            if gain == 0 || gain < policy.min_gain {
                continue;
            }
            let candidate = (gain, Reverse(given.good), Reverse(wanted.good));
            if best.is_none_or(|old| candidate > old) {
                best = Some(candidate);
            }
        }
    }
    let (_, Reverse(give), Reverse(receive)) = best?;
    Some(Offer {
        id: 0,
        proposer: a,
        recipient: b,
        give: std::iter::once(Amount {
            good: give,
            quantity: 1,
        })
        .collect(),
        receive: std::iter::once(Amount {
            good: receive,
            quantity: 1,
        })
        .collect(),
        proposed_at: tick,
    })
}
fn ask(a: u64, b: u64, policy: &Policy, holdings: &[&GoodsHolding], tick: u64) -> Option<Offer> {
    let other = holdings
        .binary_search_by_key(&b, |holding| holding.person)
        .ok()
        .map(|index| holdings[index])?;
    let wanted = other
        .items
        .iter()
        .filter_map(|item| {
            policy
                .values
                .iter()
                .find(|value| value.good == item.good)
                .filter(|value| value.value > 0 && value.value >= policy.min_gain)
                .map(|value| (value.value, Reverse(item.good)))
        })
        .max()?;
    Some(Offer {
        id: 0,
        proposer: a,
        recipient: b,
        give: Default::default(),
        receive: std::iter::once(Amount {
            good: wanted.1.0,
            quantity: 1,
        })
        .collect(),
        proposed_at: tick,
    })
}
pub fn prepare(
    market: &Market,
    cargo: &GoodsCargo,
    people: &[PersonSnapshot],
    busy: &[u64],
    tick: u64,
) -> Result<Option<Draft>, String> {
    if !market
        .policies
        .par_iter()
        .any(|p| p.enabled && (p.propose || p.request_help))
    {
        return Ok(None);
    }
    let mut busy = busy.to_vec();
    busy.par_sort_unstable();
    let mut involved: Vec<_> = market
        .offers
        .par_iter()
        .flat_map_iter(|offer| [offer.proposer, offer.recipient])
        .collect();
    involved.par_sort_unstable();
    let mut holdings: Vec<_> = cargo.holdings.par_iter().collect();
    holdings.par_sort_unstable_by_key(|holding| holding.person);
    let mut policies: Vec<_> = market
        .policies
        .par_iter()
        .filter(|p| p.enabled && (p.propose || p.request_help))
        .collect();
    policies.par_sort_unstable_by_key(|p| p.person);
    let mut local: Vec<_> = people
        .par_iter()
        .filter_map(|&(person, region, health, _)| {
            let region = region?;
            (health > 0
                && busy.binary_search(&person).is_err()
                && involved.binary_search(&person).is_err()
                && (holdings.binary_search_by_key(&person, |h| h.person).is_ok()
                    || policies
                        .binary_search_by_key(&person, |p| p.person)
                        .ok()
                        .is_some_and(|i| policies[i].request_help)))
            .then_some((region, person))
        })
        .collect();
    local.par_sort_unstable();
    // Match empty-handed requesters to actual holders before considering swaps.
    // Both sets are disjoint and ordered by region, so each person is reserved once.
    let requests: Vec<_> = if policies.par_iter().any(|policy| policy.request_help) {
        let askers: Vec<_> = local
            .par_iter()
            .filter(|row| {
                holdings
                    .binary_search_by_key(&row.1, |holding| holding.person)
                    .is_err()
                    && policies
                        .binary_search_by_key(&row.1, |policy| policy.person)
                        .ok()
                        .is_some_and(|index| policies[index].request_help)
            })
            .copied()
            .collect();
        let holders: Vec<_> = local
            .par_iter()
            .filter(|row| {
                holdings
                    .binary_search_by_key(&row.1, |holding| holding.person)
                    .is_ok()
            })
            .copied()
            .collect();
        askers
            .par_chunk_by(|a, b| a.0 == b.0)
            .flat_map(|group| {
                let start = holders.partition_point(|row| row.0 < group[0].0);
                let end = holders.partition_point(|row| row.0 <= group[0].0);
                group
                    .par_iter()
                    .zip(holders[start..end].par_iter())
                    .filter_map(|(a, b)| {
                        let policy = policies
                            .binary_search_by_key(&a.1, |policy| policy.person)
                            .ok()
                            .map(|index| policies[index])?;
                        policy
                            .request_help
                            .then(|| ask(a.1, b.1, policy, &holdings, tick))
                            .flatten()
                    })
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut reserved: Vec<_> = requests
        .par_iter()
        .flat_map_iter(|offer| [offer.proposer, offer.recipient])
        .collect();
    reserved.par_sort_unstable();
    let remaining: Vec<_> = local
        .into_par_iter()
        .filter(|row| reserved.binary_search(&row.1).is_err())
        .collect();
    let mut offers: Vec<_> = remaining
        .par_chunk_by(|a, b| a.0 == b.0)
        .flat_map(|group| {
            group.par_chunks_exact(2).filter_map(|pair| {
                let a = pair[0].1;
                let b = pair[1].1;
                let policy_a = policies
                    .binary_search_by_key(&a, |policy| policy.person)
                    .ok()
                    .map(|i| policies[i]);
                let policy_b = policies
                    .binary_search_by_key(&b, |policy| policy.person)
                    .ok()
                    .map(|i| policies[i]);
                let plan = |a, b, policy: &Policy| {
                    (policy
                        .propose
                        .then(|| choose(a, b, policy, &holdings, tick))
                        .flatten())
                    .or_else(|| {
                        policy
                            .request_help
                            .then(|| ask(a, b, policy, &holdings, tick))
                            .flatten()
                    })
                };
                policy_a
                    .and_then(|policy| plan(a, b, policy))
                    .or_else(|| policy_b.and_then(|policy| plan(b, a, policy)))
            })
        })
        .collect();
    offers.par_extend(requests);
    if offers.is_empty() {
        return Ok(None);
    }
    offers.par_sort_unstable_by_key(|offer| (offer.proposer, offer.recipient));
    let next_id = market
        .next_id
        .checked_add(u64::try_from(offers.len()).map_err(|_| "offer count overflow")?)
        .ok_or("offer id overflow")?;
    offers
        .par_iter_mut()
        .enumerate()
        .for_each(|(index, offer)| offer.id = market.next_id + index as u64);
    let actors = offers
        .par_iter()
        .flat_map_iter(|offer| [offer.proposer, offer.recipient])
        .collect();
    Ok(Some(Draft {
        offers,
        next_id,
        actors,
    }))
}
