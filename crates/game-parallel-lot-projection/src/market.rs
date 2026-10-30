//! Individual trade decisions staged against actual lot custody.
use crate::cargo;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction, View};
use game_parallel_market::{Decision, DecisionDraft, Event, Market, Offer, Outcome};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{Good, PersonSnapshot};
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
        outcomes.par_sort_unstable_by_key(|row| row.id);
        if outcomes.par_windows(2).any(|rows| rows[0].id == rows[1].id) {
            return Err("duplicate physical trade outcome".into());
        }
        let mut offers: Vec<_> = market.offers.par_iter().collect();
        offers.par_sort_unstable_by_key(|row| row.id);
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
}
pub struct Context<'a> {
    pub market: &'a Market,
    pub view: &'a View,
    pub capacity: u64,
    pub people: &'a [PersonSnapshot],
    pub goods: &'a [Good],
    pub busy: &'a [u64],
    pub opinions: Option<&'a Opinions>,
}
impl Context<'_> {
    /// `action_base` reserves a caller-selected, nonzero range for these choices.
    pub fn plan(&self, action_base: u64) -> Result<Plan, String> {
        let Self {
            market,
            view,
            capacity,
            people,
            goods,
            busy,
            opinions,
        } = *self;
        if action_base == 0 {
            return Err("invalid trade action base".into());
        }
        let cargo = cargo(view, capacity)?;
        let mut selected = game_parallel_market_auto::selected_offers(
            market,
            &cargo,
            people,
            goods,
            busy,
            view.boundary,
            opinions,
        )?;
        selected.par_sort_unstable_by_key(|event| event.offer);
        let mut offers: Vec<_> = market.offers.par_iter().collect();
        offers.par_sort_unstable_by_key(|row| row.id);
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
