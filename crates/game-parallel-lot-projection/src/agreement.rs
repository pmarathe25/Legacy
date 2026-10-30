//! People independently choose whether to fulfill their own remembered promises.
use game_parallel_agreements::{Agreements, Contract, Delivery, DeliveryDraft, Status};
use game_parallel_economy_person::travel::Colocation;
use game_parallel_lot_runtime::Events;
use game_parallel_lots::{Movement, Place, Transaction};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{PersonSnapshot, person_index};
use rayon::prelude::*;

fn together(a: PersonSnapshot, b: PersonSnapshot, locations: Option<&Colocation>) -> bool {
    a.2 > 0
        && b.2 > 0
        && locations.map_or_else(
            || a.1.is_some() && a.1 == b.1,
            |locations| locations.region((a.0, a.1), (b.0, b.1)).is_some(),
        )
}

pub struct Plan {
    actions: Vec<Transaction>,
    chosen: Vec<(u64, Contract)>,
    tick: u64,
}
pub fn decay(opinions: Option<&mut Opinions>) {
    if let Some(opinions) = opinions {
        opinions.decay_fear();
    }
}
/// Publish personal promise outcomes only after the corresponding physical boundary.
pub fn advance(
    agreements: &mut Agreements,
    plan: Option<&Plan>,
    accepted: &[game_parallel_agreements::Decision],
    events: &Events,
    people: &[PersonSnapshot],
    locations: Option<&Colocation>,
    opinions: Option<&mut Opinions>,
) -> Result<(), String> {
    let mut delivered = Vec::new();
    if let Some(plan) = plan
        && let Some(settlement) = plan.deliveries(agreements, events)?
    {
        *agreements = settlement.state;
        delivered = std::mem::take(&mut agreements.events);
    }
    let mut acknowledgments = Vec::new();
    if !accepted.is_empty() {
        agreements
            .decide_in_place(accepted, events.tick)
            .map_err(str::to_owned)?;
        acknowledgments = std::mem::take(&mut agreements.events);
    }
    agreements.advance_in_place(events.tick);
    agreements.events.extend(delivered);
    agreements.events.extend(acknowledgments);
    agreements
        .events
        .par_sort_unstable_by_key(|row| row.contract);
    if let Some(opinions) = opinions.filter(|_| {
        agreements.events.par_iter().any(|event| {
            matches!(
                event.kind,
                game_parallel_agreements::EventKind::Fulfilled
                    | game_parallel_agreements::EventKind::Defaulted
            )
        })
    }) {
        let mut contracts: Vec<_> = agreements.contracts.par_iter().collect();
        contracts.par_sort_unstable_by_key(|row| row.id);
        let witnessed: Vec<_> = agreements
            .events
            .par_iter()
            .filter(|event| {
                if !matches!(
                    event.kind,
                    game_parallel_agreements::EventKind::Fulfilled
                        | game_parallel_agreements::EventKind::Defaulted
                ) {
                    return false;
                }
                contracts
                    .binary_search_by_key(&event.contract, |row| row.id)
                    .ok()
                    .map(|index| contracts[index])
                    .is_some_and(|contract| {
                        let a = people
                            .binary_search_by_key(&contract.debtor, |row| row.0)
                            .ok()
                            .map(|i| people[i]);
                        let b = people
                            .binary_search_by_key(&contract.creditor, |row| row.0)
                            .ok()
                            .map(|i| people[i]);
                        a.zip(b).is_some_and(|(a, b)| together(a, b, locations))
                    })
            })
            .cloned()
            .collect();
        opinions.advance_validated(&agreements.contracts, &witnessed, events.tick);
    }
    Ok(())
}
impl Plan {
    /// Append a specifically chosen delivery without changing anyone else's choice.
    pub fn append_manual(
        &mut self,
        agreements: &Agreements,
        people: &[PersonSnapshot],
        locations: Option<&Colocation>,
        actor: u64,
        contract: u64,
        action_base: u64,
    ) -> Result<(), String> {
        let row = agreements
            .contracts
            .iter()
            .find(|row| row.id == contract && row.debtor == actor)
            .ok_or("unknown personal promise")?;
        let a = person_index(people, row.debtor).map(|i| people[i]);
        let b = person_index(people, row.creditor).map(|i| people[i]);
        if row.status != Status::Active
            || row.due < self.tick
            || row.quantity <= row.delivered
            || !a.zip(b).is_some_and(|(a, b)| together(a, b, locations))
        {
            return Err("promise is not deliverable here".into());
        }
        let id = action_base
            .checked_add(self.actions.len() as u64)
            .ok_or("agreement action overflow")?;
        self.chosen.push((id, row.clone()));
        self.actions.push(Transaction {
            id,
            movements: vec![Movement {
                id: 1,
                from: Place::Person(row.debtor),
                to: Some(Place::Person(row.creditor)),
                good: row.good,
                quantity: row.quantity - row.delivered,
            }],
            creations: vec![],
        });
        Ok(())
    }
    pub fn actors(&self) -> Vec<u64> {
        let mut actors: Vec<_> = self
            .chosen
            .par_iter()
            .map(|(_, contract)| contract.debtor)
            .collect();
        actors.par_sort_unstable();
        actors.dedup();
        actors
    }
    pub fn actions(&self) -> &[Transaction] {
        &self.actions
    }
    /// Record only physical transfers that actually succeeded.
    pub fn deliveries(
        &self,
        agreements: &Agreements,
        events: &Events,
    ) -> Result<Option<DeliveryDraft>, String> {
        if events.tick != self.tick {
            return Err("agreement boundary differs".into());
        }
        let mut contracts: Vec<_> = agreements.contracts.par_iter().collect();
        contracts.par_sort_unstable_by_key(|row| row.id);
        let receipts = crate::receipts::Index::new(events)?;
        let mut outcomes: Vec<_> = events.physical_actions.par_iter().collect();
        outcomes.par_sort_unstable_by_key(|row| row.id);
        if outcomes.par_windows(2).any(|rows| rows[0].id == rows[1].id) {
            return Err("duplicate agreement outcome".into());
        }
        let deliveries: Vec<_> = self
            .chosen
            .par_iter()
            .zip(self.actions.par_iter())
            .map(|((action, original), physical)| {
                if contracts
                    .binary_search_by_key(&original.id, |row| row.id)
                    .ok()
                    .is_none_or(|index| contracts[index] != original)
                {
                    return Err("agreement changed after choice".to_string());
                }
                let outcome = outcomes
                    .binary_search_by_key(action, |row| row.id)
                    .ok()
                    .map(|index| outcomes[index])
                    .ok_or("missing agreement transfer outcome")?;
                if !receipts.verify(physical, outcome.accepted) {
                    return Err("agreement receipt differs from chosen action".to_string());
                }
                Ok(outcome.accepted.then_some(Delivery {
                    contract: original.id,
                    actor: original.debtor,
                    quantity: original.quantity - original.delivered,
                }))
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_par_iter()
            .flatten()
            .collect();
        if deliveries.is_empty() {
            return Ok(None);
        }
        agreements
            .deliver(&deliveries, self.tick)
            .map(Some)
            .map_err(str::to_owned)
    }
}
pub struct Context<'a> {
    pub agreements: &'a Agreements,
    pub opinions: Option<&'a Opinions>,
    pub people: &'a [PersonSnapshot],
    pub locations: Option<&'a Colocation>,
    pub busy: &'a [u64],
    pub tick: u64,
}
impl Context<'_> {
    pub fn plan(&self, action_base: u64) -> Result<Plan, String> {
        if action_base == 0
            || self
                .people
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
        {
            return Err("invalid agreement decision context".into());
        }
        let mut busy = self.busy.to_vec();
        busy.par_sort_unstable();
        let mut policies: Vec<_> = self.agreements.policies.par_iter().collect();
        policies.par_sort_unstable_by_key(|row| row.person);
        let opinions = self.opinions.map(Opinions::index);
        let mut candidates: Vec<_> = self
            .agreements
            .contracts
            .par_iter()
            .filter(|contract| {
                if contract.status != Status::Active
                    || contract.due < self.tick
                    || contract.quantity <= contract.delivered
                    || contract.debtor == contract.creditor
                {
                    return false;
                }
                let Some(policy) = policies
                    .binary_search_by_key(&contract.debtor, |row| row.person)
                    .ok()
                    .map(|index| policies[index])
                else {
                    return false;
                };
                let assessment = opinions
                    .as_ref()
                    .map(|index| index.assessment(contract.debtor, contract.creditor));
                let obligation = opinions
                    .as_ref()
                    .map(|index| index.obligation(contract.debtor, contract.creditor));
                if !policy.honors_with_feelings(
                    assessment.map(|row| row.0),
                    assessment.map(|row| row.1),
                    obligation,
                ) {
                    return false;
                }
                let a = person_index(self.people, contract.debtor).map(|index| self.people[index]);
                let b =
                    person_index(self.people, contract.creditor).map(|index| self.people[index]);
                a.zip(b)
                    .is_some_and(|(a, b)| together(a, b, self.locations))
                    && busy.binary_search(&contract.debtor).is_err()
            })
            .collect();
        candidates.par_sort_unstable_by_key(|row| (row.debtor, row.id));
        let mut bulk = self.agreements.bulk_debtors.clone();
        bulk.par_sort_unstable();
        let mut selected: Vec<_> = candidates
            .par_chunk_by(|a, b| a.debtor == b.debtor)
            .flat_map_iter(|group| {
                group
                    .iter()
                    .take(if bulk.binary_search(&group[0].debtor).is_ok() {
                        group.len()
                    } else {
                        1
                    })
                    .copied()
            })
            .collect();
        selected.par_sort_unstable_by_key(|row| row.id);
        let rows: Vec<_> = selected
            .par_iter()
            .enumerate()
            .map(|(index, contract)| {
                let action = action_base
                    .checked_add(index as u64)
                    .ok_or("agreement action overflow")?;
                Ok((
                    action,
                    (*contract).clone(),
                    Transaction {
                        id: action,
                        movements: vec![Movement {
                            id: 1,
                            from: Place::Person(contract.debtor),
                            to: Some(Place::Person(contract.creditor)),
                            good: contract.good,
                            quantity: contract.quantity - contract.delivered,
                        }],
                        creations: vec![],
                    },
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let actions = rows.par_iter().map(|row| row.2.clone()).collect();
        let chosen = rows
            .into_par_iter()
            .map(|(id, contract, _)| (id, contract))
            .collect();
        Ok(Plan {
            actions,
            chosen,
            tick: self.tick,
        })
    }
}
