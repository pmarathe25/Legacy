//! Person-aligned, staged nonfood cargo transfers.
mod dense;
use game_parallel_goods_cargo::{Direction, GoodsCargo, GoodsHolding, Transfer};
use game_parallel_production_model::{Amount, Inventory, PersonSnapshot, person_index};
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::HashMap;
const BLOCK: usize = 4096;
#[derive(Clone, Default)]
struct Slot {
    items: SmallVec<[Amount; 1]>,
}
pub struct PackedCargo {
    capacity: u64,
    slots: Vec<Slot>,
}
struct Effect<'a> {
    index: usize,
    region: u64,
    request: &'a Transfer,
}
/// Read-only actor index for transfer admission.
pub trait PeopleView: Sync {
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn actor(&self, id: u64) -> Option<(usize, Option<u64>, u32, bool)>;
}
struct Snapshots<'a> {
    people: &'a [PersonSnapshot],
    busy: &'a [u64],
}
impl PeopleView for Snapshots<'_> {
    fn len(&self) -> usize {
        self.people.len()
    }
    fn actor(&self, id: u64) -> Option<(usize, Option<u64>, u32, bool)> {
        let index = person_index(self.people, id)?;
        let (_, region, health, _) = self.people[index];
        Some((index, region, health, self.busy.binary_search(&id).is_ok()))
    }
}
impl PackedCargo {
    /// The source must already have been validated against this sorted index.
    pub fn from_cargo(cargo: &GoodsCargo, people: &[PersonSnapshot]) -> Self {
        let mut slots: Vec<Slot> = people.par_iter().map(|_| Slot::default()).collect();
        for holding in &cargo.holdings {
            let index = person_index(people, holding.person).expect("validated holder");
            slots[index].items = holding.items.iter().cloned().collect();
        }
        Self {
            capacity: cargo.capacity,
            slots,
        }
    }
    /// Birth IDs are allocated above all existing person IDs.
    pub fn extend_people(&mut self, people: &[PersonSnapshot]) {
        if people.len() > self.slots.len() {
            self.slots
                .extend((self.slots.len()..people.len()).map(|_| Slot::default()));
        }
    }
    pub fn cargo(&self, people: &[PersonSnapshot]) -> GoodsCargo {
        assert_eq!(self.slots.len(), people.len());
        let holdings = self
            .slots
            .par_iter()
            .zip(people.par_iter())
            .filter_map(|(slot, person)| {
                (!slot.items.is_empty()).then(|| GoodsHolding {
                    person: person.0,
                    items: slot.items.iter().cloned().collect(),
                })
            })
            .collect();
        GoodsCargo {
            capacity: self.capacity,
            holdings,
        }
    }
    /// Prepare all actor and stock effects from the same snapshot, then publish.
    pub fn transfer(
        &mut self,
        inventories: &mut [Inventory],
        people: &[PersonSnapshot],
        busy: &[u64],
        requests: &[Transfer],
    ) -> Result<(), String> {
        let mut busy = busy.to_vec();
        if busy.par_windows(2).any(|pair| pair[0] > pair[1]) {
            busy.par_sort_unstable();
        }
        self.transfer_view(
            inventories,
            &Snapshots {
                people,
                busy: &busy,
            },
            requests,
        )
    }
    /// Apply a batch using a borrowed actor index without materializing snapshots.
    pub fn transfer_view(
        &mut self,
        inventories: &mut [Inventory],
        people: &impl PeopleView,
        requests: &[Transfer],
    ) -> Result<(), String> {
        if self.slots.len() != people.len() {
            return Err("cargo person index differs".into());
        }
        if let Some(result) = dense::transfer(self, inventories, people, requests) {
            return result;
        }
        let locations: HashMap<_, _> = inventories
            .iter()
            .map(|inventory| (inventory.region, inventory))
            .collect();
        let mut effects: Vec<Effect<'_>> = requests
            .par_iter()
            .map(|request| -> Result<Effect, String> {
                if request.quantity == 0 {
                    return Err("invalid or busy transfer actor".into());
                }
                let (index, region, health, busy) =
                    people.actor(request.person).ok_or("actor must be local")?;
                if busy {
                    return Err("invalid or busy transfer actor".into());
                }
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
                let slot = &self.slots[index];
                let carried = slot
                    .items
                    .iter()
                    .find(|item| item.good == request.good)
                    .map_or(0, |item| item.quantity);
                let total: u128 = slot
                    .items
                    .iter()
                    .map(|item| u128::from(item.quantity))
                    .sum();
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
                Ok(Effect {
                    index,
                    region,
                    request,
                })
            })
            .collect::<Result<_, _>>()?;
        if effects
            .par_windows(2)
            .any(|pair| pair[0].index > pair[1].index)
        {
            effects.par_sort_unstable_by_key(|effect| effect.index);
        }
        if effects
            .par_windows(2)
            .any(|pair| pair[0].index == pair[1].index)
        {
            return Err("duplicate transfer actor".into());
        }
        let totals: HashMap<(u64, u32), (u128, u128)> = effects
            .par_iter()
            .fold(HashMap::new, |mut totals, effect| {
                let total = totals
                    .entry((effect.region, effect.request.good))
                    .or_insert((0, 0));
                match effect.request.direction {
                    Direction::Pickup => total.0 += u128::from(effect.request.quantity),
                    Direction::Deposit => total.1 += u128::from(effect.request.quantity),
                }
                totals
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
                .find(|stock| stock.good == *good)
                .expect("validated stock");
            if pickup > u128::from(stock.quantity)
                || deposit > u128::from(stock.capacity - stock.quantity)
            {
                return Err("shared stock or storage unavailable".into());
            }
        }
        inventories.par_iter_mut().for_each(|inventory| {
            for stock in &mut inventory.stocks {
                if let Some(&(pickup, deposit)) = totals.get(&(inventory.region, stock.good)) {
                    stock.quantity = stock.quantity - pickup as u64 + deposit as u64;
                }
            }
        });
        self.slots
            .par_chunks_mut(BLOCK)
            .enumerate()
            .for_each(|(block, slots)| {
                let start = block * BLOCK;
                let first = effects.partition_point(|effect| effect.index < start);
                let end = effects.partition_point(|effect| effect.index < start + slots.len());
                for effect in &effects[first..end] {
                    let slot = &mut slots[effect.index - start];
                    let request = effect.request;
                    if let Some(item) = slot.items.iter_mut().find(|item| item.good == request.good)
                    {
                        match request.direction {
                            Direction::Pickup => item.quantity += request.quantity,
                            Direction::Deposit => item.quantity -= request.quantity,
                        }
                        slot.items.retain(|item| item.quantity > 0);
                    } else {
                        slot.items.push(Amount {
                            good: request.good,
                            quantity: request.quantity,
                        });
                    }
                    slot.items.sort_unstable_by_key(|item| item.good);
                }
            });
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/support/equivalence.rs"]
mod integration_tests;
