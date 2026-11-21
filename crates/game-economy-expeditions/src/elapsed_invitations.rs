//! An invitation conveys a proposed itinerary between two people.
use crate::elapsed::Plan;
use crate::rendezvous::Rendezvous;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub id: u64,
    pub inviter: u64,
    pub recipient: u64,
    pub itinerary: u64,
    pub origin: u64,
    pub destination: u64,
    pub visit_millis: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depart_after_millis: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendezvous_group: Option<u64>,
    pub offered_at_millis: u128,
}

#[derive(Clone, Copy)]
pub struct Offer {
    pub inviter: u64,
    pub recipient: u64,
    pub itinerary: u64,
    pub origin: u64,
    pub destination: u64,
    pub visit_millis: u64,
    pub depart_after_millis: Option<u128>,
    pub rendezvous_group: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preference {
    pub person: u64,
    pub destination: u64,
    #[serde(default)]
    pub willing: bool,
    #[serde(default)]
    pub return_after_visit: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendezvous: Option<CompanionPreference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompanionPreference {
    pub minimum_companions: u32,
    pub wait_millis: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Companion {
    pub person: u64,
    pub value: i64,
    #[serde(default)]
    pub affinity_weight: i32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Initiative {
    pub person: u64,
    #[serde(default)]
    pub idle: i64,
    pub companions: Vec<Companion>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Declined,
    Joined,
    JoinedUnderPressure,
    Withdrawn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub invitation: Invitation,
    pub at_millis: u128,
    pub response: Response,
    pub joined_itinerary: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invitations {
    next_id: u64,
    pending: Vec<Invitation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preferences: Vec<Preference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initiatives: Vec<Initiative>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    attempted: BTreeMap<u64, u64>,
}

impl Default for Invitations {
    fn default() -> Self {
        Self {
            next_id: 1,
            pending: Vec::new(),
            preferences: Vec::new(),
            initiatives: Vec::new(),
            attempted: BTreeMap::new(),
        }
    }
}

impl Invitations {
    pub fn is_pristine(&self) -> bool {
        self.next_id == 1
            && self.pending.is_empty()
            && self.preferences.is_empty()
            && self.initiatives.is_empty()
            && self.attempted.is_empty()
    }

    pub fn preference(&self, person: u64, destination: u64) -> Option<Preference> {
        self.preferences
            .binary_search_by_key(&(person, destination), |row| (row.person, row.destination))
            .ok()
            .map(|index| self.preferences[index])
    }

    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        now: u128,
    ) -> Result<(), String> {
        if self.next_id == 0
            || self.preferences.par_windows(2).any(|pair| {
                (pair[0].person, pair[0].destination) >= (pair[1].person, pair[1].destination)
            })
            || self
                .preferences
                .par_iter()
                .any(|row| !people.contains(&row.person) || !regions.contains(&row.destination))
            || self
                .initiatives
                .par_windows(2)
                .any(|rows| rows[0].person >= rows[1].person)
            || self.initiatives.par_iter().any(|row| {
                !people.contains(&row.person)
                    || row.companions.len() > 8
                    || row
                        .companions
                        .windows(2)
                        .any(|pair| pair[0].person >= pair[1].person)
                    || row.companions.iter().any(|target| {
                        target.person == row.person || !people.contains(&target.person)
                    })
            })
            || self.attempted.iter().any(|(&person, &trip)| {
                trip == 0
                    || !people.contains(&person)
                    || self
                        .initiatives
                        .binary_search_by_key(&person, |row| row.person)
                        .is_err()
            })
            || self
                .pending
                .par_windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
            || self.pending.par_iter().any(|row| {
                row.id == 0
                    || row.id >= self.next_id
                    || row.itinerary == 0
                    || row.inviter == row.recipient
                    || row.origin == row.destination
                    || row.visit_millis == 0
                    || row.rendezvous_group == Some(0)
                    || row.offered_at_millis > now
                    || !people.contains(&row.inviter)
                    || !people.contains(&row.recipient)
                    || !regions.contains(&row.origin)
                    || !regions.contains(&row.destination)
            })
        {
            return Err("invalid expedition invitations".into());
        }
        let mut pairs: Vec<_> = self
            .pending
            .par_iter()
            .map(|row| (row.itinerary, row.recipient))
            .collect();
        pairs.par_sort_unstable();
        if pairs.par_windows(2).any(|pair| pair[0] == pair[1]) {
            return Err("duplicate expedition invitations".into());
        }
        Ok(())
    }

    pub fn addressed_to(&self, person: u64) -> Vec<Invitation> {
        self.pending
            .par_iter()
            .filter(|row| row.recipient == person)
            .copied()
            .collect()
    }

    pub fn may_offer(&self, inviter: u64, itinerary: u64) -> bool {
        self.attempted
            .get(&inviter)
            .is_none_or(|&previous| previous != itinerary)
    }

    pub fn pending_pairs(&self) -> HashSet<(u64, u64)> {
        self.pending
            .par_iter()
            .map(|row| (row.itinerary, row.recipient))
            .collect()
    }

    pub(super) fn prepare_offers(
        &self,
        offers: &[(Offer, Option<(Response, bool)>)],
        now: u128,
    ) -> Result<Vec<Invitation>, String> {
        self.next_id
            .checked_add(offers.len() as u64)
            .ok_or("expedition invitation ID overflow")?;
        let pending = self.pending_pairs();
        offers
            .par_iter()
            .enumerate()
            .map(|(index, (offer, _))| {
                if offer.inviter == offer.recipient
                    || offer.origin == offer.destination
                    || offer.visit_millis == 0
                    || offer.rendezvous_group == Some(0)
                    || pending.contains(&(offer.itinerary, offer.recipient))
                    || !self.may_offer(offer.inviter, offer.itinerary)
                {
                    return Err("invalid or previously offered expedition invitation".into());
                }
                Ok(Invitation {
                    id: self.next_id + index as u64,
                    inviter: offer.inviter,
                    recipient: offer.recipient,
                    itinerary: offer.itinerary,
                    origin: offer.origin,
                    destination: offer.destination,
                    visit_millis: offer.visit_millis,
                    depart_after_millis: offer.depart_after_millis,
                    rendezvous_group: offer.rendezvous_group,
                    offered_at_millis: now,
                })
            })
            .collect()
    }

    pub(super) fn commit_offers(&mut self, invitations: &[Invitation], pending: &[Invitation]) {
        self.next_id += invitations.len() as u64;
        self.pending.extend_from_slice(pending);
        for row in invitations {
            self.attempted.insert(row.inviter, row.itinerary);
        }
    }

    pub fn offer(&mut self, offer: Offer, now: u128) -> Result<Invitation, String> {
        if offer.inviter == offer.recipient
            || offer.origin == offer.destination
            || offer.visit_millis == 0
            || offer.rendezvous_group == Some(0)
            || self
                .pending
                .par_iter()
                .any(|row| row.itinerary == offer.itinerary && row.recipient == offer.recipient)
        {
            return Err("invalid or previously offered expedition invitation".into());
        }
        let id = self.next_id;
        let next_id = id
            .checked_add(1)
            .ok_or("expedition invitation ID overflow")?;
        let invitation = Invitation {
            id,
            inviter: offer.inviter,
            recipient: offer.recipient,
            itinerary: offer.itinerary,
            origin: offer.origin,
            destination: offer.destination,
            visit_millis: offer.visit_millis,
            depart_after_millis: offer.depart_after_millis,
            rendezvous_group: offer.rendezvous_group,
            offered_at_millis: now,
        };
        self.pending.push(invitation);
        self.next_id = next_id;
        Ok(invitation)
    }

    pub fn get(&self, person: u64, id: u64) -> Result<Invitation, String> {
        let index = self
            .pending
            .binary_search_by_key(&id, |row| row.id)
            .map_err(|_| "unknown expedition invitation")?;
        let row = self.pending[index];
        if row.recipient != person && row.inviter != person {
            return Err("unknown expedition invitation".into());
        }
        Ok(row)
    }

    pub fn finish(
        &mut self,
        invitation: Invitation,
        response: Response,
        itinerary: Option<u64>,
        now: u128,
    ) -> Result<Reply, String> {
        let index = self
            .pending
            .binary_search_by_key(&invitation.id, |row| row.id)
            .map_err(|_| "expedition invitation is no longer pending")?;
        if self.pending[index] != invitation
            || matches!(response, Response::Joined | Response::JoinedUnderPressure)
                != itinerary.is_some()
        {
            return Err("expedition reply differs from the pending invitation".into());
        }
        self.pending.remove(index);
        Ok(Reply {
            invitation,
            at_millis: now,
            response,
            joined_itinerary: itinerary,
        })
    }
}

impl Invitation {
    /// Joining an itinerary and choosing to wait for its company are separate.
    pub fn personal_rendezvous(self, preference: Preference, now: u128) -> Option<Rendezvous> {
        let preference = preference.rendezvous?;
        let opening = self.depart_after_millis.unwrap_or(now).max(now);
        Some(Rendezvous {
            group: self.rendezvous_group?,
            minimum_companions: preference.minimum_companions,
            wait_until_millis: opening.saturating_add(u128::from(preference.wait_millis)),
        })
    }

    pub fn personal_plan(self, region: u64, head_home: bool) -> Plan {
        Plan {
            person: self.recipient,
            origin: region,
            destination: self.destination,
            visit_millis: self.visit_millis,
            depart_after_millis: self.depart_after_millis,
            head_home,
            objective: None,
        }
    }
}
