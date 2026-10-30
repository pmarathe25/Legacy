//! Portable nonfood goods and atomic local transfers.
mod exchange;
pub use exchange::Exchange;
use game_parallel_people::Region;
use game_parallel_production_model::{Amount, Good, Inventory, PersonSnapshot, person_index};
use game_parallel_transport::Traveler;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodsCargo {
    pub capacity: u64,
    #[serde(default)]
    pub holdings: Vec<GoodsHolding>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoodsHolding {
    pub person: u64,
    pub items: SmallVec<[Amount; 2]>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Pickup,
    Deposit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub person: u64,
    pub good: u32,
    pub quantity: u64,
    pub direction: Direction,
}
impl GoodsCargo {
    pub fn validate(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        goods: &[Good],
    ) -> Result<(), String> {
        if self.capacity == 0 {
            return Err("goods cargo capacity must be positive".into());
        }
        let people = game_parallel_index::snapshot_people(regions, travelers);
        self.validate_index(&people, goods)
    }
    pub fn validate_index(&self, people: &[PersonSnapshot], goods: &[Good]) -> Result<(), String> {
        if self.capacity == 0 {
            return Err("goods cargo capacity must be positive".into());
        }
        if people.par_windows(2).any(|pair| pair[0].0 >= pair[1].0) {
            return Err("invalid person index".into());
        }
        let goods: HashSet<_> = goods.iter().map(|g| g.id).collect();
        let mut holders: Vec<_> = self.holdings.par_iter().map(|h| h.person).collect();
        if holders.par_windows(2).any(|pair| pair[0] > pair[1]) {
            holders.par_sort_unstable();
        }
        if holders.par_windows(2).any(|pair| pair[0] == pair[1])
            || self.holdings.par_iter().any(|holding| {
                let total: u128 = holding
                    .items
                    .iter()
                    .map(|item| u128::from(item.quantity))
                    .sum();
                person_index(people, holding.person).is_none()
                    || holding.items.is_empty()
                    || total > u128::from(self.capacity)
                    || holding.items.iter().enumerate().any(|(index, item)| {
                        item.quantity == 0
                            || !goods.contains(&item.good)
                            || holding.items[..index]
                                .iter()
                                .any(|other| other.good == item.good)
                    })
            })
        {
            return Err("invalid goods holding".into());
        }
        Ok(())
    }
    /// All transfers use the boundary snapshot; the entire batch succeeds or fails.
    pub fn transfer(
        &self,
        inventories: &[Inventory],
        people: &[PersonSnapshot],
        goods: &[Good],
        busy: &[u64],
        requests: &[Transfer],
    ) -> Result<(Self, Vec<Inventory>), String> {
        self.validate_index(people, goods)?;
        self.transfer_validated(inventories, people, busy, requests)
    }
    /// Prepare a transfer against a previously validated private owner state.
    pub fn transfer_validated(
        &self,
        inventories: &[Inventory],
        people: &[PersonSnapshot],
        busy: &[u64],
        requests: &[Transfer],
    ) -> Result<(Self, Vec<Inventory>), String> {
        let locations: HashMap<_, _> = inventories
            .iter()
            .map(|inventory| (inventory.region, inventory))
            .collect();
        let mut ordered = None;
        if self
            .holdings
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            let mut holdings: Vec<_> = self.holdings.par_iter().collect();
            holdings.par_sort_unstable_by_key(|holding| holding.person);
            ordered = Some(holdings);
        }
        let first_holding = ordered
            .as_ref()
            .map_or_else(|| self.holdings.first(), |rows| rows.first().copied());
        let last_holding = ordered
            .as_ref()
            .map_or_else(|| self.holdings.last(), |rows| rows.last().copied());
        let dense_holdings = first_holding.and_then(|first| {
            last_holding.and_then(|last| {
                (last.person.checked_sub(first.person) == Some((self.holdings.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        let holding_for = |person: u64| -> Option<&GoodsHolding> {
            let index = dense_holdings
                .and_then(|first| person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < self.holdings.len())
                .or_else(|| {
                    if dense_holdings.is_none() {
                        ordered
                            .as_ref()
                            .map_or_else(
                                || {
                                    self.holdings
                                        .binary_search_by_key(&person, |holding| holding.person)
                                },
                                |rows| rows.binary_search_by_key(&person, |holding| holding.person),
                            )
                            .ok()
                    } else {
                        None
                    }
                });
            index.and_then(|index| {
                ordered
                    .as_ref()
                    .map_or_else(|| self.holdings.get(index), |rows| rows.get(index).copied())
            })
        };
        let mut busy = busy.to_vec();
        busy.par_sort_unstable();
        let mut actors: Vec<_> = requests.par_iter().map(|request| request.person).collect();
        if actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            actors.par_sort_unstable();
        }
        if actors.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate transfer actor".into());
        }
        let dense_actors = actors.first().and_then(|&first| {
            actors.last().and_then(|&last| {
                (last.checked_sub(first) == Some((actors.len() - 1) as u64)).then_some(first)
            })
        });
        let is_actor = |person: u64| {
            dense_actors
                .is_some_and(|first| person >= first && person - first < actors.len() as u64)
                || (dense_actors.is_none() && actors.binary_search(&person).is_ok())
        };
        let rows: Vec<_> = requests
            .par_iter()
            .map(|request| -> Result<_, String> {
                if request.quantity == 0 || busy.binary_search(&request.person).is_ok() {
                    return Err("invalid or busy transfer actor".into());
                }
                let index = person_index(people, request.person).ok_or("actor must be local")?;
                let (_, region, health, _) = people[index];
                let region = region.ok_or("actor must be local")?;
                if health == 0 {
                    return Err("actor must be living".into());
                }
                let inventory = locations.get(&region).ok_or("missing inventory")?;
                if !inventory
                    .stocks
                    .iter()
                    .any(|stock| stock.good == request.good)
                {
                    return Err("missing stock".into());
                }
                let holding = holding_for(request.person);
                let carried = holding
                    .and_then(|h| h.items.iter().find(|item| item.good == request.good))
                    .map_or(0, |item| item.quantity);
                let total: u128 = holding.map_or(0, |h| {
                    h.items.iter().map(|item| u128::from(item.quantity)).sum()
                });
                match request.direction {
                    Direction::Pickup
                        if total + u128::from(request.quantity) > u128::from(self.capacity) =>
                    {
                        return Err("carrying capacity exceeded".into());
                    }
                    Direction::Deposit if carried < request.quantity => {
                        return Err("insufficient carried goods".into());
                    }
                    _ => {}
                }
                Ok((region, request))
            })
            .collect::<Result<_, _>>()?;
        let totals: HashMap<(u64, u32), (u128, u128)> = rows
            .par_iter()
            .fold(HashMap::new, |mut map, (region, request)| {
                let total = map.entry((*region, request.good)).or_insert((0, 0));
                match request.direction {
                    Direction::Pickup => total.0 += u128::from(request.quantity),
                    Direction::Deposit => total.1 += u128::from(request.quantity),
                }
                map
            })
            .reduce(HashMap::new, |mut left, right| {
                for (key, value) in right {
                    let total = left.entry(key).or_insert((0, 0));
                    total.0 += value.0;
                    total.1 += value.1;
                }
                left
            });
        for ((region, good), &(pickup, deposit)) in &totals {
            let stock = locations[region]
                .stocks
                .iter()
                .find(|s| s.good == *good)
                .unwrap();
            if pickup > u128::from(stock.quantity)
                || deposit > u128::from(stock.capacity - stock.quantity)
            {
                return Err("shared stock or storage unavailable".into());
            }
        }
        let mut next_inventories: Vec<_> = inventories.par_iter().cloned().collect();
        next_inventories.par_iter_mut().for_each(|inventory| {
            for stock in &mut inventory.stocks {
                if let Some(&(pickup, deposit)) = totals.get(&(inventory.region, stock.good)) {
                    stock.quantity = stock.quantity - pickup as u64 + deposit as u64;
                }
            }
        });
        let mut next_holdings: Vec<_> = self
            .holdings
            .par_iter()
            .filter(|holding| !is_actor(holding.person))
            .cloned()
            .collect();
        let changed: Vec<_> = rows
            .par_iter()
            .filter_map(|(_, request)| {
                let mut holding = holding_for(request.person).map_or_else(
                    || GoodsHolding {
                        person: request.person,
                        items: SmallVec::new(),
                    },
                    |h| h.clone(),
                );
                if let Some(item) = holding
                    .items
                    .iter_mut()
                    .find(|item| item.good == request.good)
                {
                    match request.direction {
                        Direction::Pickup => item.quantity += request.quantity,
                        Direction::Deposit => item.quantity -= request.quantity,
                    }
                    holding.items.retain(|item| item.quantity > 0);
                } else {
                    holding.items.push(Amount {
                        good: request.good,
                        quantity: request.quantity,
                    });
                }
                holding.items.sort_unstable_by_key(|item| item.good);
                (!holding.items.is_empty()).then_some(holding)
            })
            .collect();
        if next_holdings.is_empty() {
            next_holdings = changed;
        } else {
            next_holdings.extend(changed);
        }
        if next_holdings
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            next_holdings.par_sort_unstable_by_key(|holding| holding.person);
        }
        Ok((
            Self {
                capacity: self.capacity,
                holdings: next_holdings,
            },
            next_inventories,
        ))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recovery {
    pub actor: u64,
    pub deceased: u64,
    pub good: u32,
    pub quantity: u64,
}
impl GoodsCargo {
    /// A living local person can move goods from a deceased holder into local stock.
    pub fn recover(
        &self,
        inventories: &[Inventory],
        regions: &[Region],
        travelers: &[Traveler],
        goods: &[Good],
        busy: &[u64],
        request: Recovery,
    ) -> Result<(Self, Vec<Inventory>), String> {
        let people = game_parallel_index::snapshot_people(regions, travelers);
        self.recover_indexed(inventories, &people, goods, busy, request)
    }
    pub fn recover_indexed(
        &self,
        inventories: &[Inventory],
        people: &[PersonSnapshot],
        goods: &[Good],
        busy: &[u64],
        request: Recovery,
    ) -> Result<(Self, Vec<Inventory>), String> {
        let Recovery {
            actor,
            deceased,
            good,
            quantity,
        } = request;
        self.validate_index(people, goods)?;
        if quantity == 0 || busy.contains(&actor) {
            return Err("invalid or busy recovery actor".into());
        }
        let a = person_index(people, actor)
            .map(|i| people[i])
            .ok_or("living actor and deceased holder must be local")?;
        let d = person_index(people, deceased)
            .map(|i| people[i])
            .ok_or("living actor and deceased holder must be local")?;
        if a.1.is_none() || a.1 != d.1 || a.2 == 0 || d.2 != 0 {
            return Err("living actor and deceased holder must be local".into());
        }
        let region = a.1.unwrap();
        let holding = self
            .holdings
            .iter()
            .find(|h| h.person == deceased)
            .ok_or("deceased has no goods")?;
        let item = holding
            .items
            .iter()
            .find(|item| item.good == good)
            .ok_or("deceased lacks good")?;
        let stock = inventories
            .iter()
            .find(|i| i.region == region)
            .and_then(|i| i.stocks.iter().find(|s| s.good == good))
            .ok_or("missing stock")?;
        if item.quantity < quantity || quantity > stock.capacity - stock.quantity {
            return Err("goods or storage unavailable".into());
        }
        let mut next = self.clone();
        let holding = next
            .holdings
            .iter_mut()
            .find(|h| h.person == deceased)
            .unwrap();
        holding
            .items
            .iter_mut()
            .find(|item| item.good == good)
            .unwrap()
            .quantity -= quantity;
        holding.items.retain(|item| item.quantity != 0);
        next.holdings.retain(|holding| !holding.items.is_empty());
        let mut inventories = inventories.to_vec();
        inventories
            .iter_mut()
            .find(|i| i.region == region)
            .unwrap()
            .stocks
            .iter_mut()
            .find(|s| s.good == good)
            .unwrap()
            .quantity += quantity;
        Ok((next, inventories))
    }
}
