//! Present travelers choose how to answer another person's planned expedition.
use super::*;
use game_economy_expeditions::{
    elapsed::Stage,
    elapsed_invitations::{Invitation, Offer, Reply, Response},
};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoreExpeditionOffer {
    pub invitation: Invitation,
    pub response: Option<Reply>,
}

#[derive(Clone, Copy)]
struct Proposal {
    offer: Offer,
    reply: Option<(Response, bool)>,
    rank: (u8, std::cmp::Reverse<i32>, u64, u64),
}

impl Proposal {
    fn admit(self, best: &mut HashMap<u64, Self>) {
        let winner = best.entry(self.offer.recipient).or_insert(self);
        if self.rank < winner.rank {
            *winner = self;
        }
    }
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
            depart_after_millis: trip.depart_after_millis,
            rendezvous_group: trip.rendezvous.map(|choice| choice.group),
        };
        let response = if beneficiary.controlled {
            None
        } else {
            Some(self.decide_expedition_invitation(
                &invite,
                self.expedition(target).is_some(),
                self.opinions.as_ref().map(|owner| owner.index()).as_ref(),
            ))
        };
        let now = self.core.simulated_millis();
        let mut next = self.expeditions.clone().expect("itinerary checked");
        let invitation = next.invitations.offer(invite, now)?;
        let reply = response
            .map(|(choice, head_home)| {
                let joined = if matches!(choice, Response::Joined | Response::JoinedUnderPressure) {
                    let itinerary = next.start(invitation.personal_plan(region, head_home), now)?.itinerary;
                    let rendezvous = next.invitations.preference(target, invitation.destination)
                        .and_then(|choice| invitation.personal_rendezvous(choice, now));
                    if rendezvous.is_some() {
                        let event = next.set_rendezvous(target, rendezvous, now)?;
                        next.pending_events.push(event);
                    }
                    Some(itinerary)
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

    pub(super) fn offer_autonomous_expeditions(
        &mut self,
        busy: &[u64],
    ) -> Result<Vec<ExpeditionEvent>, String> {
        let Some(owner) = self
            .expeditions
            .as_ref()
            .filter(|owner| !owner.invitations.initiatives.is_empty())
        else {
            return Ok(Vec::new());
        };
        let active: HashMap<_, _> = owner
            .itineraries
            .par_iter()
            .filter(|row| row.stage.active())
            .map(|row| (row.person, row))
            .collect();
        let pending = owner.invitations.pending_pairs();
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let affinity = |observer, target| {
            opinions
                .as_ref()
                .map_or(0, |row| row.affinity(observer, target))
        };
        let local = |id| {
            let person = self
                .core
                .people
                .binary_search_by_key(&id, |row| row.id)
                .ok()
                .map(|i| &self.core.people[i])?;
            (person.health > 0 && busy.binary_search(&id).is_err()).then_some(person)
        };
        let seed = self.core.seed ^ self.core.decision_time_key();
        let winners = owner
            .invitations
            .initiatives
            .par_iter()
            .filter_map(|profile| {
                let actor = local(profile.person)?;
                let trip = *active.get(&actor.id)?;
                if actor.controlled
                    || actor.region != Some(trip.origin)
                    || trip.stage != Stage::Ready
                    || !trip.go_on
                    || !owner.invitations.may_offer(actor.id, trip.id)
                {
                    return None;
                }
                let recipient = profile
                    .companions
                    .iter()
                    .filter_map(|choice| {
                        let peer = local(choice.person)?;
                        let value = choice.value.saturating_add(
                            i64::from(choice.affinity_weight)
                                * i64::from(affinity(actor.id, peer.id)),
                        );
                        (value > profile.idle
                            && peer.region == actor.region
                            && !pending.contains(&(trip.id, peer.id))
                            && self.same_local_endpoint(actor.id, peer.id))
                        .then_some((value, game_genetics::random(seed, actor.id, peer.id), peer))
                    })
                    .max_by_key(|row| (row.0, row.1, row.2.id))?
                    .2;
                let invite = Offer {
                    inviter: actor.id,
                    recipient: recipient.id,
                    itinerary: trip.id,
                    origin: trip.origin,
                    destination: trip.destination,
                    visit_millis: trip.visit_millis,
                    depart_after_millis: trip.depart_after_millis,
                    rendezvous_group: trip.rendezvous.map(|choice| choice.group),
                };
                let reply = (!recipient.controlled).then(|| {
                    self.decide_expedition_invitation(
                        &invite,
                        active.contains_key(&recipient.id),
                        opinions.as_ref(),
                    )
                });
                let response_priority = match reply {
                    None | Some((Response::Joined, _)) => 0,
                    Some((Response::JoinedUnderPressure, _)) => 1,
                    _ => 2,
                };
                Some(Proposal {
                    offer: invite,
                    reply,
                    rank: (
                        response_priority,
                        std::cmp::Reverse(affinity(recipient.id, actor.id)),
                        game_genetics::random(seed, recipient.id, actor.id),
                        actor.id,
                    ),
                })
            })
            .fold(HashMap::new, |mut best, proposal| {
                proposal.admit(&mut best);
                best
            })
            .reduce(HashMap::new, |mut left, mut right| {
                if left.len() < right.len() {
                    std::mem::swap(&mut left, &mut right);
                }
                for proposal in right.into_values() {
                    proposal.admit(&mut left);
                }
                left
            });
        let mut batch: Vec<_> = winners
            .into_values()
            .map(|row| (row.offer, row.reply))
            .collect();
        batch.par_sort_unstable_by_key(|row| row.0.recipient);
        let now = self.core.simulated_millis();
        self.expeditions
            .as_mut()
            .expect("expeditions checked")
            .offer_separate_trips(&batch, now)
    }

    fn decide_expedition_invitation(
        &self,
        offer: &Offer,
        has_itinerary: bool,
        opinions: Option<&game_opinions::Index<'_>>,
    ) -> (Response, bool) {
        if has_itinerary {
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
            .zip(opinions)
            .is_some_and(|(threshold, opinions)| {
                let (fear, duty) = opinions.feelings(offer.recipient, offer.inviter);
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
