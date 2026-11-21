//! Personal itineraries driven by accepted travel, observations, and elapsed time.
use crate::{DeliveryPreference, Missions, Objective};
use game_economy_person::{Person, travel};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Stage {
    Ready,
    Outbound,
    Visiting { until_millis: u128 },
    ReadyToReturn,
    Returning,
    Completed,
    Abandoned,
    Died,
}

impl Stage {
    fn active(self) -> bool {
        !matches!(self, Self::Completed | Self::Abandoned | Self::Died)
    }

    pub fn at_destination(self) -> bool {
        matches!(self, Self::Visiting { .. } | Self::ReadyToReturn)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Itinerary {
    pub id: u64,
    pub person: u64,
    pub origin: u64,
    pub destination: u64,
    pub visit_millis: u64,
    pub go_on: bool,
    pub head_home: bool,
    pub stage: Stage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<Objective>,
    #[serde(default)]
    pub objective_met: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_preference: Option<DeliveryPreference>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Planned,
    OutboundLeg,
    ReturnLeg,
    RouteUnavailable,
    ReachedDestination,
    VisitElapsed,
    ReachedOrigin,
    Abandoned,
    Died,
    ObjectiveMet,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub at_millis: u128,
    pub itinerary: u64,
    pub person: u64,
    pub region: u64,
    pub kind: EventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub food_observed: Option<u64>,
}

#[derive(Clone, Copy)]
pub struct Plan {
    pub person: u64,
    pub origin: u64,
    pub destination: u64,
    pub visit_millis: u64,
    pub head_home: bool,
    pub objective: Option<Objective>,
}

#[derive(Clone, Copy)]
pub struct Delivery {
    pub traveler: u64,
    pub receiver: u64,
    pub region: u64,
    pub good: u32,
    pub quantity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expeditions {
    pub next_id: u64,
    pub itineraries: Vec<Itinerary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invitation_pressure: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_events: Vec<Event>,
}

pub struct TravelContext<'a> {
    pub people: &'a [Person],
    pub travel: &'a travel::Travel,
    pub busy: &'a [u64],
    pub review: travel::RouteReview,
    pub controlled: Option<u64>,
}

impl Itinerary {
    fn event(&self, at_millis: u128, region: u64, kind: EventKind) -> Event {
        Event {
            at_millis,
            itinerary: self.id,
            person: self.person,
            region,
            kind,
            food_observed: None,
        }
    }

    fn route_target(&self) -> Option<u64> {
        match self.stage {
            Stage::Ready | Stage::Outbound if self.go_on => Some(self.destination),
            Stage::ReadyToReturn | Stage::Returning if self.head_home => Some(self.origin),
            _ => None,
        }
    }
}

impl Expeditions {
    pub fn from_fresh(owner: Missions, unit_millis: u64) -> Result<Self, String> {
        if !owner.events.is_empty() {
            return Err("fresh expeditions cannot contain past events".into());
        }
        let mut itineraries: Vec<_> = owner
            .entries
            .into_par_iter()
            .map(|row| {
                if row.phase != crate::MissionPhase::Ready
                    || row.objective_met
                    || row.party.is_some()
                {
                    return Err("fresh expedition requires an individual plan before departure");
                }
                let visit_millis = row
                    .visit_ticks
                    .checked_mul(unit_millis)
                    .ok_or("visit duration overflow")?;
                Ok(Itinerary {
                    id: row.id,
                    person: row.person,
                    origin: row.origin,
                    destination: row.destination,
                    visit_millis,
                    go_on: true,
                    head_home: true,
                    stage: Stage::Ready,
                    objective: row.objective,
                    objective_met: false,
                    delivery_preference: row.delivery_preference,
                })
            })
            .collect::<Result<_, _>>()?;
        itineraries.par_sort_unstable_by_key(|row| row.id);
        Ok(Self {
            next_id: owner.next_id,
            itineraries,
            invitation_pressure: owner.pressure_threshold,
            pending_events: Vec::new(),
        })
    }

    pub fn validate(
        &self,
        people: &HashSet<u64>,
        regions: &HashSet<u64>,
        goods: &HashSet<u32>,
        now: u128,
    ) -> Result<(), String> {
        let mut active: Vec<_> = self
            .itineraries
            .par_iter()
            .filter(|row| row.stage.active())
            .map(|row| row.person)
            .collect();
        active.par_sort_unstable();
        if self.next_id == 0
            || self.invitation_pressure == Some(0)
            || self.pending_events.par_iter().any(|event| {
                event.at_millis > now || self.itineraries.binary_search_by_key(&event.itinerary, |row| row.id)
                    .ok().is_none_or(|index| self.itineraries[index].person != event.person)
            })
            || active.par_windows(2).any(|rows| rows[0] == rows[1])
            || self.itineraries.par_windows(2).any(|rows| rows[0].id >= rows[1].id)
            || self.itineraries.par_iter().any(|row| {
                row.id == 0
                    || row.id >= self.next_id
                    || !people.contains(&row.person)
                    || row.visit_millis == 0
                    || row.origin == row.destination
                    || !regions.contains(&row.origin)
                    || !regions.contains(&row.destination)
                    || row.delivery_preference.is_some_and(|preference| {
                        !matches!(row.objective, Some(Objective::Deliver { .. }))
                            || preference.pressure_threshold == Some(0)
                            || preference.acquisition.is_some_and(|choice| choice.pressure_threshold == Some(0))
                    })
                    || matches!(row.stage, Stage::Visiting { until_millis } if until_millis < now || until_millis - now > u128::from(row.visit_millis))
                    || match row.objective {
                        None => row.objective_met,
                        Some(Objective::ObserveFood) => false,
                        Some(Objective::Meet { person }) => person == row.person || !people.contains(&person),
                        Some(Objective::Deliver { person, good, quantity, delivered }) => {
                            person == row.person || !people.contains(&person) || !goods.contains(&good)
                                || quantity == 0 || delivered > quantity || (delivered == quantity) != row.objective_met
                        }
                    }
            })
        {
            return Err("invalid elapsed expeditions".into());
        }
        Ok(())
    }

    pub fn active(&self, person: u64) -> Option<&Itinerary> {
        self.itineraries
            .iter()
            .find(|row| row.person == person && row.stage.active())
    }

    pub fn start(&mut self, plan: Plan, now: u128) -> Result<Event, String> {
        if plan.origin == plan.destination
            || plan.visit_millis == 0
            || self.active(plan.person).is_some()
        {
            return Err("an expedition requires a different destination, a positive visit, and an available itinerary".into());
        }
        let id = self.next_id;
        self.next_id = id.checked_add(1).ok_or("expedition ID overflow")?;
        let itinerary = Itinerary {
            id,
            person: plan.person,
            origin: plan.origin,
            destination: plan.destination,
            visit_millis: plan.visit_millis,
            go_on: true,
            head_home: plan.head_home,
            stage: Stage::Ready,
            objective: plan.objective,
            objective_met: false,
            delivery_preference: None,
        };
        let event = itinerary.event(now, plan.origin, EventKind::Planned);
        self.itineraries.push(itinerary);
        Ok(event)
    }

    pub fn abandon(&mut self, person: u64, region: u64, now: u128) -> Result<Event, String> {
        let row = self
            .itineraries
            .iter_mut()
            .find(|row| row.person == person && row.stage.active())
            .ok_or("no active itinerary")?;
        row.stage = Stage::Abandoned;
        Ok(row.event(now, region, EventKind::Abandoned))
    }

    pub fn choose_return(&mut self, person: u64) -> Result<(), String> {
        let row = self
            .itineraries
            .iter_mut()
            .find(|row| row.person == person && row.stage.active())
            .ok_or("no active itinerary")?;
        if row.stage.at_destination() {
            row.stage = Stage::ReadyToReturn;
        } else if row.stage != Stage::Returning {
            return Err("the itinerary has not reached its destination".into());
        }
        row.head_home = true;
        Ok(())
    }

    pub fn next_visit_deadline(&self) -> Option<u128> {
        self.itineraries
            .par_iter()
            .filter_map(|row| match row.stage {
                Stage::Visiting { until_millis } => Some(until_millis),
                _ => None,
            })
            .min()
    }

    pub fn visit_deadlines(&mut self, now: u128) -> Vec<Event> {
        self.itineraries
            .par_iter_mut()
            .filter_map(|row| {
                if !matches!(row.stage, Stage::Visiting { until_millis } if now >= until_millis) {
                    return None;
                }
                row.stage = Stage::ReadyToReturn;
                Some(row.event(now, row.destination, EventKind::VisitElapsed))
            })
            .collect()
    }

    /// One route proposal per actor. In worlds with personal routing, use only
    /// the actor's reports. Reports may be wrong when physical travel is tried.
    pub fn departures(&self, context: TravelContext<'_>) -> Vec<travel::Departure> {
        let TravelContext {
            people,
            travel,
            busy,
            review,
            controlled,
        } = context;
        let mut targets: Vec<_> = self
            .itineraries
            .par_iter()
            .filter_map(Itinerary::route_target)
            .collect();
        if targets.is_empty() {
            return Vec::new();
        }
        targets.par_sort_unstable();
        targets.dedup();
        let familiar = (!travel.personal_routing).then(|| travel::routes(travel, &targets));
        let mut reports: Vec<_> = if travel.personal_routing {
            travel.observations.par_iter().collect()
        } else {
            Vec::new()
        };
        reports.par_sort_unstable_by_key(|row| row.person);
        let mut requests: Vec<_> = self
            .itineraries
            .par_iter()
            .filter_map(|row| {
                let target = row.route_target()?;
                if controlled.is_some_and(|actor| actor != row.person)
                    || busy.binary_search(&row.person).is_ok()
                {
                    return None;
                }
                let person = &people[people
                    .binary_search_by_key(&row.person, |person| person.id)
                    .ok()?];
                let source = person.region?;
                if person.health == 0
                    || (person.controlled && controlled.is_none())
                    || source == target
                {
                    return None;
                }
                let first_leg = if let Some(routes) = &familiar {
                    routes.get(&(source, target))?.1
                } else {
                    let a = reports.partition_point(|report| report.person < row.person);
                    let b = reports.partition_point(|report| report.person <= row.person);
                    let paths = travel::known_paths_at(&reports[a..b], source, review);
                    paths.get(&target)?.1
                };
                Some(travel::Departure {
                    person: row.person,
                    destination: first_leg,
                })
            })
            .collect();
        requests.par_sort_unstable_by_key(|request| request.person);
        requests
    }

    pub fn record_departures(
        &mut self,
        requests: &[travel::Departure],
        outcomes: &[travel::DepartureOutcome],
        now: u128,
    ) -> Vec<Event> {
        self.itineraries
            .par_iter_mut()
            .filter_map(|row| {
                row.route_target()?;
                let request = requests.get(
                    requests
                        .binary_search_by_key(&row.person, |request| request.person)
                        .ok()?,
                )?;
                let accepted = outcomes[outcomes
                    .binary_search_by_key(&row.person, |outcome| outcome.person)
                    .ok()?]
                .departed;
                let kind = if !accepted {
                    EventKind::RouteUnavailable
                } else if matches!(row.stage, Stage::Ready | Stage::Outbound) {
                    row.stage = Stage::Outbound;
                    EventKind::OutboundLeg
                } else {
                    row.stage = Stage::Returning;
                    EventKind::ReturnLeg
                };
                Some(row.event(now, request.destination, kind))
            })
            .collect()
    }

    pub fn locations(&mut self, people: &[Person], now: u128) -> Vec<Event> {
        self.itineraries
            .par_iter_mut()
            .filter_map(|row| {
                if !row.stage.active() {
                    return None;
                }
                let person = &people[people
                    .binary_search_by_key(&row.person, |person| person.id)
                    .ok()?];
                if person.health == 0 && person.immortal.is_none() {
                    row.stage = Stage::Died;
                    return Some(row.event(now, person.region.unwrap_or(0), EventKind::Died));
                }
                let region = person.region?;
                let kind = match row.stage {
                    Stage::Ready | Stage::Outbound if region == row.destination => {
                        row.stage = Stage::Visiting {
                            until_millis: now.saturating_add(u128::from(row.visit_millis)),
                        };
                        EventKind::ReachedDestination
                    }
                    Stage::ReadyToReturn | Stage::Returning if region == row.origin => {
                        row.stage = Stage::Completed;
                        EventKind::ReachedOrigin
                    }
                    _ => return None,
                };
                Some(row.event(now, region, kind))
            })
            .collect()
    }

    /// Completion requires the traveler's own current observation.
    pub fn observed_food(&mut self, observations: &[(u64, u64, u64)], now: u128) -> Vec<Event> {
        self.itineraries
            .par_iter_mut()
            .filter_map(|row| {
                if row.objective_met
                    || !row.stage.at_destination()
                    || row.objective != Some(Objective::ObserveFood)
                {
                    return None;
                }
                let index = observations
                    .binary_search_by_key(&(row.person, row.destination), |&(person, region, _)| {
                        (person, region)
                    })
                    .ok()?;
                row.objective_met = true;
                let mut event = row.event(now, row.destination, EventKind::ObjectiveMet);
                event.food_observed = Some(observations[index].2);
                Some(event)
            })
            .collect()
    }

    pub fn met_people(&mut self, contacts: &[(u64, u64, u64)], now: u128) -> Vec<Event> {
        self.itineraries
            .par_iter_mut()
            .filter_map(|row| {
                let Some(Objective::Meet { person }) = row.objective else {
                    return None;
                };
                if row.objective_met
                    || !row.stage.at_destination()
                    || contacts
                        .binary_search(&(row.person, person, row.destination))
                        .is_err()
                {
                    return None;
                }
                row.objective_met = true;
                Some(row.event(now, row.destination, EventKind::ObjectiveMet))
            })
            .collect()
    }

    pub fn deliveries(&mut self, accepted: &[Delivery], now: u128) -> Vec<Event> {
        self.itineraries
            .par_iter_mut()
            .filter_map(|row| {
                let Some(Objective::Deliver {
                    person,
                    good,
                    quantity,
                    delivered,
                }) = row.objective
                else {
                    return None;
                };
                if row.objective_met || !row.stage.at_destination() {
                    return None;
                }
                let start = accepted.partition_point(|receipt| receipt.traveler < row.person);
                let end = accepted.partition_point(|receipt| receipt.traveler <= row.person);
                let received = accepted[start..end]
                    .iter()
                    .filter(|receipt| {
                        receipt.receiver == person
                            && receipt.region == row.destination
                            && receipt.good == good
                    })
                    .map(|receipt| receipt.quantity)
                    .fold(delivered, u64::saturating_add)
                    .min(quantity);
                if received == delivered {
                    return None;
                }
                row.objective = Some(Objective::Deliver {
                    person,
                    good,
                    quantity,
                    delivered: received,
                });
                row.objective_met = received == quantity;
                row.objective_met
                    .then(|| row.event(now, row.destination, EventKind::ObjectiveMet))
            })
            .collect()
    }
}
