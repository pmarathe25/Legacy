//! Checked personal promises, without automatic compliance or physical transfers.
use game_parallel_agreements::{Agreements, Policy, Status};
pub use game_parallel_agreements::{Decision, Proposal};
use game_parallel_economy_person::{Person, travel::Colocation};
use game_parallel_opinions::Opinions;
use game_parallel_production_model::{Good, PersonSnapshot, person_index};
use rayon::prelude::*;
use std::collections::HashSet;
pub mod offers;

pub struct AutomaticDecisions {
    pub decisions: Vec<Decision>,
    pub actors: Vec<u64>,
}
pub struct Plans {
    pub delivery: Option<game_parallel_lot_projection::agreement::Plan>,
    pub decisions: Vec<Decision>,
}
#[macro_export]
macro_rules! plan_world {
    ($world:expr, $people:expr, $locations:expr, $busy:expr, $tick:expr, $base:expr, $choice:expr) => {{
        let mut plans = $crate::plan(
            $world.agreements.as_ref(),
            &$people,
            $world.opinions.as_ref(),
            &$locations,
            &mut $busy,
            $tick,
            $base,
        )?;
        $crate::manual_delivery!($world, $choice, plans, $people, $locations, $base);
        plans
    }};
}
#[macro_export]
macro_rules! manual_delivery {
    ($world:expr, $choice:expr, $plans:expr, $people:expr, $locations:expr, $base:expr) => {
        if let Some(game_parallel_economy_interaction::PhysicalChoice::Promise {
            person,
            contract,
        }) = $choice
        {
            if $world
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| !$world.people[i].controlled || $world.people[i].health == 0)
            {
                return Err("promise delivery requires a living controlled person".into());
            }
            $plans
                .delivery
                .as_mut()
                .ok_or("promises are disabled")?
                .append_manual(
                    $world.agreements.as_ref().expect("enabled promises"),
                    &$people,
                    Some(&$locations),
                    person,
                    contract,
                    $base,
                )?;
        }
    };
}
impl Plans {
    pub fn action_count(&self) -> usize {
        self.delivery
            .as_ref()
            .map_or(0, |plan| plan.actions().len())
    }
}
pub fn plan(
    owner: Option<&Agreements>,
    people: &[PersonSnapshot],
    opinions: Option<&Opinions>,
    locations: &Colocation,
    busy: &mut Vec<u64>,
    tick: u64,
    action_base: u64,
) -> Result<Plans, String> {
    let Some(owner) = owner else {
        return Ok(Plans {
            delivery: None,
            decisions: vec![],
        });
    };
    let delivery = game_parallel_lot_projection::agreement::Context {
        agreements: owner,
        opinions,
        people,
        locations: Some(locations),
        busy,
        tick,
    }
    .plan(action_base)?;
    busy.extend(delivery.actors());
    busy.par_sort_unstable();
    busy.dedup();
    let automatic = automatic_optional(Some(owner), people, opinions, locations, busy, tick)
        .expect("enabled owner");
    Ok(Plans {
        delivery: Some(delivery),
        decisions: automatic.decisions,
    })
}
/// Each recipient may independently acknowledge one proposal they value.
pub fn automatic(
    owner: &Agreements,
    people: &[PersonSnapshot],
    opinions: Option<&Opinions>,
    locations: &Colocation,
    busy: &[u64],
    tick: u64,
) -> AutomaticDecisions {
    let mut policies: Vec<_> = owner
        .policies
        .par_iter()
        .filter(|p| p.accept || p.accept_pressure_threshold.is_some())
        .collect();
    policies.par_sort_unstable_by_key(|p| p.person);
    let opinions = opinions.map(Opinions::index);
    let mut candidates: Vec<_> =
        owner
            .contracts
            .par_iter()
            .filter_map(|contract| {
                if contract.status != Status::Proposed
                    || contract.due <= tick
                    || busy.binary_search(&contract.creditor).is_ok()
                {
                    return None;
                }
                let policy = policies
                    .binary_search_by_key(&contract.creditor, |p| p.person)
                    .ok()
                    .map(|i| policies[i])?;
                let trust = opinions
                    .as_ref()
                    .map(|index| index.trust(contract.creditor, contract.debtor));
                let feelings = opinions
                    .as_ref()
                    .map(|index| index.feelings(contract.creditor, contract.debtor));
                if !policy.accepts(trust, feelings.map(|f| f.0), feelings.map(|f| f.1)) {
                    return None;
                }
                let a = person_index(people, contract.debtor).map(|i| people[i])?;
                let b = person_index(people, contract.creditor).map(|i| people[i])?;
                let affinity = opinions.as_ref().map_or(0, |index| {
                    index.affinity(contract.creditor, contract.debtor)
                });
                (a.2 > 0 && b.2 > 0 && locations.region((a.0, a.1), (b.0, b.1)).is_some())
                    .then_some((contract.creditor, std::cmp::Reverse(affinity), contract.id))
            })
            .collect();
    candidates.par_sort_unstable();
    let chosen: Vec<_> = candidates
        .par_chunk_by(|a, b| a.0 == b.0)
        .map(|group| group[0])
        .collect();
    AutomaticDecisions {
        decisions: chosen
            .par_iter()
            .map(|&(actor, _, contract)| Decision {
                contract,
                actor,
                accept: true,
            })
            .collect(),
        actors: chosen.par_iter().map(|row| row.0).collect(),
    }
}
pub fn automatic_optional(
    owner: Option<&Agreements>,
    people: &[PersonSnapshot],
    opinions: Option<&Opinions>,
    locations: &Colocation,
    busy: &mut Vec<u64>,
    tick: u64,
) -> Option<AutomaticDecisions> {
    let plan = automatic(owner?, people, opinions, locations, busy, tick);
    busy.extend_from_slice(&plan.actors);
    busy.par_sort_unstable();
    busy.dedup();
    Some(plan)
}

