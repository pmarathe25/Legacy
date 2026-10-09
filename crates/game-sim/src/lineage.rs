//! Control follows the first actual eligible birth, without transferring an estate
//! or political office. Guardians sense that person's location and travel normally.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incarnation {
    pub person: PersonId,
    pub bound_at: SimTime,
    pub died_at: Option<SimTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waiting {
    pub predecessor: PersonId,
    pub died_at: SimTime,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianGoal {
    Waiting,
    Searching,
    Mentoring,
    Confined,
    Recovering,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guardian {
    pub target: Option<PersonId>,
    /// This connection conveys location only. It grants no stock, permissions,
    /// relationships, private intentions, or knowledge to other people.
    pub sensed_location: Option<Location>,
    pub sensed_at: SimTime,
    pub goal: GuardianGoal,
    pub explanation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub incarnations: Vec<Incarnation>,
    pub waiting: Option<Waiting>,
    pub guardians: BTreeMap<PersonId, Guardian>,
}

impl World {
    pub fn current(&self) -> Option<PersonId> {
        self.incarnations
            .last()
            .filter(|life| life.died_at.is_none())
            .map(|life| life.person)
    }
}

impl Simulation {
    pub fn lineage_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::combat_fixture(seed, execution)?;
        sim.state.scenario = scenarios()
            .into_iter()
            .find(|s| s.id == "lineage-fixture")
            .ok_or_else(|| Error::InvalidFixture("lineage fixture is not registered".into()))?;
        let ruler = sim.state.ruler;
        let location = sim.state.people[sim.person_index[&ruler]].location.clone();
        sim.state.lineage = Some(World {
            incarnations: vec![Incarnation {
                person: ruler,
                bound_at: sim.state.time,
                died_at: None,
            }],
            waiting: None,
            guardians: sim
                .state
                .people
                .iter()
                .filter(|p| p.role == Role::Guardian)
                .map(|p| {
                    (
                        p.id,
                        Guardian {
                            target: Some(ruler),
                            sensed_location: Some(location.clone()),
                            sensed_at: sim.state.time,
                            goal: GuardianGoal::Mentoring,
                            explanation:
                                "Assisting the current incarnation; readiness remains unassessed"
                                    .into(),
                        },
                    )
                })
                .collect(),
        });
        Ok(sim)
    }

    pub(crate) fn observe_lineage_deaths(&mut self) {
        let Some(current) = self.state.lineage.as_ref().and_then(World::current) else {
            return;
        };
        if self.state.physical.as_ref().unwrap().actors[&current]
            .vitals
            .alive
        {
            return;
        }
        let constraints: BTreeMap<_, _> = self
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .map(|id| (*id, self.guardian_constraint(*id)))
            .collect();
        let world = self.state.lineage.as_mut().unwrap();
        world.incarnations.last_mut().unwrap().died_at = Some(self.state.time);
        world.waiting = Some(Waiting {
            predecessor: current,
            died_at: self.state.time,
        });
        for (id, guardian) in &mut world.guardians {
            guardian.target = None;
            guardian.sensed_location = None;
            guardian.sensed_at = self.state.time;
            guardian.goal = constraints[id].unwrap_or(GuardianGoal::Waiting);
            guardian.explanation =
                "No living incarnation; waiting for an actual post-death birth".into();
        }
    }

    /// Called only after an actual birth has created the person and body. Birth
    /// deadlines are processed in persistent pregnancy order, so the first
    /// simultaneous eligible birth is deterministic across workers and saves.
    pub(crate) fn record_lineage_birth(&mut self, child: PersonId) {
        let Some(waiting) = self
            .state
            .lineage
            .as_ref()
            .and_then(|world| world.waiting.clone())
        else {
            return;
        };
        let Some(life) = self
            .state
            .development
            .as_ref()
            .and_then(|world| world.lives.get(&child))
        else {
            return;
        };
        if life.born != self.state.time
            || life.born <= waiting.died_at
            || life.initial_age != SimDuration(0)
            || life.parents.len() != 2
            || !self.state.physical.as_ref().unwrap().actors[&child]
                .vitals
                .alive
        {
            return;
        }
        let location = self.state.people[self.person_index[&child]]
            .location
            .clone();
        let constraints: BTreeMap<_, _> = self
            .state
            .lineage
            .as_ref()
            .unwrap()
            .guardians
            .keys()
            .map(|id| (*id, self.guardian_constraint(*id)))
            .collect();
        // Role::Ruler is the existing character-control tag. Neither this change
        // nor the lineage record grants claims, permissions, goods, or an office.
        self.state.people[self.person_index[&waiting.predecessor]].role = Role::Citizen;
        self.state.people[self.person_index[&child]].role = Role::Ruler;
        self.state.ruler = child;
        let world = self.state.lineage.as_mut().unwrap();
        world.incarnations.push(Incarnation {
            person: child,
            bound_at: self.state.time,
            died_at: None,
        });
        world.waiting = None;
        for (id, guardian) in &mut world.guardians {
            guardian.target = Some(child);
            guardian.sensed_location = Some(location.clone());
            guardian.sensed_at = self.state.time;
            guardian.goal = constraints[id].unwrap_or(GuardianGoal::Searching);
            guardian.explanation =
                "The innate connection locates the newborn; reaching them requires travel".into();
        }
    }

    pub(crate) fn lineage_handles(&self, person: PersonId) -> bool {
        self.state
            .lineage
            .as_ref()
            .is_some_and(|world| world.guardians.contains_key(&person))
    }

    fn guardian_constraint(&self, id: PersonId) -> Option<GuardianGoal> {
        if self
            .state
            .combat
            .as_ref()
            .is_some_and(|world| world.captivity.contains_key(&id))
        {
            Some(GuardianGoal::Confined)
        } else if self.state.physical.as_ref().unwrap().actors[&id]
            .vitals
            .health
            == 0
            || self.combat_restricts(id)
        {
            Some(GuardianGoal::Recovering)
        } else {
            None
        }
    }

    pub(crate) fn review_lineage(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.lineage else {
            return Ok(());
        };
        let current = world.current();
        let guardians: Vec<_> = world.guardians.keys().copied().collect();
        for id in guardians {
            let sensed = current.map(|person| {
                self.state.people[self.person_index[&person]]
                    .location
                    .clone()
            });
            let body = &self.state.physical.as_ref().unwrap().actors[&id];
            let own_location = self.state.people[self.person_index[&id]].location.clone();
            let (goal, explanation, command) = if self
                .state
                .combat
                .as_ref()
                .is_some_and(|w| w.captivity.contains_key(&id))
            {
                (
                    GuardianGoal::Confined,
                    "The body is confined; the connection does not release its restraints".into(),
                    None,
                )
            } else if body.vitals.health == 0 || self.combat_restricts(id) {
                (
                    GuardianGoal::Recovering,
                    "Recovering in place before resuming the lineage duty".into(),
                    None,
                )
            } else if self.in_combat(id) {
                (
                    GuardianGoal::Searching,
                    "The encounter must be resolved before the guardian can continue".into(),
                    None,
                )
            } else if let (Some(target), Some(target_location)) = (current, &sensed) {
                self.guardian_intention(id, target, &own_location, target_location)
            } else {
                (
                    GuardianGoal::Waiting,
                    "No living incarnation; other people and births continue normally".into(),
                    matches!(body.activity, Activity::Idle | Activity::Work { .. })
                        .then_some(Command::Rest),
                )
            };
            let guardian = self
                .state
                .lineage
                .as_mut()
                .unwrap()
                .guardians
                .get_mut(&id)
                .unwrap();
            guardian.target = current;
            guardian.sensed_location = sensed;
            guardian.sensed_at = self.state.time;
            guardian.goal = goal;
            guardian.explanation = explanation;
            if let Some(command) = command {
                let accepted = if let Command::Move { destination } = command {
                    self.start_movement(id, destination)
                } else {
                    self.debug_physical_command(id, command)
                };
                if accepted.is_err() {
                    self.state
                        .lineage
                        .as_mut()
                        .unwrap()
                        .guardians
                        .get_mut(&id)
                        .unwrap()
                        .explanation = "A physical attempt failed; the guardian will reconsider without abandoning the lineage".into();
                }
            }
        }
        Ok(())
    }

    fn guardian_intention(
        &self,
        guardian: PersonId,
        target: PersonId,
        own_location: &Location,
        target_location: &Location,
    ) -> (GuardianGoal, String, Option<Command>) {
        let body = &self.state.physical.as_ref().unwrap().actors[&guardian];
        if matches!(own_location, Location::InTransit { .. }) {
            return (
                GuardianGoal::Searching,
                "Traveling along the current physical route toward the sensed incarnation".into(),
                None,
            );
        }
        if own_location != target_location {
            let destination = match target_location {
                Location::AtArea(area) => *area,
                Location::InTransit { to, .. } => *to,
            };
            return (
                GuardianGoal::Searching,
                "The innate connection gives a destination; a known traversable route is still needed".into(),
                Some(Command::Move { destination }),
            );
        }
        if !matches!(
            body.activity,
            Activity::Idle | Activity::Rest | Activity::Work { .. }
        ) {
            return (
                GuardianGoal::Mentoring,
                "Completing the current physical assistance before reconsidering".into(),
                None,
            );
        }
        let Location::AtArea(area) = own_location else {
            unreachable!()
        };
        let mind = &self.state.decisions.as_ref().unwrap().minds[&guardian];
        // Needs and food access come from ordinary local observations. The
        // lineage connection does not reveal private body needs or distant food.
        if let Some(signal) = mind
            .child_signals
            .get(&target)
            .filter(|s| s.area == *area && s.observed_at == self.state.time)
        {
            if signal.urgency > 0 {
                if let Some(food) = mind.knowledge.values().find(|fact| {
                    fact.good == Good::Food
                        && fact.area == *area
                        && fact.accessible
                        && fact.quantity >= self.state.physical.as_ref().unwrap().rules.meal_size
                }) {
                    return (
                        GuardianGoal::Mentoring,
                        "Observed distress motivates care using food the guardian knows and can access".into(),
                        Some(Command::Care { child: target, source: food.inventory }),
                    );
                }
                return (
                    GuardianGoal::Mentoring,
                    "The child needs food, but no usable known supply is available here".into(),
                    None,
                );
            }
            if signal.preference == Some(ChildhoodInterest::Company) {
                return (
                    GuardianGoal::Mentoring,
                    "Responding to the child's observed request for company".into(),
                    Some(Command::Comfort { child: target }),
                );
            }
        }
        (
            GuardianGoal::Mentoring,
            "Present with the incarnation; readiness remains unassessed and no political authority is restored".into(),
            matches!(body.activity, Activity::Idle | Activity::Work { .. })
                .then_some(Command::Rest),
        )
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.lineage else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent lineage state".into());
    let Some(physical) = &state.physical else {
        return Err(fail());
    };
    let Some(development) = &state.development else {
        return Err(fail());
    };
    if state.decisions.is_none() || world.incarnations.is_empty() {
        return Err(fail());
    }
    let mut seen = BTreeSet::new();
    for (i, incarnation) in world.incarnations.iter().enumerate() {
        let life = development
            .lives
            .get(&incarnation.person)
            .ok_or_else(fail)?;
        let body = physical.actors.get(&incarnation.person).ok_or_else(fail)?;
        if !seen.insert(incarnation.person)
            || incarnation.bound_at > state.time
            || incarnation
                .died_at
                .is_some_and(|d| d < incarnation.bound_at || d > state.time)
            || incarnation.died_at.is_some() == body.vitals.alive
            || i + 1 < world.incarnations.len() && incarnation.died_at.is_none()
        {
            return Err(fail());
        }
        if let Some(died) = incarnation.died_at && !state.events.iter().any(|event| {
            event.time == died
                && matches!(event.kind, EventKind::Died { actor } if actor == incarnation.person)
        }) {
            return Err(fail());
        }
        if i > 0 {
            let previous_death = world.incarnations[i - 1].died_at.ok_or_else(fail)?;
            let first_eligible = development
                .lives
                .values()
                .filter(|candidate| {
                    candidate.born > previous_death
                        && candidate.initial_age == SimDuration(0)
                        && candidate.parents.len() == 2
                })
                .min_by_key(|candidate| (candidate.born, candidate.id))
                .map(|candidate| candidate.id);
            if life.born != incarnation.bound_at
                || life.born <= previous_death
                || life.initial_age != SimDuration(0)
                || life.parents.len() != 2
                || first_eligible != Some(incarnation.person)
            {
                return Err(fail());
            }
        }
    }
    let last = world.incarnations.last().unwrap();
    if last.person != state.ruler {
        return Err(fail());
    }
    match (&world.waiting, last.died_at) {
        (Some(waiting), Some(died))
            if waiting.predecessor == last.person && waiting.died_at == died => {}
        (None, None) => {}
        _ => return Err(fail()),
    }
    let expected_guardians: BTreeSet<_> = state
        .people
        .iter()
        .filter(|p| p.role == Role::Guardian)
        .map(|p| p.id)
        .collect();
    if world.guardians.keys().copied().collect::<BTreeSet<_>>() != expected_guardians {
        return Err(fail());
    }
    for guardian in world.guardians.values() {
        if guardian.target != world.current()
            || guardian.sensed_at > state.time
            || guardian.target.is_some() != guardian.sensed_location.is_some()
        {
            return Err(fail());
        }
        if let Some(location) = &guardian.sensed_location {
            let known_area = |id| state.areas.iter().any(|area| area.id == id);
            match location {
                Location::AtArea(area) if !known_area(*area) => return Err(fail()),
                Location::InTransit {
                    from,
                    to,
                    departed,
                    arrives,
                } if !known_area(*from)
                    || !known_area(*to)
                    || departed >= arrives
                    || *departed > guardian.sensed_at =>
                {
                    return Err(fail());
                }
                _ => {}
            }
        }
    }
    Ok(())
}
