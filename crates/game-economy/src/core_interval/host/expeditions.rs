//! Actor-owned plans over the same physical travel as the rest of the world.
use super::*;
use game_economy_expeditions::{
    Objective,
    elapsed::{Delivery, Event, Itinerary, Plan, TravelContext},
    rendezvous::Rendezvous,
};
use game_lots::Place;

impl CoreHost {
    pub(super) fn next_expedition_deadline(&self) -> Option<u64> {
        let next = self.expeditions.as_ref()?.next_visit_deadline()?;
        u64::try_from(next.saturating_sub(self.core.simulated_millis())).ok()
    }

    pub(super) fn expedition_time_events(&mut self) -> Vec<Event> {
        let Some(owner) = &mut self.expeditions else {
            return Vec::new();
        };
        let now = self.core.simulated_millis();
        let mut events = std::mem::take(&mut owner.pending_events);
        events.extend(owner.locations(&self.core.people, now));
        events.extend(owner.visit_deadlines(now));
        events
    }

    fn expedition_departures(
        &mut self,
        blocked: &[u64],
        controlled: Option<u64>,
    ) -> Result<(Vec<u64>, Vec<Event>), String> {
        let (Some(owner), Some(travel)) = (&self.expeditions, &self.core.travel) else {
            return Ok((Vec::new(), Vec::new()));
        };
        if owner.itineraries.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let now = self.core.simulated_millis();
        let mut busy = self.local_walk_exclusions(blocked);
        if owner.itineraries.par_iter().any(|row| row.rendezvous.is_some()) {
            busy.par_extend(self.core.physical.work().par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker));
            busy.par_sort_unstable();
            busy.dedup();
            if let Some(excluded) = self.blocked_with_rest(&busy) {
                busy = excluded;
            }
        }
        let outside = self.block_away_from_stock(&busy);
        let proposals = owner.departures(TravelContext {
            people: &self.core.people,
            travel,
            busy: outside.as_deref().unwrap_or(&busy),
            review: travel::RouteReview {
                tick: self.core.physical.tick(),
                elapsed: Some((now, travel.unit_millis(self.core.physical.tick_millis()))),
                delay: 1,
            },
            controlled,
        });
        if proposals.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let requests: Vec<_> = proposals.par_iter().map(|row| row.departure).collect();
        let actors: Vec<_> = requests.par_iter().map(|request| request.person).collect();
        let mut outcomes = self.depart(&requests, &busy)?;
        outcomes.par_sort_unstable_by_key(|outcome| outcome.person);
        let events = self
            .expeditions
            .as_mut()
            .expect("expeditions checked")
            .record_departures(&proposals, &outcomes, now);
        Ok((actors, events))
    }

    pub(super) fn act_on_expeditions(
        &mut self,
        blocked: &[u64],
    ) -> Result<(Vec<u64>, Vec<Event>), String> {
        let Some(owner) = &self.expeditions else {
            return Ok((Vec::new(), Vec::new()));
        };
        if owner.itineraries.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let busy = self.local_walk_exclusions(blocked);
        let observers: Vec<_> = owner
            .itineraries
            .par_iter()
            .filter_map(|row| {
                if row.objective != Some(Objective::ObserveFood)
                    || row.objective_met
                    || !row.go_on
                    || !row.stage.at_destination()
                    || busy.binary_search(&row.person).is_ok()
                    || !self.regional_stock_reachable(row.person, row.destination)
                {
                    return None;
                }
                let person = &self.core.people[self
                    .core
                    .people
                    .binary_search_by_key(&row.person, |person| person.id)
                    .ok()?];
                (person.health > 0 && !person.controlled && person.region == Some(row.destination))
                    .then_some((row.person, row.destination))
            })
            .collect();
        let mut events = self.observe_expedition_food(&observers)?;
        let mut busy = blocked.to_vec();
        busy.par_extend(observers.par_iter().map(|&(person, _)| person));
        busy.par_sort_unstable();
        busy.dedup();
        let (gatherers, acquisitions) = self.acquire_expedition_supplies(&busy)?;
        events.extend(acquisitions);
        busy.par_extend(gatherers.par_iter().copied());
        busy.par_sort_unstable();
        busy.dedup();
        let (couriers, delivered) = self.deliver_expedition_goods(&busy)?;
        events.extend(delivered);
        busy.par_extend(couriers.par_iter().copied());
        busy.par_sort_unstable();
        busy.dedup();
        let (mut actors, departures) = self.expedition_departures(&busy, None)?;
        actors.par_extend(observers.par_iter().map(|&(person, _)| person));
        actors.par_extend(gatherers);
        actors.par_extend(couriers);
        actors.par_sort_unstable();
        events.extend(departures);
        Ok((actors, events))
    }

    fn deliver_expedition_goods(
        &mut self,
        blocked: &[u64],
    ) -> Result<(Vec<u64>, Vec<Event>), String> {
        let owner = self.expeditions.as_ref().expect("expeditions checked");
        if !owner.itineraries.par_iter().any(|row| {
            row.delivery_preference.is_some() && row.stage.at_destination() && !row.objective_met
        }) {
            return Ok((Vec::new(), Vec::new()));
        }
        let busy = self.local_walk_exclusions(blocked);
        let opinions = self.opinions.as_ref().map(|owner| owner.index());
        let local = |id, region, controlled| {
            self.core
                .people
                .binary_search_by_key(&id, |person| person.id)
                .ok()
                .is_some_and(|index| {
                    let person = &self.core.people[index];
                    person.health > 0
                        && person.region == Some(region)
                        && (controlled || !person.controlled)
                })
        };
        let mut offers: Vec<_> = owner
            .itineraries
            .par_iter()
            .filter_map(|row| {
                let Objective::Deliver {
                    person,
                    good,
                    quantity,
                    delivered,
                } = row.objective?
                else {
                    return None;
                };
                let preference = row.delivery_preference?;
                if !row.stage.at_destination()
                    || row.objective_met
                    || busy.binary_search(&row.person).is_ok()
                    || !local(row.person, row.destination, false)
                    || !local(person, row.destination, true)
                    || !self.can_pass_possession(row.person, person)
                {
                    return None;
                }
                let motivated = preference.willing
                    || preference
                        .pressure_threshold
                        .zip(opinions.as_ref())
                        .is_some_and(|(threshold, opinions)| {
                            let (fear, duty) = opinions.feelings(row.person, person);
                            fear.saturating_add(duty) >= threshold
                        });
                if !motivated {
                    return None;
                }
                let holder = self.core.physical.custody(Place::Person(row.person))?;
                let instant = self.core.physical.lot_instant(self.core.physical.tick());
                let available = holder
                    .lots
                    .iter()
                    .filter(|lot| lot.good == good && lot.expires_at.is_none_or(|at| at > instant))
                    .map(|lot| lot.quantity)
                    .sum::<u64>()
                    .saturating_sub(preference.reserve);
                let quantity = available.min(quantity - delivered);
                (quantity > 0).then_some(Delivery {
                    traveler: row.person,
                    receiver: person,
                    region: row.destination,
                    good,
                    quantity,
                })
            })
            .collect();
        if offers.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        offers.par_sort_unstable_by_key(|row| row.traveler);
        let actions: Vec<_> = offers
            .par_iter()
            .enumerate()
            .map(|(index, offer)| {
                let id = index as u64 + 1;
                game_lots::Transaction {
                    id,
                    movements: vec![game_lots::Movement {
                        id,
                        from: Place::Person(offer.traveler),
                        to: Some(Place::Person(offer.receiver)),
                        good: offer.good,
                        quantity: offer.quantity,
                        batch: None,
                        item: None,
                    }],
                    creations: Vec::new(),
                }
            })
            .collect();
        let seed = self.core.seed ^ self.core.decision_time_key();
        let draft = self
            .core
            .physical
            .prepare_interval_transfer(&actions, seed)?;
        let mut approved: Vec<_> = draft
            .transactions()
            .par_iter()
            .filter_map(|receipt| receipt.accepted.then_some(receipt.id))
            .collect();
        approved.par_sort_unstable();
        let paid: Vec<_> = offers
            .par_iter()
            .enumerate()
            .filter_map(|(index, row)| {
                approved
                    .binary_search(&(index as u64 + 1))
                    .is_ok()
                    .then_some(*row)
            })
            .collect();
        self.core.physical.commit_interval_consumption(draft)?;
        self.work_checked_revision = None;
        let events = self
            .expeditions
            .as_mut()
            .expect("expeditions checked")
            .deliveries(&paid, self.core.simulated_millis());
        Ok((offers.par_iter().map(|row| row.traveler).collect(), events))
    }

    pub(super) fn record_expedition_deliveries(&mut self, receipts: &mut [Delivery]) {
        if let Some(owner) = &mut self.expeditions {
            receipts.par_sort_unstable_by_key(|row| row.traveler);
            let events = owner.deliveries(receipts, self.core.simulated_millis());
            owner.pending_events.extend(events);
        }
    }

    fn observe_expedition_food(&mut self, observers: &[(u64, u64)]) -> Result<Vec<Event>, String> {
        if observers.is_empty() {
            return Ok(Vec::new());
        }
        let now = self.core.simulated_millis();
        let tick = self.core.physical.tick();
        let view = self.core.physical.stock_view(tick)?;
        let edible: HashSet<_> = self
            .core
            .physical
            .goods()
            .par_iter()
            .filter(|good| good.food.is_some())
            .map(|good| good.id)
            .collect();
        let mut regions: Vec<_> = observers.par_iter().map(|&(_, region)| region).collect();
        regions.par_sort_unstable();
        regions.dedup();
        let estimate: Vec<_> = regions
            .par_iter()
            .map(|&region| {
                let start = view
                    .containers
                    .partition_point(|row| row.place < (Place::Stock { region, good: 0 }));
                let end = view.containers.partition_point(|row| {
                    row.place
                        <= (Place::Stock {
                            region,
                            good: u32::MAX,
                        })
                });
                view.containers[start..end]
                    .par_iter()
                    .filter_map(|row| {
                        let Place::Stock { good, .. } = row.place else {
                            return None;
                        };
                        edible.contains(&good).then_some(row.used)
                    })
                    .reduce(|| 0, u64::saturating_add)
            })
            .collect();
        let mut observations: Vec<_> = observers
            .par_iter()
            .map(|&(person, region)| {
                (
                    person,
                    region,
                    estimate[regions.binary_search(&region).expect("survey region")],
                )
            })
            .collect();
        observations.par_sort_unstable();
        if let Some(knowledge) = &self.knowledge {
            let observers: HashSet<_> = observers.par_iter().map(|&(person, _)| person).collect();
            let blocked: Vec<_> = self
                .core
                .people
                .par_iter()
                .filter_map(|person| (!observers.contains(&person.id)).then_some(person.id))
                .collect();
            self.knowledge = game_economy_knowledge::advance(
                Some(knowledge),
                &self.core.people,
                self.core.physical.goods(),
                &view,
                None,
                self.opinions.as_ref(),
                game_economy_knowledge::ObservationAccess {
                    now: tick,
                    not_at_stock: &blocked,
                },
            )?;
        }
        Ok(self
            .expeditions
            .as_mut()
            .expect("expedition checked")
            .observed_food(&observations, now))
    }

    pub(super) fn record_expedition_meetings(
        &mut self,
        encounters: &[game_social::Encounter],
    ) -> Vec<Event> {
        let Some(owner) = &mut self.expeditions else {
            return Vec::new();
        };
        if encounters.is_empty() || owner.itineraries.is_empty() {
            return Vec::new();
        }
        let mut contacts: Vec<_> = encounters
            .par_iter()
            .flat_map_iter(|event| {
                [
                    (event.a, event.b, event.region),
                    (event.b, event.a, event.region),
                ]
            })
            .collect();
        contacts.par_sort_unstable();
        owner.met_people(&contacts, self.core.simulated_millis())
    }

    pub fn expedition(&self, person: u64) -> Option<&Itinerary> {
        self.expeditions.as_ref()?.active(person)
    }

    pub fn plan_controlled_expedition(
        &mut self,
        person: u64,
        destination: u64,
        visit_millis: u64,
        objective: Option<Objective>,
    ) -> Result<Event, String> {
        let actor = self.controlled_person(person)?;
        let origin = actor
            .region
            .filter(|_| actor.health > 0)
            .ok_or("an expedition requires a living local traveler")?;
        let travel = self.core.travel.as_ref().ok_or("travel is disabled")?;
        if !travel
            .routes
            .par_iter()
            .any(|route| route.destination == destination || route.source == destination)
        {
            return Err("unknown travel destination".into());
        }
        match objective {
            Some(Objective::Meet { person: target })
            | Some(Objective::Deliver { person: target, .. })
                if target == person
                    || self
                        .core
                        .people
                        .binary_search_by_key(&target, |person| person.id)
                        .is_err() =>
            {
                return Err("the objective requires another known person".into());
            }
            Some(Objective::Deliver {
                good,
                quantity,
                delivered,
                ..
            }) if quantity == 0
                || delivered != 0
                || !self
                    .core
                    .physical
                    .goods()
                    .par_iter()
                    .any(|row| row.id == good) =>
            {
                return Err("invalid expedition delivery objective".into());
            }
            _ => {}
        }
        self.expeditions
            .as_mut()
            .ok_or("expeditions are disabled")?
            .start(
                Plan {
                    person,
                    origin,
                    destination,
                    visit_millis,
                    depart_after_millis: None,
                    head_home: false,
                    objective,
                },
                self.core.simulated_millis(),
            )
    }

    pub fn schedule_controlled_departure(
        &mut self,
        person: u64,
        delay: u64,
    ) -> Result<Event, String> {
        self.controlled_person(person)?;
        let now = self.core.simulated_millis();
        let depart_after_millis = (delay > 0)
            .then(|| {
                now.checked_add(u128::from(delay))
                    .ok_or("departure instant overflow")
            })
            .transpose()?;
        self.expeditions
            .as_mut()
            .ok_or("expeditions are disabled")?
            .schedule_departure(person, depart_after_millis, now)
    }

    pub fn continue_controlled_expedition(&mut self, person: u64) -> Result<Vec<Event>, String> {
        self.controlled_person(person)?;
        self.expedition(person).ok_or("no active itinerary")?;
        let mut events = self.expedition_time_events();
        let (_, departures) = self.expedition_departures(&[], Some(person))?;
        events.extend(departures);
        Ok(events)
    }

    pub fn set_controlled_rendezvous(
        &mut self,
        person: u64,
        choice: Option<(u64, u32, u64)>,
    ) -> Result<Event, String> {
        let actor = self.controlled_person(person)?;
        if actor.health == 0 {
            return Err("person cannot choose a rendezvous".into());
        }
        let now = self.core.simulated_millis();
        let rendezvous = choice.map(|(group, minimum_companions, wait)| {
            let wait_until_millis = now.checked_add(u128::from(wait)).ok_or("rendezvous instant overflow")?;
            Ok::<_, &str>(Rendezvous { group, minimum_companions, wait_until_millis })
        }).transpose()?;
        let owner = self.expeditions.as_mut().ok_or("expeditions are disabled")?;
        let event = owner.set_rendezvous(person, rendezvous, now)?;
        owner.pending_events.push(event);
        Ok(event)
    }

    pub fn return_controlled_expedition(&mut self, person: u64) -> Result<Vec<Event>, String> {
        self.controlled_person(person)?;
        self.expeditions
            .as_mut()
            .ok_or("expeditions are disabled")?
            .choose_return(person)?;
        self.continue_controlled_expedition(person)
    }

    pub fn abandon_controlled_expedition(&mut self, person: u64) -> Result<Event, String> {
        let region = self.controlled_person(person)?.region.unwrap_or(0);
        self.expeditions
            .as_mut()
            .ok_or("expeditions are disabled")?
            .abandon(person, region, self.core.simulated_millis())
    }

    pub fn survey_controlled_expedition(&mut self, person: u64) -> Result<Vec<Event>, String> {
        let actor = self.controlled_person(person)?;
        let plan = self.expedition(person).ok_or("no active itinerary")?;
        if actor.health == 0
            || actor.region != Some(plan.destination)
            || plan.objective != Some(Objective::ObserveFood)
            || !plan.stage.at_destination()
            || !self.regional_stock_reachable(person, plan.destination)
        {
            return Err("surveying requires physical access to the destination stock".into());
        }
        self.observe_expedition_food(&[(person, plan.destination)])
    }
}