fn local(people: &[Person], locations: &Colocation, a: u64, b: u64) -> bool {
    let find = |id| {
        people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &people[i])
    };
    find(a).zip(find(b)).is_some_and(|(a, b)| {
        a.health > 0
            && b.health > 0
            && locations
                .region((a.id, a.region), (b.id, b.region))
                .is_some()
    })
}
pub fn propose(
    owner: &Agreements,
    people: &[Person],
    locations: &Colocation,
    goods: &[Good],
    proposals: &[Proposal],
    tick: u64,
) -> Result<(Agreements, Vec<u64>), String> {
    if proposals
        .par_iter()
        .any(|row| !local(people, locations, row.debtor, row.creditor))
    {
        return Err("promise participants are not together".into());
    }
    let people: HashSet<_> = people.par_iter().map(|p| p.id).collect();
    let goods: HashSet<_> = goods.par_iter().map(|g| g.id).collect();
    owner
        .propose(proposals, &people, &goods, tick)
        .map_err(str::to_owned)
}
pub fn decide(
    owner: &Agreements,
    people: &[Person],
    locations: &Colocation,
    decisions: &[Decision],
    tick: u64,
) -> Result<Agreements, String> {
    let mut contracts: Vec<_> = owner.contracts.par_iter().collect();
    contracts.par_sort_unstable_by_key(|row| row.id);
    if decisions.par_iter().any(|decision| {
        let actor_alive = people
            .binary_search_by_key(&decision.actor, |p| p.id)
            .ok()
            .is_some_and(|i| people[i].health > 0);
        let contract = contracts
            .binary_search_by_key(&decision.contract, |row| row.id)
            .ok()
            .map(|i| contracts[i]);
        !actor_alive
            || contract.is_none_or(|c| {
                if decision.actor == c.debtor && !decision.accept {
                    false
                } else {
                    !local(people, locations, c.debtor, c.creditor)
                }
            })
    }) {
        return Err("promise decision requires a living participant and local discussion".into());
    }
    owner.decide(decisions, tick).map_err(str::to_owned)
}
/// A person can change their future decision independently of their promises.
pub fn set_honor(
    owner: &Agreements,
    people: &[Person],
    person: u64,
    honor: bool,
) -> Result<Agreements, String> {
    if people
        .binary_search_by_key(&person, |p| p.id)
        .ok()
        .is_none_or(|i| people[i].health == 0)
    {
        return Err("unknown or dead person".into());
    }
    let mut next = owner.clone();
    if let Some(policy) = next.policies.iter_mut().find(|row| row.person == person) {
        policy.honor = honor;
    } else {
        next.policies.push(Policy {
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
    if !honor {
        next.bulk_debtors.retain(|id| *id != person);
    }
    Ok(next)
}

/// Update only the makers who personally interpret their promises as obligations.
pub fn learn_promises(opinions: Option<&mut Opinions>, proposals: &[Proposal]) {
    let Some(opinions) = opinions else {
        return;
    };
    let pairs: Vec<_> = proposals
        .par_iter()
        .map(|p| (p.debtor, p.creditor))
        .collect();
    opinions.learn_promises(&pairs);
}

#[macro_export]
macro_rules! promise_methods {
    () => {
        pub fn propose_promises(
            &mut self,
            proposals: &[$crate::Proposal],
        ) -> Result<Vec<u64>, String> {
            if proposals.is_empty() {
                return Ok(Vec::new());
            }
            let owner = self.agreements.as_ref().ok_or("promises are disabled")?;
            let (next, ids) = $crate::propose(
                owner,
                &self.people,
                &game_parallel_economy_person::travel::Colocation::new(self.travel.as_ref()),
                self.physical.goods(),
                proposals,
                self.tick(),
            )?;
            let mut feelings = self
                .opinions
                .as_ref()
                .filter(|o| o.tracks_promise_obligation())
                .cloned();
            $crate::learn_promises(feelings.as_mut(), proposals);
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agreements = Some(next);
            if feelings.is_some() {
                self.opinions = feelings;
            }
            self.epoch = epoch;
            Ok(ids)
        }
        pub fn decide_promises(&mut self, decisions: &[$crate::Decision]) -> Result<(), String> {
            if decisions.is_empty() {
                return Ok(());
            }
            let owner = self.agreements.as_ref().ok_or("promises are disabled")?;
            let next = $crate::decide(
                owner,
                &self.people,
                &game_parallel_economy_person::travel::Colocation::new(self.travel.as_ref()),
                decisions,
                self.tick(),
            )?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agreements = Some(next);
            self.epoch = epoch;
            Ok(())
        }
        /// Change this person's voluntary inclination to accept offers; pressure remains separate.
        pub fn set_promise_accept_choice(
            &mut self,
            person: u64,
            willing: bool,
        ) -> Result<(), String> {
            if self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            {
                return Err("person must be alive".into());
            }
            let mut next = self.agreements.clone().ok_or("promises are disabled")?;
            next.policies
                .iter_mut()
                .find(|p| p.person == person)
                .ok_or("person has no promise preference")?
                .accept = willing;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agreements = Some(next);
            self.epoch = epoch;
            Ok(())
        }
        /// Change only this individual's voluntary inclination to make a promise.
        pub fn set_promise_offer_choice(
            &mut self,
            person: u64,
            willing: bool,
        ) -> Result<(), String> {
            if self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            {
                return Err("person must be alive".into());
            }
            let mut next = self.agreements.clone().ok_or("promises are disabled")?;
            next.offering
                .iter_mut()
                .find(|p| p.person == person)
                .ok_or("person has no offer preference")?
                .willing = willing;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agreements = Some(next);
            self.epoch = epoch;
            Ok(())
        }
        /// Change this person's willingness to act on their felt obligation.
        pub fn set_promise_obligation_choice(
            &mut self,
            person: u64,
            threshold: Option<u64>,
        ) -> Result<(), String> {
            if threshold == Some(0)
                || self
                    .people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .is_none_or(|i| self.people[i].health == 0)
            {
                return Err("invalid personal promise preference".into());
            }
            let mut next = self.agreements.clone().ok_or("promises are disabled")?;
            let policy = next
                .policies
                .iter_mut()
                .find(|p| p.person == person)
                .ok_or("person has no promise preference")?;
            policy.obligation_honor_threshold = threshold;
            if !policy.honor && policy.fear_honor_threshold.is_none() && threshold.is_none() {
                next.bulk_debtors.retain(|id| *id != person);
            }
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agreements = Some(next);
            self.epoch = epoch;
            Ok(())
        }
        pub fn set_promise_choice(&mut self, person: u64, honor: bool) -> Result<(), String> {
            let owner = self.agreements.as_ref().ok_or("promises are disabled")?;
            let next = $crate::set_honor(owner, &self.people, person, honor)?;
            let epoch = self
                .epoch
                .checked_add(1)
                .ok_or("economic generation overflow")?;
            self.agreements = Some(next);
            self.epoch = epoch;
            Ok(())
        }
    };
}
