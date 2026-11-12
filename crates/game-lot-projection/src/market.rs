//! Individual trade decisions staged against actual lot custody.
use crate::cargo;
use game_goods_cargo::GoodsCargo;
use game_lot_runtime::Events;
use game_lots::{Movement, Place, Transaction, View};
use game_market::{Decision, DecisionDraft, Event, Market, Offer, Outcome};
use game_opinions::Opinions;
use game_production_model::{Good, PersonSnapshot};
use rayon::prelude::*;

pub struct Plan {
    actions: Vec<Transaction>,
    selected: Vec<(u64, Event, Offer)>,
    tick: u64,
}
impl Plan {
    pub fn actors(&self) -> Vec<u64> {
        let mut actors: Vec<_> = self
            .selected
            .par_iter()
            .flat_map_iter(|(_, _, offer)| [offer.proposer, offer.recipient])
            .collect();
        actors.par_sort_unstable();
        actors.dedup();
        actors
    }
    pub fn actions(&self) -> &[Transaction] {
        &self.actions
    }
    /// Publish only choices whose physical transfers were actually admitted.
    pub fn decisions(
        &self,
        market: &Market,
        events: &Events,
    ) -> Result<Option<DecisionDraft>, String> {
        if events.tick != self.tick {
            return Err("market boundary differs".into());
        }
        let receipts = crate::receipts::Index::new(events)?;
        let mut outcomes: Vec<_> = events.physical_actions.par_iter().collect();
        if outcomes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            outcomes.par_sort_unstable_by_key(|row| row.id);
        }
        if outcomes.par_windows(2).any(|rows| rows[0].id == rows[1].id) {
            return Err("duplicate physical trade outcome".into());
        }
        let mut offers: Vec<_> = market.offers.par_iter().collect();
        if offers.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            offers.par_sort_unstable_by_key(|row| row.id);
        }
        let accepted: Vec<_> = self
            .selected
            .par_iter()
            .zip(self.actions.par_iter())
            .map(|((action, event, original), physical)| {
                if offers
                    .binary_search_by_key(&original.id, |row| row.id)
                    .ok()
                    .is_none_or(|index| offers[index] != original)
                {
                    return Err("trade offer changed after choice".to_string());
                }
                let outcome = outcomes
                    .binary_search_by_key(action, |row| row.id)
                    .ok()
                    .map(|index| outcomes[index])
                    .ok_or("missing physical trade outcome")?;
                if !receipts.verify(physical, outcome.accepted) {
                    return Err("trade receipt differs from chosen action".to_string());
                }
                Ok(outcome.accepted.then_some(event))
            })
            .collect::<Result<_, String>>()?;
        let decisions: Vec<_> = accepted
            .par_iter()
            .filter_map(|event| {
                event.map(|event| Decision {
                    id: event.offer,
                    actor: event.actor,
                    outcome: Outcome::Accepted,
                })
            })
            .collect();
        if decisions.is_empty() {
            return Ok(None);
        }
        let mut draft = market.stage_decisions(&decisions, self.tick, true)?;
        let mut influences: Vec<_> = accepted
            .par_iter()
            .filter_map(|event| {
                event.and_then(|event| {
                    event
                        .influence
                        .as_ref()
                        .map(|value| (event.offer, value.clone()))
                })
            })
            .collect();
        influences.par_sort_unstable_by_key(|row| row.0);
        draft.record_influences(&influences);
        Ok(Some(draft))
    }
    /// Give all movement legs unique identities before interval-wide arbitration.
    pub fn interval_actions(&self) -> Result<Vec<Transaction>, String> {
        let counts: Vec<_> = self
            .actions
            .par_iter()
            .map(|action| u64::try_from(action.movements.len()).map_err(|_| "trade leg overflow"))
            .collect::<Result<_, _>>()?;
        let offsets = game_lot_allocation::prefixes(&counts);
        if offsets
            .last()
            .zip(counts.last())
            .is_some_and(|(&offset, &count)| offset + u128::from(count) > u128::from(u64::MAX))
        {
            return Err("trade leg overflow".into());
        }
        Ok(self
            .actions
            .par_iter()
            .zip(offsets.par_iter())
            .map(|(action, &offset)| {
                let mut next = action.clone();
                for (index, movement) in next.movements.iter_mut().enumerate() {
                    movement.id = offset as u64 + index as u64 + 1;
                }
                next
            })
            .collect())
    }
    /// Reconcile outcomes from the physical draft prepared directly from these actions.
    pub fn interval_decisions(
        &self,
        market: &Market,
        tick: u64,
        outcomes: &[game_lots::TransactionOutcome],
    ) -> Result<Option<DecisionDraft>, String> {
        if tick != self.tick {
            return Err("market interval differs".into());
        }
        let mut outcomes: Vec<_> = outcomes.par_iter().collect();
        if outcomes.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            outcomes.par_sort_unstable_by_key(|row| row.id);
        }
        if outcomes.par_windows(2).any(|pair| pair[0].id == pair[1].id) {
            return Err("duplicate physical trade outcome".into());
        }
        let mut offers: Vec<_> = market.offers.par_iter().collect();
        if offers.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            offers.par_sort_unstable_by_key(|row| row.id);
        }
        let accepted: Vec<_> = self
            .selected
            .par_iter()
            .map(|(action, event, original)| -> Result<_, String> {
                if offers
                    .binary_search_by_key(&original.id, |row| row.id)
                    .ok()
                    .is_none_or(|i| offers[i] != original)
                {
                    return Err("trade offer changed after choice".into());
                }
                let outcome = outcomes
                    .binary_search_by_key(action, |row| row.id)
                    .ok()
                    .map(|i| outcomes[i])
                    .ok_or("missing physical trade outcome")?;
                Ok(outcome.accepted.then_some(event))
            })
            .collect::<Result<_, _>>()?;
        let decisions: Vec<_> = accepted
            .par_iter()
            .filter_map(|event| {
                event.map(|event| Decision {
                    id: event.offer,
                    actor: event.actor,
                    outcome: Outcome::Accepted,
                })
            })
            .collect();
        if decisions.is_empty() {
            return Ok(None);
        }
        let mut draft = market.stage_decisions(&decisions, tick, true)?;
        let mut influences: Vec<_> = accepted
            .par_iter()
            .filter_map(|event| {
                event.and_then(|event| {
                    event
                        .influence
                        .as_ref()
                        .map(|value| (event.offer, value.clone()))
                })
            })
            .collect();
        influences.par_sort_unstable_by_key(|row| row.0);
        draft.record_influences(&influences);
        Ok(Some(draft))
    }
}
pub struct Context<'a> {
    pub market: &'a Market,
    pub view: &'a View,
    pub capacity: u64,
    pub people: &'a [PersonSnapshot],
    pub goods: &'a [Good],
    pub busy: &'a [u64],
    pub opinions: Option<&'a Opinions>,
    pub together: Option<&'a game_market_auto::Together<'a>>,
}
impl Context<'_> {
    /// `action_base` reserves a caller-selected, nonzero range for these choices.
    pub fn plan(&self, action_base: u64) -> Result<Plan, String> {
        if action_base == 0 {
            return Err("invalid trade action base".into());
        }
        let cargo = cargo(self.view, self.capacity)?;
        self.plan_with_cargo(action_base, &cargo)
    }

    /// Reuse the same boundary's already projected personal cargo.
    pub fn plan_with_cargo(&self, action_base: u64, cargo: &GoodsCargo) -> Result<Plan, String> {
        let Self {
            market,
            view,
            capacity: _,
            people,
            goods,
            busy,
            opinions,
            together,
        } = *self;
        if action_base == 0 {
            return Err("invalid trade action base".into());
        }
        let mut selected = game_market_auto::selected_offers_together(
            market,
            cargo,
            people,
            goods,
            busy,
            view.boundary,
            (opinions, together),
        )?;
        if selected
            .par_windows(2)
            .any(|pair| pair[0].offer > pair[1].offer)
        {
            selected.par_sort_unstable_by_key(|event| event.offer);
        }
        let mut offers: Vec<_> = market.offers.par_iter().collect();
        if offers.par_windows(2).any(|pair| pair[0].id > pair[1].id) {
            offers.par_sort_unstable_by_key(|row| row.id);
        }
        let rows: Vec<_> = selected
            .par_iter()
            .enumerate()
            .map(|(index, event)| {
                let action = action_base
                    .checked_add(index as u64)
                    .ok_or("trade action overflow")?;
                let offer = offers[offers
                    .binary_search_by_key(&event.offer, |offer| offer.id)
                    .map_err(|_| "selected offer missing")?];
                let movements = offer
                    .give
                    .iter()
                    .map(|item| (offer.proposer, offer.recipient, item))
                    .chain(
                        offer
                            .receive
                            .iter()
                            .map(|item| (offer.recipient, offer.proposer, item)),
                    )
                    .enumerate()
                    .map(|(index, (from, to, item))| {
                        Ok(Movement {
                            batch: None,
                            item: None,
                            id: u64::try_from(index)
                                .map_err(|_| "trade leg overflow")?
                                .checked_add(1)
                                .ok_or("trade leg overflow")?,
                            from: Place::Person(from),
                            to: Some(Place::Person(to)),
                            good: item.good,
                            quantity: item.quantity,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok((
                    action,
                    event.clone(),
                    offer.clone(),
                    Transaction {
                        id: action,
                        movements,
                        creations: vec![],
                    },
                ))
            })
            .collect::<Result<_, String>>()?;
        let actions = rows.par_iter().map(|row| row.3.clone()).collect();
        let selected = rows
            .into_par_iter()
            .map(|(action, event, offer, _)| (action, event, offer))
            .collect();
        Ok(Plan {
            actions,
            selected,
            tick: view.boundary,
        })
    }
}
