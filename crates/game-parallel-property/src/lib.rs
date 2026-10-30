//! Explicit ownership of regional nonfood stocks and consensual title transfers.
use game_parallel_goods_cargo::Transfer;
use game_parallel_production_model::{Inventory, PersonSnapshot, Recipe, Start, person_index};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Property {
    pub next_id: u64,
    #[serde(default)]
    pub claims: Vec<Claim>,
    #[serde(default)]
    pub offers: Vec<Offer>,
    #[serde(default)]
    pub thefts: Vec<Theft>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inheritance: Option<Inheritance>,
    #[serde(default)]
    pub estates: Vec<Estate>,
}
/// Optional inheritance policy for indivisible regional stock titles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Inheritance {
    pub minimum_age: u64,
    pub prefer_partner: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Estate {
    pub tick: u64,
    pub region: u64,
    pub good: u32,
    pub previous_owner: u64,
    pub heir: u64,
}
/// A physical unauthorized taking; only a present owner witnesses it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Theft {
    pub tick: u64,
    pub actor: u64,
    pub owner: u64,
    pub region: u64,
    pub good: u32,
    pub quantity: u64,
    pub witnessed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub region: u64,
    pub good: u32,
    pub owner: u64,
    #[serde(default)]
    pub delegates: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub id: u64,
    pub region: u64,
    pub good: u32,
    pub proposer: u64,
    pub recipient: u64,
    pub proposed_at: u64,
}
pub struct Index<'a> {
    claims: HashMap<(u64, u32), Entry<'a>>,
}
struct Entry<'a> {
    owner: u64,
    delegates: Cow<'a, [u64]>,
    dense: Option<u64>,
}
impl Index<'_> {
    /// Unclaimed stocks remain available to everyone.
    pub fn allows(&self, actor: u64, region: u64, good: u32) -> bool {
        self.claims.get(&(region, good)).is_none_or(|claim| {
            claim.owner == actor
                || claim.dense.is_some_and(|first| {
                    actor >= first && actor - first < claim.delegates.len() as u64
                })
                || (claim.dense.is_none() && claim.delegates.binary_search(&actor).is_ok())
        })
    }
    pub fn allows_recipe(&self, actor: u64, region: u64, recipe: &Recipe) -> bool {
        recipe
            .inputs
            .iter()
            .chain(&recipe.outputs)
            .all(|item| self.allows(actor, region, item.good))
    }
}
impl Property {
    pub fn index(&self) -> Index<'_> {
        Index {
            claims: self
                .claims
                .par_iter()
                .map(|claim| {
                    let delegates = if claim.delegates.par_windows(2).any(|pair| pair[0] > pair[1])
                    {
                        let mut rows = claim.delegates.clone();
                        rows.par_sort_unstable();
                        Cow::Owned(rows)
                    } else {
                        Cow::Borrowed(claim.delegates.as_slice())
                    };
                    let dense = delegates.first().and_then(|&first| {
                        delegates.last().and_then(|&last| {
                            (last.checked_sub(first) == Some((delegates.len() - 1) as u64))
                                .then_some(first)
                        })
                    });
                    (
                        (claim.region, claim.good),
                        Entry {
                            owner: claim.owner,
                            delegates,
                            dense,
                        },
                    )
                })
                .collect(),
        }
    }
    pub fn validate(
        &self,
        people: &HashSet<u64>,
        inventories: &[Inventory],
        now: u64,
    ) -> Result<(), String> {
        let stock: HashSet<_> = inventories
            .iter()
            .flat_map(|inventory| {
                inventory
                    .stocks
                    .iter()
                    .map(move |s| (inventory.region, s.good))
            })
            .collect();
        let mut claims = HashSet::new();
        if self.claims.iter().any(|claim| {
            let mut delegates = HashSet::new();
            !stock.contains(&(claim.region, claim.good))
                || !claims.insert((claim.region, claim.good))
                || !people.contains(&claim.owner)
                || claim.delegates.iter().any(|person| {
                    *person == claim.owner || !people.contains(person) || !delegates.insert(*person)
                })
        }) {
            return Err("invalid property claim".into());
        }
        if self.thefts.par_iter().any(|theft| {
            theft.tick > now
                || theft.actor == theft.owner
                || theft.quantity == 0
                || !people.contains(&theft.actor)
                || !people.contains(&theft.owner)
                || !stock.contains(&(theft.region, theft.good))
        }) {
            return Err("invalid theft record".into());
        }
        if self.estates.par_iter().any(|event| {
            event.tick > now
                || event.previous_owner == event.heir
                || !people.contains(&event.previous_owner)
                || !people.contains(&event.heir)
                || !stock.contains(&(event.region, event.good))
        }) {
            return Err("invalid estate record".into());
        }
        let mut offers = HashSet::new();
        if self.offers.iter().any(|offer| {
            offer.id >= self.next_id
                || !offers.insert(offer.id)
                || offer.proposer == offer.recipient
                || !people.contains(&offer.proposer)
                || !people.contains(&offer.recipient)
                || !claims.contains(&(offer.region, offer.good))
                || offer.proposed_at > now
        }) {
            return Err("invalid property offer".into());
        }
        Ok(())
    }
    pub fn check_jobs(&self, recipes: &[Recipe], requests: &[Start]) -> Result<(), String> {
        let index = self.index();
        let recipes: HashMap<_, _> = recipes.iter().map(|recipe| (recipe.id, recipe)).collect();
        if requests.par_iter().any(|request| {
            recipes
                .get(&request.recipe)
                .is_some_and(|recipe| !index.allows_recipe(request.worker, request.region, recipe))
        }) {
            return Err("stock owner has not authorized this work".into());
        }
        Ok(())
    }
    pub fn check_transfers(
        &self,
        people: &[PersonSnapshot],
        requests: &[Transfer],
    ) -> Result<(), String> {
        let index = self.index();
        if requests.par_iter().any(|request| {
            person_index(people, request.person)
                .and_then(|i| people[i].1)
                .is_some_and(|region| !index.allows(request.person, region, request.good))
        }) {
            return Err("stock owner has not authorized this transfer".into());
        }
        Ok(())
    }
    pub fn check_recovery(
        &self,
        people: &[PersonSnapshot],
        actor: u64,
        good: u32,
    ) -> Result<(), String> {
        if person_index(people, actor)
            .and_then(|i| people[i].1)
            .is_some_and(|region| !self.index().allows(actor, region, good))
        {
            return Err("stock owner has not authorized this transfer".into());
        }
        Ok(())
    }
    fn local(people: &[PersonSnapshot], person: u64, region: u64) -> bool {
        person_index(people, person).is_some_and(|i| people[i].1 == Some(region) && people[i].2 > 0)
    }
    pub fn offer(
        &mut self,
        people: &[PersonSnapshot],
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
        now: u64,
    ) -> Result<u64, String> {
        if owner == recipient
            || !Self::local(people, owner, region)
            || !Self::local(people, recipient, region)
        {
            return Err("participants must be living and local".into());
        }
        if !self
            .claims
            .iter()
            .any(|claim| claim.region == region && claim.good == good && claim.owner == owner)
        {
            return Err("proposer does not own the stock".into());
        }
        let id = self.next_id;
        let next = id.checked_add(1).ok_or("property offer ID overflow")?;
        self.offers.push(Offer {
            id,
            region,
            good,
            proposer: owner,
            recipient,
            proposed_at: now,
        });
        self.next_id = next;
        Ok(id)
    }
    pub fn decide(
        &mut self,
        people: &[PersonSnapshot],
        id: u64,
        actor: u64,
        accept: bool,
    ) -> Result<(), String> {
        let position = self
            .offers
            .iter()
            .position(|offer| offer.id == id)
            .ok_or("unknown property offer")?;
        let offer = self.offers[position].clone();
        if offer.recipient != actor || !Self::local(people, actor, offer.region) {
            return Err("recipient must be living and local".into());
        }
        if accept {
            if !Self::local(people, offer.proposer, offer.region) {
                return Err("proposer must be living and local".into());
            }
            let claim = self
                .claims
                .iter()
                .position(|claim| {
                    claim.region == offer.region
                        && claim.good == offer.good
                        && claim.owner == offer.proposer
                })
                .ok_or("property offer is stale")?;
            self.claims[claim].owner = actor;
            self.claims[claim].delegates.clear();
            // Other proposals for the same indivisible title cannot survive its transfer.
            self.offers
                .retain(|other| (other.region, other.good) != (offer.region, offer.good));
        } else {
            self.offers.remove(position);
        }
        Ok(())
    }
    pub fn grant(
        &mut self,
        people: &[PersonSnapshot],
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    ) -> Result<(), String> {
        if owner == recipient
            || !Self::local(people, owner, region)
            || !Self::local(people, recipient, region)
        {
            return Err("participants must be living and local".into());
        }
        let claim = self
            .claims
            .iter_mut()
            .find(|claim| claim.region == region && claim.good == good && claim.owner == owner)
            .ok_or("actor does not own the stock")?;
        if !claim.delegates.contains(&recipient) {
            claim.delegates.push(recipient);
            claim.delegates.sort_unstable();
        }
        Ok(())
    }
    pub fn revoke(
        &mut self,
        people: &[PersonSnapshot],
        owner: u64,
        recipient: u64,
        region: u64,
        good: u32,
    ) -> Result<(), String> {
        if !Self::local(people, owner, region) {
            return Err("owner must be living and local".into());
        }
        let claim = self
            .claims
            .iter_mut()
            .find(|claim| claim.region == region && claim.good == good && claim.owner == owner)
            .ok_or("actor does not own the stock")?;
        let index = claim
            .delegates
            .iter()
            .position(|person| *person == recipient)
            .ok_or("person has no delegated access")?;
        claim.delegates.remove(index);
        Ok(())
    }
    pub fn withdraw(
        &mut self,
        people: &[PersonSnapshot],
        id: u64,
        actor: u64,
    ) -> Result<(), String> {
        let index = self
            .offers
            .iter()
            .position(|offer| offer.id == id)
            .ok_or("unknown property offer")?;
        let offer = &self.offers[index];
        if offer.proposer != actor || !Self::local(people, actor, offer.region) {
            return Err("proposer must be living and local".into());
        }
        self.offers.remove(index);
        Ok(())
    }
}
