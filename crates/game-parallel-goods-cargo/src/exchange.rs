//! Atomic local transfer of possessions between people.
use crate::{GoodsCargo, GoodsHolding};
use game_parallel_production_model::{Amount, Good, PersonSnapshot, person_index};
use rayon::prelude::*;
use smallvec::SmallVec;
use std::collections::HashSet;
/// The host decides how the participants came to perform this physical exchange.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub a: u64,
    pub b: u64,
    pub offer_a: SmallVec<[Amount; 1]>,
    pub offer_b: SmallVec<[Amount; 1]>,
}
fn move_items(source: &mut SmallVec<[Amount; 2]>, offered: &[Amount]) -> Result<(), String> {
    for (index, item) in offered.iter().enumerate() {
        if item.quantity == 0 || offered[..index].iter().any(|other| other.good == item.good) {
            return Err("invalid offer".into());
        }
        let amount = source
            .iter_mut()
            .find(|held| held.good == item.good)
            .ok_or("offered good not held")?;
        amount.quantity = amount
            .quantity
            .checked_sub(item.quantity)
            .ok_or("insufficient offered goods")?;
    }
    source.retain(|item| item.quantity > 0);
    Ok(())
}
fn receive(
    target: &mut SmallVec<[Amount; 2]>,
    offered: &[Amount],
    capacity: u64,
) -> Result<(), String> {
    for item in offered {
        if let Some(amount) = target.iter_mut().find(|held| held.good == item.good) {
            amount.quantity = amount
                .quantity
                .checked_add(item.quantity)
                .ok_or("goods overflow")?;
        } else {
            target.push(item.clone());
        }
    }
    if target
        .iter()
        .map(|item| u128::from(item.quantity))
        .sum::<u128>()
        > u128::from(capacity)
    {
        return Err("carrying capacity exceeded".into());
    }
    target.sort_unstable_by_key(|item| item.good);
    Ok(())
}
struct View<'a> {
    people: &'a [PersonSnapshot],
    holdings: Vec<&'a GoodsHolding>,
    dense_start: Option<u64>,
    busy: HashSet<u64>,
    capacity: u64,
}
impl View<'_> {
    fn evaluate(&self, exchange: &Exchange) -> Result<[Option<GoodsHolding>; 2], String> {
        if exchange.a == exchange.b || (exchange.offer_a.is_empty() && exchange.offer_b.is_empty())
        {
            return Err("empty or self exchange".into());
        }
        if self.busy.contains(&exchange.a) || self.busy.contains(&exchange.b) {
            return Err("exchange participant is busy".into());
        }
        let a = person_index(self.people, exchange.a)
            .map(|index| self.people[index])
            .ok_or("participant must be local")?;
        let b = person_index(self.people, exchange.b)
            .map(|index| self.people[index])
            .ok_or("participant must be local")?;
        if a.1.is_none() || a.1 != b.1 || a.2 == 0 || b.2 == 0 {
            return Err("participants must be living and together".into());
        }
        let stock = |person: u64| -> SmallVec<[Amount; 2]> {
            self.dense_start
                .and_then(|first| person.checked_sub(first))
                .and_then(|offset| usize::try_from(offset).ok())
                .filter(|&index| index < self.holdings.len())
                .or_else(|| {
                    if self.dense_start.is_none() {
                        self.holdings
                            .binary_search_by_key(&person, |holding| holding.person)
                            .ok()
                    } else {
                        None
                    }
                })
                .map_or_else(SmallVec::new, |index| self.holdings[index].items.clone())
        };
        let mut a = stock(exchange.a);
        let mut b = stock(exchange.b);
        move_items(&mut a, &exchange.offer_a)?;
        move_items(&mut b, &exchange.offer_b)?;
        receive(&mut a, &exchange.offer_b, self.capacity)?;
        receive(&mut b, &exchange.offer_a, self.capacity)?;
        Ok([
            (!a.is_empty()).then_some(GoodsHolding {
                person: exchange.a,
                items: a,
            }),
            (!b.is_empty()).then_some(GoodsHolding {
                person: exchange.b,
                items: b,
            }),
        ])
    }
}
impl GoodsCargo {
    fn view<'a>(
        &'a self,
        people: &'a [PersonSnapshot],
        goods: &[Good],
        busy: &[u64],
    ) -> Result<View<'a>, String> {
        self.validate_index(people, goods)?;
        let mut holdings: Vec<_> = self.holdings.par_iter().collect();
        if holdings
            .par_windows(2)
            .any(|pair| pair[0].person > pair[1].person)
        {
            holdings.par_sort_unstable_by_key(|holding| holding.person);
        }
        let dense_start = holdings.first().and_then(|first| {
            holdings.last().and_then(|last| {
                (last.person.checked_sub(first.person) == Some((holdings.len() - 1) as u64))
                    .then_some(first.person)
            })
        });
        Ok(View {
            people,
            holdings,
            dense_start,
            busy: busy.par_iter().copied().collect(),
            capacity: self.capacity,
        })
    }
    /// Stage each proposal against the same physical snapshot without publishing it.
    pub fn assess_exchanges(
        &self,
        people: &[PersonSnapshot],
        goods: &[Good],
        busy: &[u64],
        exchanges: &[Exchange],
    ) -> Result<Vec<Option<[Option<GoodsHolding>; 2]>>, String> {
        let view = self.view(people, goods, busy)?;
        Ok(exchanges
            .par_iter()
            .map(|exchange| view.evaluate(exchange).ok())
            .collect())
    }
    /// Exchanges are independent; no person may participate twice in one batch.
    pub fn exchange(
        &self,
        people: &[PersonSnapshot],
        goods: &[Good],
        busy: &[u64],
        exchanges: &[Exchange],
    ) -> Result<Self, String> {
        let view = self.view(people, goods, busy)?;
        let mut actors: Vec<_> = exchanges
            .par_iter()
            .flat_map_iter(|exchange| [exchange.a, exchange.b])
            .collect();
        actors.par_sort_unstable();
        if actors.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate exchange participant".into());
        }
        let changed: Vec<_> = exchanges
            .par_iter()
            .map(|exchange| view.evaluate(exchange))
            .collect::<Result<_, _>>()?;
        let mut next: Vec<_> = self
            .holdings
            .par_iter()
            .filter(|holding| actors.binary_search(&holding.person).is_err())
            .cloned()
            .collect();
        next.par_extend(changed.into_par_iter().flatten().flatten());
        next.par_sort_unstable_by_key(|holding| holding.person);
        Ok(Self {
            capacity: self.capacity,
            holdings: next,
        })
    }
}
