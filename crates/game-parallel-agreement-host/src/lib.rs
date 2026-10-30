//! Compose promises and physical custody without inventing goods or consent.
use game_parallel_agreements::{Agreements, Decision, Delivery, Proposal};
use game_parallel_goods_cargo::{Exchange, GoodsCargo};
use game_parallel_people::Region;
use game_parallel_production::Production;
use game_parallel_production_model::{Amount, PersonSnapshot, person_index};
use game_parallel_transport::Traveler;
use rayon::prelude::*;
use std::collections::HashSet;

pub struct Context<'a> {
    pub agreements: &'a mut Option<Agreements>,
    pub opinions: &'a mut Option<game_parallel_opinions::Opinions>,
    pub production: &'a Option<Production>,
    pub cargo: &'a mut Option<GoodsCargo>,
    pub regions: &'a [Region],
    pub travelers: &'a [Traveler],
    pub tick: u64,
    pub suspended: Option<u64>,
}
impl Context<'_> {
    /// Change one person's standing choice; this does not discharge any obligation.
    pub fn set_honor(&mut self, person: u64, honor: bool, bulk: bool) -> Result<(), String> {
        if self.suspended == Some(person) {
            return Err("player control is suspended".into());
        }
        let people = self.people()?;
        if person_index(&people, person).is_none_or(|index| people[index].2 == 0) {
            return Err("agreement policy requires a living person".into());
        }
        if bulk && !honor {
            return Err("bulk fulfillment requires choosing to honor promises".into());
        }
        let state = self.agreements.as_mut().ok_or("agreements are disabled")?;
        if let Some(policy) = state
            .policies
            .iter_mut()
            .find(|policy| policy.person == person)
        {
            policy.honor = honor;
        } else {
            state.policies.push(game_parallel_agreements::Policy {
                person,
                honor,
                accept: false,
                minimum_trust: None,
                minimum_honor_trust: None,
                fear_honor_threshold: None,
                obligation_honor_threshold: None,
                accept_pressure_threshold: None,
            });
        }
        state.bulk_debtors.retain(|id| *id != person);
        if bulk {
            state.bulk_debtors.push(person);
        }
        Ok(())
    }
    fn people(&self) -> Result<Vec<PersonSnapshot>, String> {
        self.production
            .as_ref()
            .ok_or("production is disabled")?
            .validate_people(self.regions, self.travelers, self.tick)
    }
    fn local_pair(people: &[PersonSnapshot], a: u64, b: u64) -> bool {
        let Some(a) = person_index(people, a).map(|i| people[i]) else {
            return false;
        };
        let Some(b) = person_index(people, b).map(|i| people[i]) else {
            return false;
        };
        a.2 > 0 && b.2 > 0 && a.1.is_some() && a.1 == b.1
    }
    pub fn propose(&mut self, proposals: &[Proposal]) -> Result<Vec<u64>, String> {
        let people = self.people()?;
        let busy: HashSet<_> = self
            .production
            .as_ref()
            .unwrap()
            .busy_people()
            .into_iter()
            .collect();
        if proposals.par_iter().any(|p| {
            self.suspended == Some(p.debtor)
                || self.suspended == Some(p.creditor)
                || busy.contains(&p.debtor)
                || busy.contains(&p.creditor)
                || !Self::local_pair(&people, p.debtor, p.creditor)
        }) {
            return Err("agreement participants must be living and together".into());
        }
        let identities: HashSet<_> = people.par_iter().map(|p| p.0).collect();
        let goods = self
            .production
            .as_ref()
            .unwrap()
            .goods
            .iter()
            .map(|good| good.id)
            .collect();
        let (next, ids) = self
            .agreements
            .as_ref()
            .ok_or("agreements are disabled")?
            .propose(proposals, &identities, &goods, self.tick)?;
        *self.agreements = Some(next);
        Ok(ids)
    }
    pub fn decide(&mut self, decisions: &[Decision]) -> Result<(), String> {
        let people = self.people()?;
        let state = self.agreements.as_ref().ok_or("agreements are disabled")?;
        let busy: HashSet<_> = self
            .production
            .as_ref()
            .unwrap()
            .busy_people()
            .into_iter()
            .collect();
        let mut contracts: Vec<_> = state.contracts.par_iter().collect();
        contracts.par_sort_unstable_by_key(|contract| contract.id);
        if decisions.par_iter().any(|decision| {
            self.suspended == Some(decision.actor)
                || busy.contains(&decision.actor)
                || contracts
                    .binary_search_by_key(&decision.contract, |c| c.id)
                    .ok()
                    .map(|i| contracts[i])
                    .is_none_or(|contract| {
                        if decision.actor == contract.debtor && !decision.accept {
                            person_index(&people, decision.actor).is_none_or(|i| people[i].2 == 0)
                        } else {
                            self.suspended == Some(contract.debtor)
                                || self.suspended == Some(contract.creditor)
                                || busy.contains(&contract.debtor)
                                || busy.contains(&contract.creditor)
                                || !Self::local_pair(&people, contract.debtor, contract.creditor)
                        }
                    })
        }) {
            return Err("agreement decision requires available participants".into());
        }
        *self.agreements = Some(state.decide(decisions, self.tick)?);
        Ok(())
    }
    pub fn deliver(&mut self, deliveries: &[Delivery]) -> Result<(), String> {
        if deliveries.is_empty() {
            return Ok(());
        }
        if deliveries
            .par_iter()
            .any(|delivery| self.suspended == Some(delivery.actor))
        {
            return Err("player control is suspended".into());
        }
        let production = self.production.as_ref().ok_or("production is disabled")?;
        let people = self.people()?;
        let draft = self
            .agreements
            .as_ref()
            .ok_or("agreements are disabled")?
            .deliver(deliveries, self.tick)?;
        if draft.transfers.par_iter().any(|transfer| {
            self.suspended == Some(transfer.debtor) || self.suspended == Some(transfer.creditor)
        }) {
            return Err("player control is suspended".into());
        }
        let exchanges: Vec<_> = draft
            .transfers
            .par_iter()
            .map(|transfer| Exchange {
                a: transfer.debtor,
                b: transfer.creditor,
                offer_a: [Amount {
                    good: transfer.good,
                    quantity: transfer.quantity,
                }]
                .into_iter()
                .collect(),
                offer_b: Default::default(),
            })
            .collect();
        let cargo = self.cargo.as_ref().ok_or("goods cargo is disabled")?;
        let next = cargo.exchange(
            &people,
            &production.goods,
            &production.busy_people(),
            &exchanges,
        )?;
        *self.cargo = Some(next);
        if let Some(opinions) = &mut self.opinions {
            opinions.advance_validated(&draft.state.contracts, &draft.state.events, self.tick);
        }
        *self.agreements = Some(draft.state);
        Ok(())
    }
}
/// Embed the typed public commands into the editable World host.
#[macro_export]
macro_rules! agreement_methods {
    () => {
        fn agreement_context(&mut self) -> game_parallel_agreement_host::Context<'_> {
            let suspended = self.suspended_person();
            game_parallel_agreement_host::Context {
                agreements: &mut self.agreements,
                opinions: &mut self.opinions,
                production: &self.production,
                cargo: &mut self.goods_cargo,
                regions: &self.regions,
                travelers: &self.travelers,
                tick: self.tick,
                suspended,
            }
        }
        pub fn propose_agreements(
            &mut self,
            proposals: &[AgreementProposal],
        ) -> Result<Vec<u64>, String> {
            self.agreement_context().propose(proposals)
        }
        pub fn set_agreement_honor(
            &mut self,
            person: u64,
            honor: bool,
            bulk: bool,
        ) -> Result<(), String> {
            self.agreement_context().set_honor(person, honor, bulk)
        }
        pub fn decide_agreements(&mut self, decisions: &[AgreementDecision]) -> Result<(), String> {
            self.agreement_context().decide(decisions)
        }
        pub fn deliver_agreements(
            &mut self,
            deliveries: &[AgreementDelivery],
        ) -> Result<(), String> {
            self.agreement_context().deliver(deliveries)
        }
    };
}
/// Resolve affected actors for a batch without a serial per-request graph scan.
pub fn participants(state: Option<&Agreements>, ids: &[u64]) -> Vec<u64> {
    let Some(state) = state else {
        return Vec::new();
    };
    let mut contracts: Vec<_> = state.contracts.par_iter().collect();
    contracts.par_sort_unstable_by_key(|contract| contract.id);
    ids.par_iter()
        .flat_map_iter(|id| {
            contracts
                .binary_search_by_key(id, |contract| contract.id)
                .ok()
                .map(|index| [contracts[index].debtor, contracts[index].creditor])
                .into_iter()
                .flatten()
        })
        .collect()
}
/// Withdrawal is unilateral; other decisions reserve both participants.
pub fn decision_participants(state: Option<&Agreements>, decisions: &[Decision]) -> Vec<u64> {
    let Some(state) = state else {
        return decisions
            .par_iter()
            .map(|decision| decision.actor)
            .collect();
    };
    let mut contracts: Vec<_> = state.contracts.par_iter().collect();
    contracts.par_sort_unstable_by_key(|contract| contract.id);
    decisions
        .par_iter()
        .flat_map_iter(|decision| {
            let mut actors = vec![decision.actor];
            if let Ok(index) =
                contracts.binary_search_by_key(&decision.contract, |contract| contract.id)
            {
                let contract = contracts[index];
                if !(decision.actor == contract.debtor && !decision.accept) {
                    actors.extend([contract.debtor, contract.creditor]);
                }
            }
            actors
        })
        .collect()
}
/// The fused host checks actor reservations before using the same transactional command.
#[macro_export]
macro_rules! fused_agreement_methods {
    () => {
        pub fn set_agreement_honor(
            &mut self,
            person: u64,
            honor: bool,
            bulk: bool,
        ) -> Result<(), String> {
            self.edit_world(|world| world.set_agreement_honor(person, honor, bulk))
        }
        pub fn propose_agreements(
            &mut self,
            proposals: &[game_parallel::AgreementProposal],
        ) -> Result<Vec<u64>, String> {
            let actors: Vec<_> = proposals
                .iter()
                .flat_map(|proposal| [proposal.debtor, proposal.creditor])
                .collect();
            game_parallel_fused_conflict::check(&self.template, &actors)?;
            self.edit_world(|world| world.propose_agreements(proposals))
        }
        pub fn decide_agreements(
            &mut self,
            decisions: &[game_parallel::AgreementDecision],
        ) -> Result<(), String> {
            let actors = game_parallel_agreement_host::decision_participants(
                self.template.agreements.as_ref(),
                decisions,
            );
            game_parallel_fused_conflict::check(&self.template, &actors)?;
            self.edit_world(|world| world.decide_agreements(decisions))
        }
        pub fn deliver_agreements(
            &mut self,
            deliveries: &[game_parallel::AgreementDelivery],
        ) -> Result<(), String> {
            let ids: Vec<_> = deliveries
                .iter()
                .map(|delivery| delivery.contract)
                .collect();
            let actors =
                game_parallel_agreement_host::participants(self.template.agreements.as_ref(), &ids);
            game_parallel_fused_conflict::check(&self.template, &actors)?;
            self.edit_world(|world| world.deliver_agreements(deliveries))
        }
    };
}
