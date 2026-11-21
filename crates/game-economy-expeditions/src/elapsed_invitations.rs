//! An invitation conveys a proposed itinerary between two people.
use crate::elapsed::Plan;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
}

impl Default for Invitations {
    fn default() -> Self {
        Self {
            next_id: 1,
            pending: Vec::new(),
            preferences: Vec::new(),
        }
    }
}

impl Invitations {
    pub fn is_pristine(&self) -> bool {
        self.next_id == 1 && self.pending.is_empty() && self.preferences.is_empty()
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

    pub fn offer(&mut self, offer: Offer, now: u128) -> Result<Invitation, String> {
        if offer.inviter == offer.recipient
            || offer.origin == offer.destination
            || offer.visit_millis == 0
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
    pub fn personal_plan(self, region: u64, head_home: bool) -> Plan {
        Plan {
            person: self.recipient,
            origin: region,
            destination: self.destination,
            visit_millis: self.visit_millis,
            head_home,
            objective: None,
        }
    }
}
