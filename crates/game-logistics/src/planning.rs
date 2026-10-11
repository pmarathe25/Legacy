//! Compose personal acquisition facts without letting providers choose a step.
use crate::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ActivityClass {
    Idle,
    Rest,
    Work,
    #[default]
    Other,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Readiness {
    pub actor_present: bool,
    pub body_present: bool,
    pub mind_present: bool,
    pub alive: bool,
    pub functional: bool,
    pub independent: bool,
    pub restricted: bool,
    pub in_combat: bool,
    pub dependent_care: bool,
    pub activity: ActivityClass,
    pub carried: bool,
    pub health_busy: bool,
    pub property_talk: bool,
    pub army_talk: bool,
    pub realm_busy: bool,
    pub mission_talk: bool,
    pub social_busy: bool,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub can_depart: bool,
}
impl Readiness {
    pub fn usable(self) -> bool {
        self.alive
            && self.functional
            && self.independent
            && !self.restricted
            && !self.in_combat
            && !self.dependent_care
    }
    pub fn free(self) -> bool {
        self.usable()
            && matches!(
                self.activity,
                ActivityClass::Idle | ActivityClass::Rest | ActivityClass::Work
            )
            && !self.carried
            && !self.health_busy
            && !self.property_talk
            && !self.army_talk
            && !self.realm_busy
            && !self.mission_talk
            && !self.social_busy
    }
    pub fn can_travel(self) -> bool {
        self.usable() && self.fatigue < self.fatigue_limit && self.can_depart
    }
}
/// A provider's capability is separate from the character's own judgment.
pub struct Candidate<A> {
    pub offer: AcquisitionOffer<A>,
    pub willing: bool,
}
/// Each call supplies feature-owned prospects for precisely this demand.
/// It cannot inspect the delivery and cannot select an action or fallback.
pub trait AcquisitionSource<A> {
    fn offers(&self, good: Good, quantity: Quantity, room: Quantity) -> Vec<Candidate<A>>;
}
/// Transient facts, never another stock ledger or a copy of personal state.
pub struct DeliveryFacts<'a, A> {
    pub now: SimTime,
    pub place: Location,
    pub readiness: Readiness,
    pub room: Quantity,
    pub cargo: Vec<Cargo>,
    pub stocks: Vec<KnownStock>,
    pub contacts: BTreeSet<PersonId>,
    pub routes: &'a KnownRoutes,
    pub destination: Option<KnownDestination>,
    pub direct_transfer: bool,
    pub additional: Vec<Candidate<A>>,
}
impl Delivery {
    pub fn plan_with_acquisition<A: Clone>(
        &self,
        facts: DeliveryFacts<'_, A>,
        minimum_handoff: Quantity,
        source: &impl AcquisitionSource<A>,
    ) -> Step<A> {
        if !facts.readiness.actor_present || !facts.readiness.body_present {
            return Step::Blocked(BlockReason::CannotTravel);
        }
        if !facts.readiness.mind_present {
            return Step::Blocked(BlockReason::UnknownRoute);
        }
        let wanted = Quantity(self.remaining.0.max(minimum_handoff.0).min(facts.room.0));
        let mut requested = BTreeMap::from([(self.requirement.good, wanted)]);
        for prerequisite in facts.additional.iter().flat_map(|c| &c.offer.requirements) {
            requested
                .entry(prerequisite.requirement.good)
                .and_modify(|q| q.0 = q.0.max(prerequisite.requirement.quantity.0))
                .or_insert(prerequisite.requirement.quantity);
        }
        let mut projected = BTreeSet::new();
        let mut offers = facts.additional;
        while let Some((good, quantity)) = requested
            .iter()
            .find(|(good, _)| !projected.contains(*good))
            .map(|(g, q)| (*g, *q))
        {
            projected.insert(good);
            let owned = source.offers(good, quantity, facts.room);
            for prerequisite in owned.iter().flat_map(|c| &c.offer.requirements) {
                requested
                    .entry(prerequisite.requirement.good)
                    .and_modify(|q| q.0 = q.0.max(prerequisite.requirement.quantity.0))
                    .or_insert(prerequisite.requirement.quantity);
            }
            offers.extend(owned);
        }
        let offers: Vec<_> = offers
            .into_iter()
            .filter(|c| c.willing)
            .map(|c| c.offer)
            .collect();
        let prerequisite_room = requested.keys().map(|g| (*g, facts.room)).collect();
        let mut destination = facts.destination;
        if let Some(destination) = &mut destination {
            destination.direct_transfer = facts.direct_transfer;
        }
        self.plan(&Input {
            now: facts.now,
            place: &facts.place,
            free: facts.readiness.free(),
            can_travel: facts.readiness.can_travel(),
            room: facts.room,
            prerequisite_room: &prerequisite_room,
            minimum_handoff,
            cargo: &facts.cargo,
            stocks: &facts.stocks,
            destination: destination.as_ref(),
            contacts: &facts.contacts,
            routes: facts.routes,
            offers: &offers,
        })
    }
}
#[cfg(test)]
mod tests;

impl<A> DeliveryFacts<'_, A> {
    pub fn absent(actor_present: bool, body_present: bool, mind_present: bool) -> Self {
        static ROUTES: KnownRoutes = BTreeMap::new();
        Self {
            now: SimTime(0),
            place: Location::AtArea(AreaId(0)),
            readiness: Readiness {
                actor_present,
                body_present,
                mind_present,
                ..Readiness::default()
            },
            room: Quantity(0),
            cargo: Vec::new(),
            stocks: Vec::new(),
            contacts: BTreeSet::new(),
            routes: &ROUTES,
            destination: None,
            direct_transfer: false,
            additional: Vec::new(),
        }
    }
}
