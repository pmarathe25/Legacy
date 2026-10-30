//! Individual decisions to help known people, with disjoint actor arbitration.
mod requests;
pub mod stores;
use game_parallel::{FoodGift, World};
use game_parallel_fused_data::{Block, NONE};
use rayon::prelude::*;
pub use requests::admit;

pub struct AidDraft {
    pub gifts: Vec<FoodGift>,
    pub actors: Vec<u64>,
}
pub fn prepare_aid(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    occupied: &[u64],
    tick: u64,
    learned: &[game_parallel::FoodAidPolicy],
) -> Option<AidDraft> {
    let cargo = world
        .cargo
        .as_ref()
        .filter(|cargo| !cargo.food_aid.is_empty() || !learned.is_empty())?;
    let mut busy = occupied.to_vec();
    busy.extend(world.conflict.as_ref().into_iter().flat_map(|c| c.actors()));
    busy.extend(world.care.as_ref().into_iter().flat_map(|c| c.actors()));
    if busy.par_windows(2).any(|p| p[0] > p[1]) {
        busy.par_sort_unstable();
    }
    let mut holdings: Vec<_> = cargo
        .holdings
        .par_iter()
        .map(|h| (h.person, h.food))
        .collect();
    if holdings.par_windows(2).any(|p| p[0].0 > p[1].0) {
        holdings.par_sort_unstable_by_key(|h| h.0);
    }
    let dense_holdings = holdings.first().and_then(|first| {
        holdings.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((holdings.len() - 1) as u64)).then_some(first.0)
        })
    });
    let dense_positions = positions.first().and_then(|first| {
        positions.last().and_then(|last| {
            (last.0.checked_sub(first.0) == Some((positions.len() - 1) as u64)).then_some(first.0)
        })
    });
    let index = |id: u64, first: Option<u64>, len: usize| {
        first
            .and_then(|base| id.checked_sub(base))
            .and_then(|offset| usize::try_from(offset).ok())
            .filter(|&i| i < len)
    };
    let current = |id| {
        index(id, dense_holdings, holdings.len())
            .or_else(|| {
                if dense_holdings.is_none() {
                    holdings.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .map_or(0, |i| holdings[i].1)
    };
    let locate = |id| {
        index(id, dense_positions, positions.len())
            .or_else(|| {
                if dense_positions.is_none() {
                    positions.binary_search_by_key(&id, |row| row.0).ok()
                } else {
                    None
                }
            })
            .map(|i| {
                let (_, block, slot) = positions[i];
                (&blocks[block], slot)
            })
    };
    let mut requests: Vec<_> = cargo
        .food_requests
        .par_iter()
        .filter(|request| request.expires_at >= tick)
        .collect();
    if requests
        .par_windows(2)
        .any(|p| (p[0].giver, p[0].requester) > (p[1].giver, p[1].requester))
    {
        requests.par_sort_unstable_by_key(|r| (r.giver, r.requester));
    }
    let opinions = world
        .opinions
        .as_ref()
        .filter(|_| !cargo.food_aid.is_empty())
        .map(|owner| owner.index());
    let mut candidates: Vec<_> = cargo
        .food_aid
        .par_iter()
        .map(|policy| (policy, false))
        .chain(learned.par_iter().map(|policy| (policy, true)))
        .filter_map(|(policy, already_scored)| {
            if !policy.enabled
                || !tick.is_multiple_of(policy.interval)
                || busy.binary_search(&policy.giver).is_ok()
                || busy.binary_search(&policy.recipient).is_ok()
            {
                return None;
            }
            let request = requests
                .binary_search_by_key(&(policy.giver, policy.recipient), |r| {
                    (r.giver, r.requester)
                })
                .ok()
                .map(|i| requests[i]);
            let amount = if policy.requires_request {
                let request = request?;
                if request.amount > policy.amount {
                    return None;
                }
                request.amount
            } else {
                policy.amount
            };
            let (a, ai) = locate(policy.giver)?;
            let (b, bi) = locate(policy.recipient)?;
            let score = policy.help_value.saturating_add(
                opinions
                    .as_ref()
                    .filter(|_| !already_scored)
                    .map_or(0, |o| {
                        o.fear(policy.giver, policy.recipient)
                            .saturating_add(o.obligation(policy.giver, policy.recipient))
                    }),
            );
            let after_giving = current(policy.giver).checked_sub(amount)?;
            let after_receiving = current(policy.recipient).checked_add(amount)?;
            (a.journey.is_none()
                && b.journey.is_none()
                && a.region == b.region
                && a.health[ai] > 0
                && b.health[bi] > 0
                && a.work[ai] == NONE
                && b.work[bi] == NONE
                && a.fatigue
                    .as_ref()
                    .is_none_or(|state| state.flags[ai] & 2 == 0)
                && b.fatigue
                    .as_ref()
                    .is_none_or(|state| state.flags[bi] & 2 == 0)
                && b.hunger[bi].saturating_add(world.rules.hunger_per_tick)
                    >= world.rules.hunger_satisfied
                && score >= policy.threshold
                && after_giving >= policy.retain
                && after_receiving <= cargo.capacity)
                .then_some(FoodGift {
                    giver: policy.giver,
                    recipient: policy.recipient,
                    amount,
                })
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    if candidates.par_windows(2).any(|p| {
        (p[0].giver, p[0].recipient, p[0].amount) > (p[1].giver, p[1].recipient, p[1].amount)
    }) {
        candidates.par_sort_unstable_by_key(|gift| (gift.giver, gift.recipient, gift.amount));
    }
    // Both participants choose the first candidate touching them. This conservative
    // one-round matching is bounded and does not turn a crowded recipient into a serial loop.
    let (mut claims, recipients): (Vec<_>, Vec<_>) = rayon::join(
        || {
            candidates
                .par_iter()
                .enumerate()
                .map(|(i, gift)| (gift.giver, i))
                .collect()
        },
        || {
            candidates
                .par_iter()
                .enumerate()
                .map(|(i, gift)| (gift.recipient, i))
                .collect()
        },
    );
    claims.par_extend(recipients);
    if claims.par_windows(2).any(|pair| pair[0] > pair[1]) {
        claims.par_sort_unstable();
    }
    if !claims.par_windows(2).any(|pair| pair[0].0 == pair[1].0) {
        let actors = claims.into_par_iter().map(|row| row.0).collect();
        return Some(AidDraft {
            gifts: candidates,
            actors,
        });
    }
    let first = |person| claims.partition_point(|row| row.0 < person);
    let gifts: Vec<_> = candidates
        .par_iter()
        .enumerate()
        .filter_map(|(i, gift)| {
            (claims[first(gift.giver)].1 == i && claims[first(gift.recipient)].1 == i)
                .then_some(*gift)
        })
        .collect();
    if gifts.is_empty() {
        return None;
    }
    let mut actors: Vec<_> = gifts
        .par_iter()
        .flat_map_iter(|g| [g.giver, g.recipient])
        .collect();
    actors.par_sort_unstable();
    Some(AidDraft { gifts, actors })
}

/// Attach explicit request admission to the fused host.
#[macro_export]
macro_rules! request_methods {
    () => {
        /// Change a person's tendency to ask their known contact.
        pub fn set_food_asking(&mut self, person: u64, enabled: bool) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            if self.blocks[block].health[slot] == 0 {
                return Err("person is dead".into());
            }
            let policy = self
                .template
                .cargo
                .as_mut()
                .and_then(|cargo| {
                    cargo
                        .food_askers
                        .iter_mut()
                        .find(|policy| policy.person == person)
                })
                .ok_or("no food asking preference")?;
            policy.enabled = enabled;
            self.revision = revision;
            Ok(())
        }
        /// Withdraw a person's own pending request without affecting the addressed person.
        pub fn withdraw_food_request(&mut self, person: u64, id: u64) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            if self
                .template
                .lineage
                .as_ref()
                .and_then(|lineage| lineage.suspended_person(self.template.tick))
                == Some(person)
            {
                return Err("player control is suspended".into());
            }
            let index = self
                .positions
                .binary_search_by_key(&person, |row| row.0)
                .map_err(|_| "unknown person")?;
            let (_, block, slot) = self.positions[index];
            if self.blocks[block].health[slot] == 0 {
                return Err("person is dead".into());
            }
            let cargo = self
                .template
                .cargo
                .as_mut()
                .ok_or("personal cargo is disabled")?;
            let index = cargo
                .food_requests
                .iter()
                .position(|request| request.id == id && request.requester == person)
                .ok_or("request not found")?;
            cargo.food_requests.remove(index);
            self.revision = revision;
            Ok(())
        }

        pub fn request_food_batch(
            &mut self,
            requests: &[game_parallel::FoodRequest],
        ) -> Result<(), String> {
            if requests.is_empty() {
                return Ok(());
            }
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            use rayon::prelude::*;
            let next_id = requests
                .par_iter()
                .map(|request| request.id)
                .max()
                .expect("nonempty requests")
                .checked_add(1)
                .ok_or("food request id overflow")?;
            game_parallel_fused_food_aid::admit(
                &self.template,
                &self.blocks,
                &self.positions,
                requests,
            )?;
            let cargo = self.template.cargo.as_mut().expect("validated cargo");
            cargo.food_requests.extend_from_slice(requests);
            cargo.next_food_request = cargo.next_food_request.max(next_id);
            self.revision = revision;
            Ok(())
        }
    };
}
