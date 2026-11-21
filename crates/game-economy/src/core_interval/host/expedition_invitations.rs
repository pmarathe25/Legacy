//! Present travelers choose how to answer another person's planned expedition.
use super::*;
use game_economy_expeditions::{
    elapsed::Stage,
    elapsed_invitations::{Invitation, Offer, Reply, Response},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreExpeditionOffer {
    pub invitation: Invitation,
    pub response: Option<Reply>,
}

impl CoreHost {
    pub fn expedition_invitations(&self, person: u64) -> Result<Vec<Invitation>, String> {
        self.controlled_person(person)?;
        Ok(self
            .expeditions
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.invitations.addressed_to(person)))
    }

    pub fn invite_controlled_expedition(
        &mut self,
        person: u64,
        target: u64,
    ) -> Result<CoreExpeditionOffer, String> {
        let actor = self.controlled_person(person)?;
        let region = actor
            .region
            .filter(|_| actor.health > 0)
            .ok_or("an invitation requires a living local sender")?;
        let beneficiary = self
            .core
            .people
            .binary_search_by_key(&target, |row| row.id)
            .ok()
            .map(|index| &self.core.people[index])
            .ok_or("unknown expedition invitee")?;
        let trip = self
            .expedition(person)
            .ok_or("there is no proposed personal itinerary")?;
        if person == target
            || trip.stage != Stage::Ready
            || trip.origin != region
            || beneficiary.health == 0
            || beneficiary.region != Some(region)
            || !self.same_local_endpoint(person, target)
        {
            return Err("an invitation requires a ready itinerary and a nearby recipient".into());
        }
        let invite = Offer {
            inviter: person,
            recipient: target,
            itinerary: trip.id,
            origin: region,
            destination: trip.destination,
            visit_millis: trip.visit_millis,
        };
        let response = if beneficiary.controlled {
            None
        } else {
            Some(self.decide_expedition_invitation(&invite))
        };
        let now = self.core.simulated_millis();
        let mut next = self.expeditions.clone().expect("itinerary checked");
        let invitation = next.invitations.offer(invite, now)?;
        let reply = response
            .map(|(choice, head_home)| {
                let joined = if matches!(choice, Response::Joined | Response::JoinedUnderPressure) {
                    Some(
                        next.start(invitation.personal_plan(region, head_home), now)?
                            .itinerary,
                    )
                } else {
                    None
                };
                next.invitations.finish(invitation, choice, joined, now)
            })
            .transpose()?;
        self.expeditions = Some(next);
        Ok(CoreExpeditionOffer {
            invitation,
            response: reply,
        })
    }

    fn decide_expedition_invitation(&self, offer: &Offer) -> (Response, bool) {
        if self.expedition(offer.recipient).is_some() {
            return (Response::Declined, false);
        }
        let owner = self.expeditions.as_ref().expect("itinerary checked");
        let preference = owner
            .invitations
            .preference(offer.recipient, offer.destination);
        let willingly = preference.map_or_else(
            || {
                self.core.travel.as_ref().is_some_and(|travel| {
                    travel
                        .destinations
                        .binary_search_by_key(&offer.recipient, |row| row.person)
                        .ok()
                        .is_some_and(|index| {
                            let choice = &travel.destinations[index];
                            choice.destination == offer.destination && choice.enabled
                        })
                })
            },
            |preference| preference.willing,
        );
        let head_home = preference.is_some_and(|row| row.return_after_visit);
        if willingly {
            return (Response::Joined, head_home);
        }
        let fear_or_duty = self
            .expeditions
            .as_ref()
            .and_then(|owner| owner.invitation_pressure)
            .zip(self.opinions.as_ref())
            .is_some_and(|(threshold, opinions)| {
                let (fear, duty) = opinions.index().feelings(offer.recipient, offer.inviter);
                fear.saturating_add(duty) >= threshold
            });
        let response = if fear_or_duty {
            Response::JoinedUnderPressure
        } else {
            Response::Declined
        };
        (response, head_home)
    }

    pub fn reply_controlled_expedition(
        &mut self,
        person: u64,
        id: u64,
        join: bool,
    ) -> Result<Reply, String> {
        let actor = self.controlled_person(person)?;
        let mut next = self.expeditions.clone().ok_or("expeditions are disabled")?;
        let invitation = next.invitations.get(person, id)?;
        if invitation.recipient != person {
            return Err("only the addressed person can reply".into());
        }
        let now = self.core.simulated_millis();
        let (response, joined) = if join {
            let region = actor
                .region
                .filter(|_| actor.health > 0)
                .ok_or("starting a personal itinerary requires a living local traveler")?;
            let event = next.start(invitation.personal_plan(region, false), now)?;
            (Response::Joined, Some(event.itinerary))
        } else {
            (Response::Declined, None)
        };
        let reply = next.invitations.finish(invitation, response, joined, now)?;
        self.expeditions = Some(next);
        Ok(reply)
    }

    pub fn withdraw_controlled_expedition_invitation(
        &mut self,
        person: u64,
        id: u64,
    ) -> Result<Reply, String> {
        self.controlled_person(person)?;
        let now = self.core.simulated_millis();
        let owner = self
            .expeditions
            .as_mut()
            .ok_or("expeditions are disabled")?;
        let invitation = owner.invitations.get(person, id)?;
        if invitation.inviter != person {
            return Err("only the sender can withdraw this invitation".into());
        }
        owner
            .invitations
            .finish(invitation, Response::Withdrawn, None, now)
    }
}
