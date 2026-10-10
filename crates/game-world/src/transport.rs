//! The coordinator supplies actual bodies and commits pure transport effects.
//! Carrying changes physical support, never ownership, kinship, or government.
use crate::*;
pub use game_transport::{
    Action, Authority, Body, CarryGroup, Permission, Position, RoadPosition, Rules, View, World,
};

struct ComponentFacts {
    groups: BTreeMap<PersonId, CarryGroup>,
    bodies: BTreeMap<PersonId, Body>,
}

impl Simulation {
    pub fn initialize_transport(&mut self, rules: Rules) -> Result<(), Error> {
        if self.state.transport.is_some() || self.state.physical.is_none() {
            return Err(Error::Unavailable);
        }
        self.state.transport = Some(World::new(rules)?);
        Ok(())
    }
    pub fn transport_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::lineage_fixture(seed, execution)?;
        sim.state.scenario = ScenarioInfo {
            id: "transport-fixture".into(),
            version: 1,
            debug_fixture: true,
        };
        sim.initialize_transport(Rules {
            adult_mass: Quantity(6_000),
            newborn_mass: Quantity(1_000),
            lift_time: SimDuration(6_000),
            ..Rules::default()
        })?;
        Ok(sim)
    }
    pub(crate) fn resolved_location(&self, actor: PersonId) -> Location {
        let mut current = actor;
        let mut seen = BTreeSet::new();
        while seen.insert(current) {
            match self.state.people[self.person_index[&current]]
                .location
                .clone()
            {
                Location::Carried { carrier } => current = carrier,
                location => return location,
            }
        }
        self.state.people[self.person_index[&actor]]
            .location
            .clone()
    }
    pub(crate) fn transport_position(&self, actor: PersonId) -> Option<Position> {
        if !self.person_index.contains_key(&actor) {
            return None;
        }
        location_position(&self.resolved_location(actor), self.state.time).ok()
    }
    pub(crate) fn transport_same_place(&self, a: PersonId, b: PersonId) -> bool {
        if !matches!((self.transport_position(a),self.transport_position(b)),(Some(a),Some(b)) if a.same_point(b))
        {
            return false;
        }
        let Some(combat) = &self.state.combat else {
            return true;
        };
        let anchor = |mut actor: PersonId| {
            let mut visited = BTreeSet::new();
            while visited.insert(actor) {
                match self.state.people[self.person_index[&actor]].location {
                    Location::Carried { carrier } => actor = carrier,
                    _ => break,
                }
            }
            actor
        };
        let tactical = |actor: PersonId| {
            combat
                .encounters
                .values()
                .filter(|e| e.active)
                .find_map(|e| {
                    e.fighters
                        .get(&actor)
                        .filter(|f| !f.withdrawn)
                        .map(|f| (e.id, e.map, f.position))
                })
        };
        match (tactical(anchor(a)), tactical(anchor(b))) {
            (None, None) => true,
            (Some((a, map, ap)), Some((b, _, bp))) => {
                a == b
                    && game_conflict::distance(ap, bp) <= u128::from(combat.rules.reach).pow(2)
                    && combat.map(map).is_some_and(|m| m.clear(ap, bp))
            }
            _ => false,
        }
    }
    pub(crate) fn transport_is_moving(&self, actor: PersonId) -> bool {
        matches!(self.resolved_location(actor), Location::InTransit { .. })
    }
    pub(crate) fn transport_exerting(&self, actor: PersonId) -> bool {
        matches!(
            self.state.people[self.person_index[&actor]].location,
            Location::InTransit { .. }
        ) || self
            .state
            .physical
            .as_ref()
            .is_some_and(|w| matches!(w.actors[&actor].activity, Activity::Lift { .. }))
    }
    fn transport_bodies_at(&self, now: SimTime) -> Result<BTreeMap<PersonId, Body>, Error> {
        self.transport_body_facts_at(self.state.people.iter(), now)
    }
    fn transport_body_facts_at<'a>(
        &self,
        people: impl Iterator<Item = &'a Person>,
        now: SimTime,
    ) -> Result<BTreeMap<PersonId, Body>, Error> {
        let rules = &self
            .state
            .transport
            .as_ref()
            .ok_or(Error::Unavailable)?
            .rules;
        people
            .map(|person| {
                let body = &self
                    .state
                    .physical
                    .as_ref()
                    .ok_or(Error::Unavailable)?
                    .actors[&person.id];
                let limits = self.body_rules(person.id);
                let mass = if let Some(w) = &self.state.development {
                    game_transport::body_mass(
                        rules,
                        SimDuration(w.lives[&person.id].age_at(now)?),
                        w.rules.adulthood,
                    )
                } else {
                    rules.adult_mass
                };
                let cargo = self
                    .inventory_index
                    .get(&InventoryLocation::Carried(person.id))
                    .into_iter()
                    .flatten()
                    .try_fold(0_u64, |sum, id| {
                        sum.checked_add(self.state.inventories[id].quantity.0)
                            .ok_or(Error::Overflow)
                    })?;
                let can_walk = body.vitals.alive
                    && body.vitals.health > 0
                    && body.vitals.fatigue < limits.fatigue_limit
                    && self.independent(person.id)
                    && !self.combat_restricts(person.id)
                    && !matches!(person.location, Location::Carried { .. });
                let can_bear = can_walk
                    && matches!(
                        body.activity,
                        Activity::Idle | Activity::Rest | Activity::Lift { .. }
                    );
                Ok((
                    person.id,
                    Body {
                        position: self
                            .transport_position(person.id)
                            .ok_or(Error::Unavailable)?,
                        mass,
                        cargo: Quantity(cargo),
                        capacity: limits.carrying_capacity,
                        can_hold: body.vitals.alive
                            && body.vitals.health > 0
                            && self
                                .state
                                .combat
                                .as_ref()
                                .is_none_or(|w| body.vitals.health > w.rules.incapacitation),
                        can_walk,
                        can_bear,
                    },
                ))
            })
            .collect()
    }
    fn transport_component_facts(&self, actor: PersonId) -> Result<ComponentFacts, Error> {
        let world = self.state.transport.as_ref().ok_or(Error::Unavailable)?;
        let component = world.component(actor);
        let people = component
            .iter()
            .map(|id| {
                self.person_index
                    .get(id)
                    .and_then(|index| self.state.people.get(*index))
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let bodies = self.transport_body_facts_at(people.into_iter(), self.state.time)?;
        let groups = world
            .groups
            .iter()
            .filter(|(id, _)| component.contains(id))
            .map(|(id, group)| (*id, group.clone()))
            .collect();
        Ok(ComponentFacts { groups, bodies })
    }
    pub(crate) fn transport_bodies(&self) -> Result<BTreeMap<PersonId, Body>, Error> {
        self.transport_bodies_at(self.state.time)
    }
    pub(crate) fn transport_can_depart(&self, actor: PersonId) -> bool {
        let Some(w) = &self.state.transport else {
            return true;
        };
        if w.groups.contains_key(&actor) || w.busy(actor) {
            return false;
        }
        let Ok(ComponentFacts { groups, bodies }) = self.transport_component_facts(actor) else {
            return false;
        };
        if !bodies
            .get(&actor)
            .is_some_and(|body| body.can_walk && body.cargo <= body.capacity)
        {
            return false;
        }
        let Ok(plan) = game_transport::capacity_plan(&groups, &bodies) else {
            return false;
        };
        let component = w.component(actor);
        plan.unable.is_disjoint(&component) && plan.overloaded.is_disjoint(&component)
    }
    pub(crate) fn transport_body_load(&self, actor: PersonId) -> Quantity {
        let Some(_) = &self.state.transport else {
            return Quantity(0);
        };
        let Ok(ComponentFacts { groups, bodies }) = self.transport_component_facts(actor) else {
            return Quantity(0);
        };
        let Ok(plan) = game_transport::capacity_plan(&groups, &bodies) else {
            return Quantity(0);
        };
        Quantity(
            plan.shares
                .iter()
                .filter(|((_, bearer), _)| *bearer == actor)
                .map(|(_, q)| q.0)
                .sum(),
        )
    }
    pub(crate) fn transport_transfer_capacity(&self, actor: PersonId, amount: u64) -> bool {
        let Some(_) = &self.state.transport else {
            return true;
        };
        let Ok(ComponentFacts { groups, bodies }) = self.transport_component_facts(actor) else {
            return false;
        };
        game_transport::transfer_capacity(&groups, &bodies, actor, Quantity(amount))
            .unwrap_or(false)
    }
    pub(crate) fn transport_boundary(&self, limit: SimTime) -> Result<SimTime, Error> {
        let Some(w) = &self.state.transport else {
            return Ok(limit);
        };
        if w.groups.is_empty()
            || !w
                .groups
                .values()
                .any(|g| self.transport_is_moving(g.anchor))
        {
            return Ok(limit);
        }
        let crossing = game_physiology::first_crossing(limit.0 - self.state.time.0, |elapsed| {
            let bodies = self.transport_bodies_at(SimTime(self.state.time.0 + elapsed))?;
            let plan = game_transport::capacity_plan(&w.groups, &bodies)?;
            Ok(plan
                .overloaded
                .iter()
                .any(|id| self.transport_is_moving(w.groups[id].anchor)))
        })?;
        Ok(crossing.map_or(limit, |elapsed| SimTime(self.state.time.0 + elapsed)))
    }
    pub(crate) fn transport_departed(&mut self, actor: PersonId) {
        let Some(w) = &self.state.transport else {
            return;
        };
        let members = w.component(actor);
        let location = self.state.people[self.person_index[&actor]]
            .location
            .clone();
        let Location::InTransit {
            from, to, arrives, ..
        } = location
        else {
            return;
        };
        let passengers: BTreeSet<_> = w.groups.keys().copied().collect();
        let approach = w.approaches.get(&actor).cloned();
        for member in members {
            for dependents in self.dependent_index.values_mut() {
                dependents.remove(&member);
            }
            self.leave_shelter_on_movement(member);
            if member == actor || passengers.contains(&member) {
                continue;
            }
            self.interrupt_actor_activity(member);
            self.clear_transport_arrival(member);
            self.leave_shelter_on_movement(member);
            for dependents in self.dependent_index.values_mut() {
                dependents.remove(&member);
            }
            let person = &mut self.state.people[self.person_index[&member]];
            person.location = location.clone();
            person.route.clear();
            let deadline = approach.as_ref().map_or(arrives, |a| a.completes);
            if let Some(approach) = &approach {
                self.state
                    .transport
                    .as_mut()
                    .unwrap()
                    .approaches
                    .insert(member, approach.clone());
            }
            self.state
                .due
                .entry(deadline)
                .or_default()
                .push(Due::Arrive(member));
            self.emit(EventKind::Departed {
                actor: member,
                from,
                to,
                arrives: deadline,
            });
        }
    }
    fn clear_transport_arrival(&mut self, actor: PersonId) {
        for due in self.state.due.values_mut() {
            due.retain(|d| !matches!(d,Due::Arrive(id) if *id==actor));
        }
        self.state.due.retain(|_, due| !due.is_empty());
    }
    fn stop_transport_component(&mut self, actor: PersonId) {
        let members = self
            .state
            .transport
            .as_ref()
            .map_or_else(|| BTreeSet::from([actor]), |w| w.component(actor));
        let positions: Vec<_> = members
            .iter()
            .filter_map(|id| {
                let location = self.state.people[self.person_index[id]].location.clone();
                if matches!(location, Location::InTransit { .. }) {
                    location_position(&location, self.state.time)
                        .ok()
                        .map(|place| (*id, place))
                } else {
                    None
                }
            })
            .collect();
        for (member, position) in positions {
            self.clear_transport_arrival(member);
            let destination = match self.state.people[self.person_index[&member]].location {
                Location::InTransit { to, .. } => Some(to),
                _ => None,
            };
            if let Some(w) = &mut self.state.transport {
                if let Some(destination) = destination {
                    w.resume_targets.insert(member, destination);
                }
                if let Some(approach) = w.approaches.remove(&member) {
                    w.paused_approaches.insert(member, approach.target);
                }
            }
            self.state.people[self.person_index[&member]].location = position_location(position);
            self.state.people[self.person_index[&member]].route.clear();
            self.emit(EventKind::Interrupted { actor: member });
        }
    }
    pub(crate) fn transport_death(&mut self, actor: PersonId) {
        self.interrupt_transport_lift(actor);
        if !matches!(
            self.state.people[self.person_index[&actor]].location,
            Location::Carried { .. }
        ) {
            self.stop_transport_component(actor);
        }
    }
    pub(crate) fn transport_stop_for_force(&mut self, target: PersonId) -> Result<(), Error> {
        if !self.person_index.contains_key(&target) {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(self.state.people.len())?;
        self.stop_transport_component(target);
        Ok(())
    }
    pub(crate) fn transport_retreat(
        &mut self,
        actor: PersonId,
        destination: AreaId,
    ) -> Result<(), Error> {
        if !self.person_index.contains_key(&actor) || self.state.transport.is_none() {
            return Err(Error::Unavailable);
        }
        let Some(Position::Road(position)) = self.transport_position(actor) else {
            return Err(Error::Unavailable);
        };
        let direction_from = if destination == position.to {
            position.from
        } else if destination == position.from {
            position.to
        } else {
            return Err(Error::NoRoute);
        };
        let reverse = self
            .state
            .areas
            .iter()
            .find(|a| a.id == direction_from)
            .and_then(|a| a.links.iter().find(|l| l.destination == destination))
            .map(|l| l.duration)
            .ok_or(Error::NoRoute)?;
        let leg = game_transport::retreat(position, destination, reverse, self.state.time)?;
        // Tactical retreat has already resolved its turn and escape boundary.
        // Physical capability and burden still gate the remaining journey.
        if !self.transport_can_depart(actor) {
            return Err(Error::Unavailable);
        }
        self.ensure_event_capacity(self.state.people.len() + 1)?;
        self.stop_transport_component(actor);
        self.interrupt_actor_activity(actor);
        self.state.people[self.person_index[&actor]].location = Location::InTransit {
            from: leg.from,
            to: leg.to,
            departed: leg.departed,
            road_start: leg.road_start,
            arrives: leg.arrives,
        };
        self.state
            .due
            .entry(leg.arrives)
            .or_default()
            .push(Due::Arrive(actor));
        self.emit(EventKind::Departed {
            actor,
            from: leg.from,
            to: leg.to,
            arrives: leg.arrives,
        });
        self.transport_departed(actor);
        Ok(())
    }
    pub(crate) fn transport_release_confinement(&mut self, target: PersonId) {
        let Some(group) = self
            .state
            .transport
            .as_ref()
            .and_then(|w| w.groups.get(&target))
            .filter(|g| g.authority == Authority::Captive)
            .cloned()
        else {
            return;
        };
        self.stop_transport_component(group.anchor);
        let Some(position) = self.transport_position(target) else {
            return;
        };
        let shelter = self.health_before_putdown(target);
        self.state
            .transport
            .as_mut()
            .unwrap()
            .groups
            .remove(&target);
        self.state.people[self.person_index[&target]].location = position_location(position);
        if let Position::Area(area) = position {
            self.index_dependent(target, area);
        }
        self.health_after_putdown(target, shelter);
    }
    pub(crate) fn transport_birth(&mut self, child: PersonId, gestator: PersonId) {
        if self.state.transport.is_none()
            || !matches!(self.resolved_location(gestator), Location::InTransit { .. })
        {
            return;
        }
        let bodies = self
            .transport_bodies()
            .expect("actual birth has a physical location");
        let group = self
            .state
            .transport
            .as_mut()
            .unwrap()
            .attach_newborn(child, gestator, self.state.time, &bodies)
            .expect("newborn colocated with actual gestator");
        self.clear_transport_arrival(child);
        self.state.people[self.person_index[&child]].location = Location::Carried {
            carrier: group.anchor,
        };
        self.state.people[self.person_index[&child]].route.clear();
        self.reconcile_transport();
    }
    /// A guardian has physically reached the sensed body. This choice uses
    /// local body/support facts and the guardian's own ability to help.
    pub(crate) fn transport_guardian_rescue(
        &self,
        guardian: PersonId,
        target: PersonId,
    ) -> Option<Command> {
        let world = self.state.transport.as_ref()?;
        if !self.transport_same_place(guardian, target)
            || self.transport_is_moving(guardian)
            || world.busy(guardian)
        {
            return None;
        }
        let body = &self.state.physical.as_ref()?.actors[&guardian];
        if !matches!(body.activity, Activity::Idle | Activity::Rest)
            || self.combat_restricts(guardian)
        {
            return None;
        }
        if matches!(
            self.resolved_location(guardian),
            Location::StoppedOnRoad { .. }
        ) {
            let mind = &self.state.decisions.as_ref()?.minds[&guardian];
            if let Some(signal) = mind.child_signals.get(&target).filter(|s| {
                s.location == self.resolved_location(guardian) && s.observed_at == self.state.time
            }) {
                if signal.urgency > 0 {
                    let size = self.state.physical.as_ref()?.rules.meal_size;
                    if let Some(source) = mind.knowledge.values().find(|f| {
                        f.good == Good::Food
                            && f.accessible
                            && f.quantity >= size
                            && self
                                .inventory_index
                                .get(&InventoryLocation::Carried(guardian))
                                .is_some_and(|ids| ids.contains(&f.inventory))
                    }) {
                        return Some(Command::Care {
                            child: target,
                            source: source.inventory,
                        });
                    }
                }
                if signal.preference == Some(ChildhoodInterest::Company) {
                    return Some(Command::Comfort { child: target });
                }
            }
            if body.vitals.fatigue >= self.body_rules(guardian).fatigue_limit {
                return Some(Command::Rest);
            }
            if world
                .groups
                .get(&target)
                .is_some_and(|g| g.bearers.contains(&guardian))
                && self.transport_can_depart(guardian)
            {
                return Some(Command::Transport {
                    action: Action::Resume,
                });
            }
        }
        if !self.independent(target)
            || self.state.physical.as_ref()?.actors[&target].vitals.health == 0
        {
            let bodies = self.transport_bodies().ok()?;
            let needs_support = world.groups.get(&target).is_none_or(|group| {
                group.bearers.iter().any(|id| !bodies[id].can_hold)
                    || game_transport::capacity_plan(&world.groups, &bodies)
                        .is_ok_and(|plan| plan.overloaded.contains(&target))
            });
            if needs_support && bodies[&guardian].can_bear {
                return Some(Command::Transport {
                    action: if world.groups.contains_key(&target) {
                        Action::JoinCarry { passenger: target }
                    } else {
                        Action::OfferCarry { passenger: target }
                    },
                });
            }
        }
        None
    }
    pub(crate) fn reconcile_transport(&mut self) {
        let Some(w) = &self.state.transport else {
            return;
        };
        let Ok(bodies) = self.transport_bodies() else {
            return;
        };
        let Ok(plan) = game_transport::capacity_plan(&w.groups, &bodies) else {
            return;
        };
        let mut stopped: BTreeSet<_> = plan.unable;
        for passenger in plan.overloaded {
            stopped.extend(w.groups[&passenger].bearers.iter().copied());
        }
        for (id, body) in &bodies {
            if (!body.can_walk || body.cargo > body.capacity)
                && matches!(
                    self.state.people[self.person_index[id]].location,
                    Location::InTransit { .. }
                )
            {
                stopped.insert(*id);
            }
        }
        for actor in stopped {
            self.stop_transport_component(actor);
        }
        self.state
            .transport
            .as_mut()
            .unwrap()
            .prune_offers(self.state.time);
        let stopped: BTreeSet<_> = self
            .state
            .people
            .iter()
            .filter(|p| matches!(p.location, Location::StoppedOnRoad { .. }))
            .map(|p| p.id)
            .collect();
        self.state
            .transport
            .as_mut()
            .unwrap()
            .resume_targets
            .retain(|id, _| stopped.contains(id));
        self.state
            .transport
            .as_mut()
            .unwrap()
            .paused_approaches
            .retain(|id, _| stopped.contains(id));
        let dependents: Vec<_> = self
            .state
            .transport
            .as_ref()
            .unwrap()
            .groups
            .keys()
            .filter_map(|id| {
                if let Location::AtArea(area) = self.resolved_location(*id) {
                    self.state.physical.as_ref().unwrap().actors[id]
                        .vitals
                        .alive
                        .then_some((*id, area))
                } else {
                    None
                }
            })
            .collect();
        for (child, area) in dependents {
            self.index_dependent(child, area);
        }
    }
    fn carry_permission(
        &self,
        passenger: PersonId,
        bearers: &BTreeSet<PersonId>,
        accepted: bool,
    ) -> Permission {
        let body = &self.state.physical.as_ref().unwrap().actors[&passenger];
        let incapacitated = !body.vitals.alive
            || body.vitals.health == 0
            || self
                .state
                .combat
                .as_ref()
                .is_some_and(|w| body.vitals.health <= w.rules.incapacitation);
        let yielded = self
            .state
            .force
            .as_ref()
            .and_then(|w| w.yielded.get(&passenger))
            .is_some_and(|y| bearers.is_subset(&y.captors));
        let captive = self
            .state
            .combat
            .as_ref()
            .and_then(|w| w.captivity.get(&passenger))
            .is_some_and(|c| bearers.contains(&c.captor));
        Permission {
            passenger_accepted: accepted,
            unable_to_consent: incapacitated || !self.independent(passenger),
            yielded,
            captive,
            newborn: false,
        }
    }
    fn begin_transport_lift(
        &mut self,
        passenger: PersonId,
        bearers: BTreeSet<PersonId>,
        accepted: bool,
    ) -> Result<(), Error> {
        let bodies = self.transport_bodies()?;
        if bearers
            .iter()
            .any(|id| !self.transport_same_place(*id, passenger))
        {
            return Err(Error::Unavailable);
        }
        if bearers.iter().any(|id| self.transport_is_moving(*id))
            || self.transport_is_moving(passenger)
        {
            return Err(Error::Unavailable);
        }
        let permission = self.carry_permission(passenger, &bearers, accepted);
        self.ensure_event_capacity(bearers.len() + 1)?;
        let mut candidate = self.state.transport.as_ref().unwrap().clone();
        let lift =
            candidate.begin_lift(passenger, bearers, permission, self.state.time, &bodies)?;
        for bearer in &lift.bearers {
            self.interrupt_actor_activity(*bearer);
        }
        if !matches!(
            self.state.physical.as_ref().unwrap().actors[&passenger].activity,
            Activity::Escape { .. }
        ) {
            self.interrupt_actor_activity(passenger);
        }
        self.state.transport = Some(candidate);
        for bearer in &lift.bearers {
            self.state
                .physical
                .as_mut()
                .unwrap()
                .actors
                .get_mut(bearer)
                .unwrap()
                .activity = Activity::Lift {
                passenger,
                completes: lift.completes,
            };
            self.state
                .due
                .entry(lift.completes)
                .or_default()
                .push(Due::Physical(*bearer));
        }
        Ok(())
    }
    pub(crate) fn complete_transport_lift(&mut self, actor: PersonId) {
        let Some(w) = &self.state.transport else {
            return;
        };
        let Some(lift) = w
            .lifts
            .values()
            .find(|l| l.bearers.contains(&actor))
            .cloned()
        else {
            return;
        };
        let Ok(bodies) = self.transport_bodies() else {
            self.interrupt_transport_lift(actor);
            return;
        };
        let group = self.state.transport.as_mut().unwrap().complete_lift(
            lift.passenger,
            self.state.time,
            &bodies,
        );
        match group {
            Ok(group) => {
                // Remove coupled endpoints only after the domain completion has
                // consumed the pending lift, so the generic hook cannot cancel it.
                for bearer in &group.bearers {
                    self.interrupt_actor_activity(*bearer);
                }
                if !matches!(
                    self.state.physical.as_ref().unwrap().actors[&group.passenger].activity,
                    Activity::Escape { .. }
                ) {
                    self.interrupt_actor_activity(group.passenger);
                }
                self.clear_transport_arrival(group.passenger);
                for dependents in self.dependent_index.values_mut() {
                    dependents.remove(&group.passenger);
                }
                self.state.people[self.person_index[&group.passenger]].location =
                    Location::Carried {
                        carrier: group.anchor,
                    };
                self.state.people[self.person_index[&group.passenger]]
                    .route
                    .clear();
            }
            Err(_) => self.interrupt_transport_lift(actor),
        }
    }
    pub(crate) fn interrupt_transport_lift(&mut self, actor: PersonId) {
        let Some(w) = &mut self.state.transport else {
            return;
        };
        let lifts = w.cancel_lifts(actor);
        for lift in lifts {
            for bearer in lift.bearers {
                if matches!(self.state.physical.as_ref().unwrap().actors[&bearer].activity,Activity::Lift {passenger,..} if passenger==lift.passenger)
                {
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&bearer)
                        .unwrap()
                        .activity = Activity::Idle;
                    for due in self.state.due.values_mut() {
                        due.retain(|d| !matches!(d,Due::Physical(id) if *id==bearer));
                    }
                }
            }
        }
        self.state.due.retain(|_, due| !due.is_empty());
    }
    pub(crate) fn transport_arrive(&mut self, actor: PersonId) -> bool {
        let Some(approach) = self
            .state
            .transport
            .as_ref()
            .and_then(|w| w.approaches.get(&actor))
            .cloned()
        else {
            return false;
        };
        if approach.completes != self.state.time {
            return false;
        }
        self.state
            .transport
            .as_mut()
            .unwrap()
            .approaches
            .remove(&actor);
        let position = Position::Road(approach.target);
        self.state.people[self.person_index[&actor]].location = position_location(position);
        self.state.people[self.person_index[&actor]].route.clear();
        self.emit(EventKind::Interrupted { actor });
        true
    }
    pub(crate) fn transport_approach_road(
        &mut self,
        actor: PersonId,
        target: RoadPosition,
    ) -> Result<(), Error> {
        if !self.transport_can_depart(actor) || self.state.transport.is_none() {
            return Err(Error::Unavailable);
        }
        let position = self.transport_position(actor).ok_or(Error::Unavailable)?;
        let start = match position {
            Position::Area(endpoint) => RoadPosition {
                progress: if endpoint == target.from {
                    SimDuration(0)
                } else if endpoint == target.to {
                    target.duration
                } else {
                    return Err(Error::Unavailable);
                },
                ..target
            },
            Position::Road(point) => point,
        };
        let link = |from: AreaId, to: AreaId| {
            self.state
                .areas
                .iter()
                .find(|a| a.id == from)
                .and_then(|a| a.links.iter().find(|l| l.destination == to))
                .map(|l| l.duration)
        };
        let original = link(target.from, target.to).ok_or(Error::NoRoute)?;
        if target.duration != original {
            return Err(Error::Unavailable);
        }
        let forward = original;
        let reverse = link(target.to, target.from).unwrap_or(SimDuration(0));
        let (leg, approach) =
            game_transport::approach_from(start, target, forward, reverse, self.state.time)?;
        if !self.state.decisions.as_ref().is_none_or(|w| {
            w.minds[&actor]
                .routes
                .get(&leg.from)
                .is_some_and(|links| links.contains_key(&leg.to))
        }) {
            return Err(Error::NoRoute);
        }
        self.ensure_event_capacity(self.state.people.len() + 1)?;
        self.interrupt_actor_activity(actor);
        self.clear_transport_arrival(actor);
        self.state
            .transport
            .as_mut()
            .unwrap()
            .paused_approaches
            .remove(&actor);
        self.leave_shelter_on_movement(actor);
        let p = &mut self.state.people[self.person_index[&actor]];
        p.location = Location::InTransit {
            from: leg.from,
            to: leg.to,
            departed: leg.departed,
            road_start: leg.road_start,
            arrives: leg.arrives,
        };
        p.route.clear();
        self.state
            .due
            .entry(approach.completes)
            .or_default()
            .push(Due::Arrive(actor));
        self.state
            .transport
            .as_mut()
            .unwrap()
            .approaches
            .insert(actor, approach.clone());
        self.emit(EventKind::Departed {
            actor,
            from: leg.from,
            to: leg.to,
            arrives: approach.completes,
        });
        self.transport_departed(actor);
        Ok(())
    }
    pub(crate) fn transport_command(
        &mut self,
        actor: PersonId,
        action: &Action,
    ) -> Result<(), Error> {
        if self.state.transport.is_none() || !self.person_index.contains_key(&actor) {
            return Err(Error::Unavailable);
        }
        match action {
            Action::OfferCarry { passenger } => {
                self.state
                    .time
                    .0
                    .checked_add(self.state.transport.as_ref().unwrap().rules.lift_time.0)
                    .ok_or(Error::Overflow)?;
                let bodies = self.transport_bodies()?;
                if !self.person_index.contains_key(passenger)
                    || self.transport_is_moving(actor)
                    || self.transport_is_moving(*passenger)
                    || !self.transport_same_place(actor, *passenger)
                {
                    return Err(Error::Unavailable);
                }
                self.state.transport.as_mut().unwrap().offer(
                    actor,
                    *passenger,
                    self.state.time,
                    &bodies,
                )?;
                let bearers = BTreeSet::from([actor]);
                if self
                    .carry_permission(*passenger, &bearers, false)
                    .authority()
                    .is_ok()
                {
                    // A lone willing bearer may need companions. Preserve the
                    // offer when it cannot yet support the actual body.
                    match self.begin_transport_lift(*passenger, bearers, false) {
                        Ok(()) | Err(Error::Unavailable) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Action::AcceptCarry { bearers } => {
                let set: BTreeSet<_> = bearers.iter().copied().collect();
                if set.len() != bearers.len()
                    || !self.independent(actor)
                    || !self.state.physical.as_ref().unwrap().actors[&actor]
                        .vitals
                        .alive
                    || self.combat_restricts(actor)
                {
                    return Err(Error::Unavailable);
                }
                self.begin_transport_lift(actor, set, true)?;
            }
            Action::RefuseCarry { bearer } => {
                let offers = self
                    .state
                    .transport
                    .as_mut()
                    .unwrap()
                    .offers
                    .get_mut(&actor)
                    .ok_or(Error::Unavailable)?;
                if offers.remove(bearer).is_none() {
                    return Err(Error::Unavailable);
                }
                self.state
                    .transport
                    .as_mut()
                    .unwrap()
                    .offers
                    .retain(|_, offers| !offers.is_empty());
            }
            Action::JoinCarry { passenger } => {
                self.state
                    .time
                    .0
                    .checked_add(self.state.transport.as_ref().unwrap().rules.lift_time.0)
                    .ok_or(Error::Overflow)?;
                let bodies = self.transport_bodies()?;
                if !self.person_index.contains_key(passenger)
                    || self.transport_is_moving(actor)
                    || self.transport_is_moving(*passenger)
                    || !self.transport_same_place(actor, *passenger)
                {
                    return Err(Error::Unavailable);
                }
                self.state.transport.as_mut().unwrap().offer(
                    actor,
                    *passenger,
                    self.state.time,
                    &bodies,
                )?;
                let w = self.state.transport.as_ref().unwrap();
                let mut bearers: BTreeSet<_> = w.offers[passenger]
                    .values()
                    .filter(|o| o.expires > self.state.time)
                    .map(|o| o.bearer)
                    .collect();
                if let Some(existing) = w.groups.get(passenger) {
                    bearers.extend(
                        existing
                            .bearers
                            .iter()
                            .filter(|id| bodies[id].can_hold)
                            .copied(),
                    );
                }
                if self
                    .carry_permission(*passenger, &bearers, false)
                    .authority()
                    .is_ok()
                {
                    match self.begin_transport_lift(*passenger, bearers, false) {
                        Ok(()) | Err(Error::Unavailable) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Action::PutDown { passenger } | Action::WithdrawSupport { passenger } => {
                let group = self
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .groups
                    .get(passenger)
                    .ok_or(Error::Unavailable)?
                    .clone();
                if actor != *passenger && !group.bearers.contains(&actor) {
                    return Err(Error::Unavailable);
                }
                if actor == *passenger && (!self.independent(actor) || self.combat_restricts(actor))
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(self.state.people.len())?;
                self.stop_transport_component(group.anchor);
                let position = self
                    .transport_position(*passenger)
                    .ok_or(Error::Unavailable)?;
                let shelter = self.health_before_putdown(*passenger);
                self.state
                    .transport
                    .as_mut()
                    .unwrap()
                    .remove_group(*passenger);
                self.state.people[self.person_index[passenger]].location =
                    position_location(position);
                if let Position::Area(area) = position {
                    self.index_dependent(*passenger, area);
                }
                self.health_after_putdown(*passenger, shelter);
            }
            Action::Stop => {
                if !self.independent(actor)
                    || !self.state.physical.as_ref().unwrap().actors[&actor]
                        .vitals
                        .alive
                    || !matches!(
                        self.state.people[self.person_index[&actor]].location,
                        Location::InTransit { .. }
                    )
                {
                    return Err(Error::Unavailable);
                }
                self.ensure_event_capacity(self.state.people.len())?;
                self.stop_transport_component(actor);
            }
            Action::Resume => {
                if let Some(target) = self
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .paused_approaches
                    .get(&actor)
                    .copied()
                {
                    self.transport_approach_road(actor, target)?;
                    self.revision += 1;
                    return Ok(());
                }
                if !self.transport_can_depart(actor) {
                    return Err(Error::Unavailable);
                }
                let Location::StoppedOnRoad {
                    from,
                    to,
                    progress,
                    duration,
                } = self.state.people[self.person_index[&actor]].location
                else {
                    return Err(Error::Unavailable);
                };
                let destination = self
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .resume_targets
                    .get(&actor)
                    .copied()
                    .unwrap_or(to);
                let direction_from = if destination == to { from } else { to };
                let actual_duration = self
                    .state
                    .areas
                    .iter()
                    .find(|a| a.id == direction_from)
                    .and_then(|a| a.links.iter().find(|l| l.destination == destination))
                    .map(|l| l.duration)
                    .ok_or(Error::NoRoute)?;
                let leg = game_transport::retreat(
                    RoadPosition {
                        from,
                        to,
                        progress,
                        duration,
                    },
                    destination,
                    actual_duration,
                    self.state.time,
                )?;
                self.ensure_event_capacity(self.state.people.len() + 1)?;
                self.interrupt_actor_activity(actor);
                self.state.people[self.person_index[&actor]].location = Location::InTransit {
                    from: leg.from,
                    to: leg.to,
                    departed: leg.departed,
                    road_start: leg.road_start,
                    arrives: leg.arrives,
                };
                self.state
                    .due
                    .entry(leg.arrives)
                    .or_default()
                    .push(Due::Arrive(actor));
                self.emit(EventKind::Departed {
                    actor,
                    from: leg.from,
                    to: leg.to,
                    arrives: leg.arrives,
                });
                self.transport_departed(actor);
            }
            Action::ApproachRoad {
                from,
                to,
                progress,
                duration,
            } => self.transport_approach_road(
                actor,
                RoadPosition {
                    from: *from,
                    to: *to,
                    progress: *progress,
                    duration: *duration,
                },
            )?,
        }
        self.revision += 1;
        Ok(())
    }
    pub(crate) fn transport_view(&self, actor: PersonId) -> Option<View> {
        let w = self.state.transport.as_ref()?;
        let bodies = self.transport_bodies().ok()?;
        Some(View {
            mass: bodies.get(&actor)?.mass,
            carried_by: w
                .groups
                .get(&actor)
                .map_or_else(Vec::new, |g| g.bearers.iter().copied().collect()),
            passengers: w
                .groups
                .values()
                .filter(|g| g.bearers.contains(&actor))
                .map(|g| g.passenger)
                .collect(),
            body_load: self.transport_body_load(actor),
            offers: w
                .offers
                .values()
                .flat_map(|offers| offers.values())
                .filter(|o| {
                    (o.passenger == actor || o.bearer == actor) && o.expires > self.state.time
                })
                .cloned()
                .collect(),
            responses: w
                .responses
                .iter()
                .filter(|r| r.passenger == actor || r.bearer == actor)
                .cloned()
                .collect(),
        })
    }
    fn record_transport_response(
        &mut self,
        passenger: PersonId,
        bearer: PersonId,
        kind: game_transport::ResponseKind,
        reason: &str,
    ) {
        let world = self.state.transport.as_mut().unwrap();
        let response = game_transport::Response {
            passenger,
            bearer,
            kind,
            reason: reason.into(),
            observed_at: self.state.time,
        };
        if !world
            .responses
            .iter()
            .rev()
            .take(64)
            .any(|r| r.passenger == passenger && r.bearer == bearer && r.kind == kind)
        {
            world.responses.push(response);
            if world.responses.len() > 64 {
                world.responses.remove(0);
            }
        }
    }
    pub(crate) fn review_transport_choices(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.transport else {
            return Ok(());
        };
        let recipients: Vec<_> = world.offers.keys().copied().collect();
        for passenger in recipients {
            if passenger == self.state.ruler
                || !self.independent(passenger)
                || !self.state.physical.as_ref().unwrap().actors[&passenger]
                    .vitals
                    .alive
                || self.combat_restricts(passenger)
                || self.state.transport.as_ref().unwrap().busy(passenger)
            {
                continue;
            }
            let offers: Vec<_> = self
                .state
                .transport
                .as_ref()
                .unwrap()
                .offers
                .get(&passenger)
                .into_iter()
                .flat_map(|o| o.values())
                .filter(|o| {
                    o.expires > self.state.time && self.transport_same_place(passenger, o.bearer)
                })
                .cloned()
                .collect();
            if offers.is_empty() {
                continue;
            }
            let bearers: BTreeSet<_> = offers.iter().map(|o| o.bearer).collect();
            let bodies = self.transport_bodies()?;
            let mut candidate = self.state.transport.as_ref().unwrap().clone();
            let sufficient = candidate
                .begin_lift(
                    passenger,
                    bearers.clone(),
                    Permission {
                        passenger_accepted: true,
                        ..Permission::default()
                    },
                    self.state.time,
                    &bodies,
                )
                .is_ok();
            let mind = &self.state.decisions.as_ref().unwrap().minds[&passenger];
            let relation = mind
                .relationships
                .get(&offers[0].bearer)
                .cloned()
                .unwrap_or(decisions::Relationship {
                    trust: 0,
                    affection: 0,
                });
            let body = &self.state.physical.as_ref().unwrap().actors[&passenger];
            let rules = self.body_rules(passenger);
            let (kind, reason) = game_transport::judge_offer(game_transport::OfferChoice {
                need_help: body.vitals.health < rules.max_health
                    || body.vitals.fatigue >= rules.fatigue_limit / 3,
                urgent_own_need: body.vitals.hunger >= rules.hunger_harm,
                busy: !matches!(body.activity, Activity::Idle | Activity::Rest),
                trust: relation.trust,
                affection: relation.affection,
                independence_goal: mind.dream == decisions::Aspiration::Independence,
                sufficient_support: sufficient,
            });
            self.record_transport_response(passenger, offers[0].bearer, kind, reason);
            match kind {
                game_transport::ResponseKind::Accepted => self.transport_command(
                    passenger,
                    &Action::AcceptCarry {
                        bearers: bearers.into_iter().collect(),
                    },
                )?,
                game_transport::ResponseKind::Refused => {
                    for offer in offers {
                        self.transport_command(
                            passenger,
                            &Action::RefuseCarry {
                                bearer: offer.bearer,
                            },
                        )?;
                    }
                }
                game_transport::ResponseKind::WaitingForSupport => {
                    let expires = offers.iter().map(|o| o.expires).min().unwrap();
                    let observers: Vec<_> = self
                        .state
                        .people
                        .iter()
                        .map(|p| p.id)
                        .filter(|id| *id != passenger && self.transport_same_place(*id, passenger))
                        .take(64)
                        .collect();
                    for observer in observers {
                        let known = self
                            .state
                            .transport
                            .as_mut()
                            .unwrap()
                            .known_support
                            .entry(observer)
                            .or_default();
                        if known.len() < 64 || known.contains_key(&passenger) {
                            known.insert(
                                passenger,
                                game_transport::SupportRequest {
                                    passenger,
                                    requester: passenger,
                                    observed_at: self.state.time,
                                    expires,
                                },
                            );
                        }
                    }
                }
            }
        }
        let requests: Vec<_> = self
            .state
            .transport
            .as_ref()
            .unwrap()
            .known_support
            .iter()
            .flat_map(|(observer, requests)| {
                requests
                    .values()
                    .map(|request| (*observer, request.clone()))
            })
            .collect();
        for (actor, request) in requests {
            if actor == self.state.ruler
                || request.expires <= self.state.time
                || !self.transport_same_place(actor, request.passenger)
                || self
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .groups
                    .contains_key(&request.passenger)
                || self.state.transport.as_ref().unwrap().busy(actor)
                || self
                    .state
                    .transport
                    .as_ref()
                    .unwrap()
                    .offers
                    .get(&request.passenger)
                    .is_some_and(|o| o.contains_key(&actor))
            {
                continue;
            }
            let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
            let body = &self.state.physical.as_ref().unwrap().actors[&actor];
            let rules = self.body_rules(actor);
            let relation = mind
                .relationships
                .get(&request.requester)
                .cloned()
                .unwrap_or(decisions::Relationship {
                    trust: 0,
                    affection: 0,
                });
            let physical = self.transport_bodies()?;
            if game_transport::willing_support(game_transport::SupportChoice {
                empathy: mind.personality.empathy,
                trust: relation.trust,
                affection: relation.affection,
                own_need: body.vitals.hunger >= rules.meal_relief
                    || body.vitals.fatigue >= rules.fatigue_limit / 2,
                busy: !matches!(body.activity, Activity::Idle | Activity::Rest)
                    || self.lineage_handles(actor)
                    || self.health_actor_busy(actor)
                    || self.mission_actor_busy(actor),
                has_capacity: physical
                    .get(&actor)
                    .is_some_and(|b| b.can_bear && b.capacity.0 > b.cargo.0),
            }) {
                match self.transport_command(
                    actor,
                    &Action::JoinCarry {
                        passenger: request.passenger,
                    },
                ) {
                    Ok(()) | Err(Error::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }
    pub(crate) fn available_transport_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(w) = &self.state.transport else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if let Some(group) = w.groups.get(&actor) {
            if self.independent(actor) && !self.combat_restricts(actor) {
                actions.push(Action::PutDown {
                    passenger: group.passenger,
                });
            }
        } else if self.transport_is_moving(actor) {
            actions.push(Action::Stop);
        } else {
            if matches!(
                self.state.people[self.person_index[&actor]].location,
                Location::StoppedOnRoad { .. }
            ) && self.transport_can_depart(actor)
            {
                actions.push(Action::Resume);
            }
            if let Ok(bodies) = self.transport_bodies() {
                if bodies.get(&actor).is_some_and(|b| b.can_bear) {
                    for (id, body) in &bodies {
                        if *id != actor
                            && body.position == bodies[&actor].position
                            && !w.groups.contains_key(id)
                            && !self.transport_is_moving(*id)
                        {
                            actions.push(Action::OfferCarry { passenger: *id });
                        }
                    }
                }
                if let Some(offers) = w.offers.get(&actor) {
                    let bearers: Vec<_> = offers
                        .values()
                        .filter(|o| o.expires > self.state.time)
                        .map(|o| o.bearer)
                        .collect();
                    if !bearers.is_empty() {
                        actions.push(Action::AcceptCarry { bearers });
                    }
                    for bearer in offers.keys() {
                        actions.push(Action::RefuseCarry { bearer: *bearer });
                    }
                }
            }
        }
        for group in w.groups.values().filter(|g| g.bearers.contains(&actor)) {
            actions.push(Action::PutDown {
                passenger: group.passenger,
            });
        }
        actions
            .into_iter()
            .map(|action| Command::Transport { action })
            .collect()
    }
}
pub(crate) fn location_position(location: &Location, now: SimTime) -> Result<Position, Error> {
    match *location {
        Location::AtArea(area) => Ok(Position::Area(area)),
        Location::InTransit {
            from,
            to,
            departed,
            road_start,
            arrives,
        } => game_transport::Leg {
            from,
            to,
            departed,
            road_start,
            arrives,
        }
        .position(now),
        Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        } => Ok(Position::Road(RoadPosition {
            from,
            to,
            progress,
            duration,
        })),
        Location::Carried { .. } => Err(Error::Unavailable),
    }
}
fn position_location(position: Position) -> Location {
    match position {
        Position::Area(area) => Location::AtArea(area),
        Position::Road(RoadPosition {
            from,
            to,
            progress,
            duration,
        }) => Location::StoppedOnRoad {
            from,
            to,
            progress,
            duration,
        },
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(w) = &state.transport else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("invalid physical transport".into());
    let physical = state.physical.as_ref().ok_or_else(fail)?;
    let resolved = resolved_locations_for(state)?;
    let mut cargo_by_actor: BTreeMap<PersonId, u64> = BTreeMap::new();
    for stock in state.inventories.values() {
        if let InventoryLocation::Carried(actor) = stock.location {
            let quantity = cargo_by_actor.entry(actor).or_default();
            *quantity = quantity
                .checked_add(stock.quantity.0)
                .ok_or(Error::Overflow)?;
        }
    }
    let mut bodies = BTreeMap::new();
    for p in &state.people {
        let body = physical.actors.get(&p.id).ok_or_else(fail)?;
        let mass = if let Some(lives) = &state.development {
            game_transport::body_mass(
                &w.rules,
                SimDuration(
                    lives
                        .lives
                        .get(&p.id)
                        .ok_or_else(fail)?
                        .age_at(state.time)?,
                ),
                lives.rules.adulthood,
            )
        } else {
            w.rules.adult_mass
        };
        let cargo = cargo_by_actor.get(&p.id).copied().unwrap_or(0);
        let position = location_position(&resolved[&p.id], state.time).map_err(|_| fail())?;
        if let Location::Carried { carrier } = p.location {
            if !w.groups.get(&p.id).is_some_and(|g| g.anchor == carrier) || !p.route.is_empty() {
                return Err(fail());
            }
        } else if w.groups.contains_key(&p.id) {
            return Err(fail());
        }
        let can_walk = body.vitals.alive
            && body.vitals.health > 0
            && body.vitals.fatigue < physical.rules.fatigue_limit;
        bodies.insert(
            p.id,
            Body {
                position,
                mass,
                cargo: Quantity(cargo),
                capacity: physical::rules_for(state, p.id).carrying_capacity,
                can_hold: body.vitals.alive
                    && body.vitals.health > 0
                    && state
                        .combat
                        .as_ref()
                        .is_none_or(|w| body.vitals.health > w.rules.incapacitation),
                can_walk,
                can_bear: can_walk,
            },
        );
    }
    game_transport::validate(w, &bodies, state.time)?;
    for (id, lift) in &w.lifts {
        for bearer in &lift.bearers {
            if !matches!(physical.actors[bearer].activity,Activity::Lift {passenger,completes} if passenger==*id&&completes==lift.completes)
            {
                return Err(fail());
            }
        }
    }
    for (id, body) in &physical.actors {
        if let Activity::Lift {
            passenger,
            completes,
        } = body.activity
            && !w
                .lifts
                .get(&passenger)
                .is_some_and(|l| l.bearers.contains(id) && l.completes == completes)
        {
            return Err(fail());
        }
    }
    Ok(())
}

pub(crate) fn resolved_locations_for(
    state: &CanonicalState,
) -> Result<BTreeMap<PersonId, Location>, Error> {
    let catalog = state
        .people
        .iter()
        .map(|p| (p.id, p.location.clone()))
        .collect();
    game_transport::resolve_locations(&catalog)
}

#[cfg(test)]
mod connected_tests {
    use super::*;
    fn quiet(sim: &mut Simulation) {
        let citizens: Vec<_> = sim
            .state
            .people
            .iter()
            .filter(|p| p.role != Role::Guardian)
            .map(|p| p.id)
            .collect();
        for actor in citizens {
            let mut traits = decisions::Personality::restful();
            traits.empathy = 0;
            sim.debug_set_personality(actor, traits, decisions::Aspiration::Comfort)
                .unwrap();
        }
    }
    fn traveling_birth(workers: usize) -> (Simulation, PersonId, PersonId) {
        let mut sim = Simulation::transport_fixture(
            1,
            Execution {
                workers,
                reverse_jobs: workers > 1,
            },
        )
        .unwrap();
        sim.state.physical.as_mut().unwrap().rules.fatigue_limit = 2_000_000;
        sim.advance(SimDuration(120_000)).unwrap();
        let gestator = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
            .find(|p| matches!(p.state, development::PregnancyState::Gestating { .. }))
            .unwrap()
            .gestator;
        quiet(&mut sim);
        sim.advance(SimDuration(120_000)).unwrap();
        let predecessor = sim.state.ruler;
        sim.debug_injure(predecessor, sim.body_rules(predecessor).max_health)
            .unwrap();
        let routes = sim
            .state
            .areas
            .iter()
            .map(|area| {
                (
                    area.id,
                    area.links
                        .iter()
                        .map(|l| (l.destination, l.duration))
                        .collect(),
                )
            })
            .collect();
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&gestator)
            .unwrap()
            .routes = routes;
        sim.debug_move_actor(gestator, AreaId(3)).unwrap();
        sim.debug_move_actor(gestator, AreaId(2)).unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        let child = sim.state.ruler;
        assert_ne!(child, predecessor);
        (sim, gestator, child)
    }
    #[test]
    fn component_capacity_checks_match_the_full_body_snapshot() {
        let (sim, _, _) = traveling_birth(1);
        let world = sim.state.transport.as_ref().unwrap();
        let bodies = sim.transport_bodies().unwrap();
        let full = game_transport::capacity_plan(&world.groups, &bodies).unwrap();
        for actor in bodies.keys().copied() {
            for amount in [0, 500, 2_000, 20_000] {
                assert_eq!(
                    sim.transport_transfer_capacity(actor, amount),
                    game_transport::transfer_capacity(
                        &world.groups,
                        &bodies,
                        actor,
                        Quantity(amount)
                    )
                    .unwrap()
                );
            }
            let load = full
                .shares
                .iter()
                .filter(|((_, bearer), _)| *bearer == actor)
                .map(|(_, q)| q.0)
                .sum();
            assert_eq!(sim.transport_body_load(actor), Quantity(load));
        }
        assert!(!sim.transport_transfer_capacity(PersonId(u64::MAX), 1));
    }
    #[test]
    fn actual_newborn_follows_the_carrier_on_later_legs_without_own_arrival() {
        let (mut sim, mother, child) = traveling_birth(1);
        assert_eq!(
            sim.state.people[sim.person_index[&child]].location,
            Location::Carried { carrier: mother }
        );
        assert!(sim.observe().available_commands.iter().any(|c| matches!(
            c,
            Command::Signal {
                preference: ChildhoodInterest::Food
            }
        )));
        assert!(
            sim.state
                .due
                .values()
                .flatten()
                .all(|d| !matches!(d,Due::Arrive(id) if *id==child))
        );
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&child]
                .vitals
                .fatigue,
            0
        );
        sim.advance(SimDuration(150_000)).unwrap();
        assert!(matches!(
            sim.resolved_location(child),
            Location::InTransit {
                from: AreaId(3),
                to: AreaId(1),
                departed: SimTime(420_000),
                ..
            }
        ));
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&child]
                .vitals
                .fatigue,
            0
        );
        let loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(loaded.state, sim.state);
    }
    #[test]
    fn a_carrier_dies_at_an_exact_road_position_with_body_passenger_and_cargo_preserved() {
        let (mut sim, mother, child) = traveling_birth(1);
        let cargo = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Carried(mother),
                Quantity(1_000),
                Quantity(1_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        let hunger_harm = sim.body_rules(mother).hunger_harm;
        let body = sim
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&mother)
            .unwrap();
        body.vitals.hunger = hunger_harm;
        body.vitals.health = 10_000;
        sim.advance(SimDuration(10_000)).unwrap();
        assert!(
            !sim.state.physical.as_ref().unwrap().actors[&mother]
                .vitals
                .alive
        );
        let stopped = Location::StoppedOnRoad {
            from: AreaId(1),
            to: AreaId(3),
            progress: SimDuration(70_000),
            duration: SimDuration(180_000),
        };
        assert_eq!(
            sim.state.people[sim.person_index[&mother]].location,
            stopped
        );
        assert_eq!(sim.resolved_location(child), stopped);
        assert_eq!(sim.state.inventories[&cargo].quantity, Quantity(1_000));
        sim.advance(SimDuration(20_003)).unwrap();
        assert_eq!(sim.resolved_location(child), stopped);
        let loaded = Simulation::load(&sim.save().unwrap(), Execution::default()).unwrap();
        assert_eq!(loaded.state, sim.state);
    }
    #[test]
    fn real_timed_lift_requires_the_adult_passengers_personal_acceptance() {
        let mut sim = Simulation::transport_fixture(7, Execution::default()).unwrap();
        quiet(&mut sim);
        let carrier = sim.state.ruler;
        let passenger = sim.state.people[8].id;
        sim.transport_command(carrier, &Action::OfferCarry { passenger })
            .unwrap();
        assert!(sim.state.transport.as_ref().unwrap().groups.is_empty());
        assert!(sim.state.transport.as_ref().unwrap().lifts.is_empty());
        let before = sim.state.clone();
        assert_eq!(
            sim.transport_command(
                passenger,
                &Action::AcceptCarry {
                    bearers: vec![carrier, carrier]
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
        sim.transport_command(
            passenger,
            &Action::AcceptCarry {
                bearers: vec![carrier],
            },
        )
        .unwrap();
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(sim.state.transport.as_ref().unwrap().groups.is_empty());
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            sim.state.people[sim.person_index[&passenger]].location,
            Location::Carried { carrier }
        );
        assert_eq!(
            sim.state.transport.as_ref().unwrap().groups[&passenger].authority,
            Authority::Consented
        );
        sim.transport_command(passenger, &Action::PutDown { passenger })
            .unwrap();
        assert_eq!(
            sim.state.people[sim.person_index[&passenger]].location,
            Location::AtArea(AreaId(1))
        );
    }
    #[test]
    fn birth_transport_and_stopping_replay_identically_across_workers_and_time_slices() {
        let (mut whole, mother, child) = traveling_birth(1);
        let (mut split, _, _) = traveling_birth(4);
        whole.advance(SimDuration(70_003)).unwrap();
        for dt in [1, 19_999, 7_001, 43_002] {
            split.advance(SimDuration(dt)).unwrap();
            let _ = split.observe();
        }
        assert_eq!(whole.state, split.state);
        for sim in [&mut whole, &mut split] {
            sim.transport_command(mother, &Action::Stop).unwrap();
        }
        assert_eq!(
            whole.resolved_location(child),
            split.resolved_location(child)
        );
        whole.advance(SimDuration(11_007)).unwrap();
        for dt in [7_003, 4_004] {
            split.advance(SimDuration(dt)).unwrap();
        }
        assert_eq!(whole.state, split.state);
        let mut restored = Simulation::load(&split.save().unwrap(), Execution::default()).unwrap();
        split.transport_command(mother, &Action::Resume).unwrap();
        restored.transport_command(mother, &Action::Resume).unwrap();
        split.advance(SimDuration(19_003)).unwrap();
        restored.advance(SimDuration(19_003)).unwrap();
        assert_eq!(restored.state, split.state);
    }
    #[test]
    fn a_stopped_roadside_child_can_signal_and_receive_real_carried_food() {
        let (mut sim, mother, child) = traveling_birth(1);
        sim.advance(SimDuration(30_000)).unwrap();
        sim.transport_command(mother, &Action::Stop).unwrap();
        let source = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(mother),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        sim.debug_actor_command(
            child,
            Command::Signal {
                preference: ChildhoodInterest::Food,
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert!(
            matches!(
                sim.state.physical.as_ref().unwrap().actors[&child].activity,
                Activity::Idle | Activity::Rest
            ),
            "child primary action {:?}",
            sim.state.physical.as_ref().unwrap().actors[&child].activity
        );
        assert!(sim.transport_same_place(mother, child));
        assert!(sim.has_stock_access(mother, &sim.state.inventories[&source]));
        assert!(!sim.care_index.contains_key(&child));
        sim.debug_actor_command(mother, Command::Care { child, source })
            .unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        assert_eq!(sim.state.inventories[&source].quantity, Quantity(1_000));
        assert_eq!(
            sim.state.physical.as_ref().unwrap().actors[&child]
                .vitals
                .hunger,
            0
        );
        assert!(matches!(
            sim.resolved_location(child),
            Location::StoppedOnRoad { .. }
        ));
    }
    #[test]
    fn simultaneous_incoming_cargo_respects_the_entire_bearer_group() {
        let mut sim = Simulation::transport_fixture(23, Execution::default()).unwrap();
        quiet(&mut sim);
        sim.state.transport.as_mut().unwrap().rules.adult_mass = Quantity(16_000);
        let a = sim.state.ruler;
        let b = sim.state.people[7].id;
        let passenger = sim.state.people[8].id;
        for bearer in [a, b] {
            sim.transport_command(bearer, &Action::OfferCarry { passenger })
                .unwrap();
        }
        sim.transport_command(
            passenger,
            &Action::AcceptCarry {
                bearers: vec![a, b],
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let source = sim
            .debug_add_inventory(
                Good::Timber,
                InventoryLocation::Area(AreaId(1)),
                Quantity(6_000),
                Quantity(6_000),
                BTreeSet::from([a, b]),
            )
            .unwrap();
        let destinations: Vec<_> = [a, b]
            .into_iter()
            .map(|actor| {
                sim.debug_add_inventory(
                    Good::Timber,
                    InventoryLocation::Carried(actor),
                    Quantity(0),
                    Quantity(6_000),
                    BTreeSet::from([actor]),
                )
                .unwrap()
            })
            .collect();
        sim.debug_schedule_transfers(
            SimTime(12_000),
            vec![
                TransferIntent {
                    actor: a,
                    source,
                    destination: destinations[0],
                    quantity: Quantity(3_000),
                },
                TransferIntent {
                    actor: b,
                    source,
                    destination: destinations[1],
                    quantity: Quantity(3_000),
                },
            ],
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(sim.state.inventories[&source].quantity, Quantity(3_000));
        assert_eq!(
            sim.state.inventories[&destinations[0]].quantity,
            Quantity(3_000)
        );
        assert_eq!(
            sim.state.inventories[&destinations[1]].quantity,
            Quantity(0)
        );
        assert_eq!(
            sim.state.inventories[&source].quantity.0
                + destinations
                    .iter()
                    .map(|id| sim.state.inventories[id].quantity.0)
                    .sum::<u64>(),
            6_000
        );
    }
    #[test]
    fn road_observation_uses_real_supplies_and_local_distress_without_endpoint_knowledge() {
        let (mut sim, mother, child) = traveling_birth(1);
        sim.transport_command(mother, &Action::Stop).unwrap();
        let endpoint = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Area(AreaId(1)),
                Quantity(20_000),
                Quantity(20_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        let carried = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(mother),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([mother]),
            )
            .unwrap();
        sim.debug_actor_command(
            child,
            Command::Signal {
                preference: ChildhoodInterest::Food,
            },
        )
        .unwrap();
        sim.advance(SimDuration(6_000)).unwrap();
        let mut traits = decisions::Personality::restful();
        traits.empathy = 900;
        sim.debug_set_personality(mother, traits, decisions::Aspiration::Comfort)
            .unwrap();
        sim.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&mother)
            .unwrap()
            .vitals
            .hunger = 0;
        sim.review_decisions().unwrap();
        let mind = &sim.state.decisions.as_ref().unwrap().minds[&mother];
        assert!(!mind.knowledge.contains_key(&endpoint));
        assert_eq!(mind.knowledge[&carried].quantity, Quantity(2_000));
        assert!(matches!(
            mind.child_signals[&child].location,
            Location::StoppedOnRoad { .. }
        ));
        assert!(
            matches!(sim.state.physical.as_ref().unwrap().actors[&mother].activity,Activity::Care {child:target,source,..} if target==child&&source==carried)
        );
        sim.advance(SimDuration(60_000)).unwrap();
        assert_eq!(sim.state.inventories[&carried].quantity, Quantity(1_000));
        assert_eq!(sim.state.inventories[&endpoint].quantity, Quantity(20_000));
    }
    #[test]
    fn a_carried_child_in_battle_requires_the_anchors_actual_tactical_reach() {
        let (mut sim, mother, child) = traveling_birth(1);
        sim.state.people[sim.person_index[&mother]].route.clear();
        sim.state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&mother)
            .unwrap()
            .vitals
            .hunger = 0;
        sim.advance(SimDuration(120_000)).unwrap();
        let enemy = sim.state.people[8].id;
        let outsider = sim.state.people[9].id;
        for actor in [enemy, outsider] {
            sim.state.people[sim.person_index[&actor]].location = Location::AtArea(AreaId(3));
        }
        sim.debug_start_encounter(vec![(mother, 0), (enemy, 1)])
            .unwrap();
        assert!(sim.transport_same_place(mother, child));
        assert!(!sim.transport_same_place(outsider, child));
        let source = sim
            .debug_add_inventory(
                Good::Food,
                InventoryLocation::Carried(outsider),
                Quantity(2_000),
                Quantity(2_000),
                BTreeSet::from([outsider]),
            )
            .unwrap();
        let before = sim.state.clone();
        assert_eq!(
            sim.debug_actor_command(outsider, Command::Care { child, source }),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
    }
    #[test]
    fn an_actual_birth_beside_a_carried_gestator_uses_the_existing_willing_bearers() {
        let mut sim = Simulation::transport_fixture(1, Execution::default()).unwrap();
        sim.state.physical.as_mut().unwrap().rules.fatigue_limit = 2_000_000;
        sim.advance(SimDuration(120_000)).unwrap();
        let mother = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
            .find(|p| matches!(p.state, development::PregnancyState::Gestating { .. }))
            .unwrap()
            .gestator;
        quiet(&mut sim);
        let bearer = sim.state.ruler;
        sim.transport_command(bearer, &Action::OfferCarry { passenger: mother })
            .unwrap();
        sim.transport_command(
            mother,
            &Action::AcceptCarry {
                bearers: vec![bearer],
            },
        )
        .unwrap();
        sim.advance(SimDuration(120_000)).unwrap();
        sim.debug_move_actor(bearer, AreaId(3)).unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        let child = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
            .find_map(|p| {
                if let development::PregnancyState::Born(child) = p.state {
                    Some(child)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(
            sim.state.people[sim.person_index[&child]].location,
            Location::Carried { carrier: bearer }
        );
        assert_eq!(
            sim.state.transport.as_ref().unwrap().groups[&child].bearers,
            BTreeSet::from([bearer])
        );
        assert_eq!(sim.resolved_location(child), sim.resolved_location(mother));
        assert!(
            !sim.state.transport.as_ref().unwrap().groups[&child]
                .bearers
                .contains(&mother)
        );
        assert_eq!(
            sim.state.development.as_ref().unwrap().lives[&child].parents[0],
            mother
        );
    }
    #[test]
    fn unknown_public_transport_and_yield_targets_are_inert() {
        let mut sim = Simulation::transport_fixture(44, Execution::default()).unwrap();
        let before = sim.state.clone();
        for command in [
            Command::Yield {
                captor: PersonId(999),
            },
            Command::Transport {
                action: Action::OfferCarry {
                    passenger: PersonId(999),
                },
            },
        ] {
            assert_eq!(
                sim.submit(Submission {
                    caller_sequence: 1,
                    command
                }),
                Err(Error::Unavailable)
            );
            assert_eq!(sim.state, before);
        }
    }
    #[test]
    fn exhaustion_during_an_approach_preserves_the_known_point_through_rest_and_resume() {
        let mut sim = Simulation::transport_fixture(44, Execution::default()).unwrap();
        quiet(&mut sim);
        let actor = sim.state.ruler;
        sim.state.physical.as_mut().unwrap().rules.fatigue_limit = 100_000;
        let target = RoadPosition {
            from: AreaId(1),
            to: AreaId(3),
            progress: SimDuration(150_000),
            duration: SimDuration(180_000),
        };
        sim.transport_approach_road(actor, target).unwrap();
        sim.advance(SimDuration(100_000)).unwrap();
        assert_eq!(
            sim.transport_position(actor).unwrap(),
            Position::Road(RoadPosition {
                progress: SimDuration(100_000),
                ..target
            })
        );
        assert_eq!(
            sim.state.transport.as_ref().unwrap().paused_approaches[&actor],
            target
        );
        sim.debug_actor_command(actor, Command::Rest).unwrap();
        sim.advance(SimDuration(50_000)).unwrap();
        sim.transport_command(actor, &Action::Resume).unwrap();
        sim.advance(SimDuration(50_000)).unwrap();
        assert_eq!(sim.transport_position(actor), Some(Position::Road(target)));
        assert!(sim.state.transport.as_ref().unwrap().approaches.is_empty());
        assert_eq!(sim.state.time, SimTime(200_000));
    }
    #[test]
    fn public_offer_gets_a_personal_npc_response_and_a_real_timed_lift() {
        let mut sim = Simulation::transport_fixture(51, Execution::default()).unwrap();
        quiet(&mut sim);
        let ruler = sim.state.ruler;
        let passenger = sim.state.people[8].id;
        sim.debug_set_relationship(
            passenger,
            ruler,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.debug_injure(passenger, 120_000).unwrap();
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Transport {
                action: Action::OfferCarry { passenger },
            },
        })
        .unwrap();
        for _ in 0..12 {
            let _ = sim.observe();
        }
        assert!(sim.state.transport.as_ref().unwrap().lifts.is_empty());
        sim.advance(SimDuration(60_000)).unwrap();
        assert!(
            sim.state
                .transport
                .as_ref()
                .unwrap()
                .lifts
                .contains_key(&passenger)
        );
        assert!(
            sim.observe()
                .transport
                .unwrap()
                .responses
                .iter()
                .any(|r| r.passenger == passenger
                    && r.kind == game_transport::ResponseKind::Accepted)
        );
        sim.advance(SimDuration(5_999)).unwrap();
        assert!(sim.state.transport.as_ref().unwrap().groups.is_empty());
        sim.advance(SimDuration(1)).unwrap();
        assert_eq!(
            sim.state.people[sim.person_index[&passenger]].location,
            Location::Carried { carrier: ruler }
        );
        let declining = sim.state.people[9].id;
        sim.debug_set_personality(
            declining,
            decisions::Personality::restful(),
            decisions::Aspiration::Independence,
        )
        .unwrap();
        sim.debug_set_relationship(
            declining,
            ruler,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.submit(Submission {
            caller_sequence: 2,
            command: Command::Transport {
                action: Action::OfferCarry {
                    passenger: declining,
                },
            },
        })
        .unwrap();
        sim.advance(SimDuration(54_000)).unwrap();
        assert!(
            !sim.state
                .transport
                .as_ref()
                .unwrap()
                .groups
                .contains_key(&declining)
        );
        assert!(
            sim.observe().transport.unwrap().responses.iter().any(
                |r| r.passenger == declining && r.kind == game_transport::ResponseKind::Refused
            )
        );
    }
    #[test]
    fn public_heavy_person_offer_receives_bounded_voluntary_support() {
        let mut sim = Simulation::transport_fixture(51, Execution::default()).unwrap();
        quiet(&mut sim);
        sim.state.transport.as_mut().unwrap().rules.adult_mass = Quantity(16_000);
        sim.state.transport.as_mut().unwrap().rules.max_bearers = 2;
        let ruler = sim.state.ruler;
        let passenger = sim.state.people[8].id;
        let helper = sim.state.people[7].id;
        sim.debug_set_relationship(
            passenger,
            ruler,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.debug_injure(passenger, 120_000).unwrap();
        let mut personality = decisions::Personality::restful();
        personality.empathy = 900;
        sim.debug_set_personality(helper, personality, decisions::Aspiration::Comfort)
            .unwrap();
        sim.debug_set_relationship(
            helper,
            passenger,
            decisions::Relationship {
                trust: 500,
                affection: 500,
            },
        )
        .unwrap();
        sim.submit(Submission {
            caller_sequence: 1,
            command: Command::Transport {
                action: Action::OfferCarry { passenger },
            },
        })
        .unwrap();
        sim.advance(SimDuration(60_000)).unwrap();
        assert!(sim.state.transport.as_ref().unwrap().lifts.is_empty());
        assert_eq!(
            sim.state.transport.as_ref().unwrap().offers[&passenger]
                .keys()
                .copied()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([ruler, helper])
        );
        sim.advance(SimDuration(60_000)).unwrap();
        assert!(
            sim.state
                .transport
                .as_ref()
                .unwrap()
                .lifts
                .contains_key(&passenger)
        );
        sim.advance(SimDuration(6_000)).unwrap();
        assert_eq!(
            sim.state.transport.as_ref().unwrap().groups[&passenger].bearers,
            BTreeSet::from([ruler, helper])
        );
        assert!(sim.transport_body_load(ruler).0 > 0 && sim.transport_body_load(helper).0 > 0);
    }
}
