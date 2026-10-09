//! Authoritative headless simulation. Debug fixtures are never player scenarios.
pub mod agreements;
#[doc(hidden)]
pub mod benchmark;
pub mod combat;
pub mod compute;
pub mod decisions;
pub mod development;
pub mod environment;
mod persistence;
pub mod physical;
pub mod session;
#[doc(hidden)]
pub mod transactions;

use game_api::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Execution {
    pub workers: usize,
    pub reverse_jobs: bool,
}
impl Default for Execution {
    fn default() -> Self {
        Self {
            workers: 1,
            reverse_jobs: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub evaluation_ns: u128,
    pub arbitration_ns: u128,
    pub commit_ns: u128,
    pub evaluated_people: u64,
    pub boundaries: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub destination: AreaId,
    pub duration: SimDuration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Area {
    pub id: AreaId,
    pub name: String,
    pub links: Vec<Link>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: PersonId,
    pub role: Role,
    pub location: Location,
    pub inventory: InventoryId,
    pub work_units: u64,
    pub work_remainder: u64,
    pub work_rate: u64,
    pub route: Vec<AreaId>,
    pub hunger_exempt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InventoryLocation {
    Area(AreaId),
    Carried(PersonId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    pub id: InventoryId,
    pub location: InventoryLocation,
    pub quantity: Quantity,
    pub capacity: Quantity,
    pub access: BTreeSet<PersonId>,
    pub last_granted: Option<PersonId>,
    pub good: Good,
    pub spoils_at: Option<SimTime>,
}

/// Explicit fixture intentions, not an interface for commanding citizens in play.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransferIntent {
    pub actor: PersonId,
    pub source: InventoryId,
    pub destination: InventoryId,
    pub quantity: Quantity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Due {
    Arrive(PersonId),
    Transfers(Vec<TransferIntent>),
    Review,
    Physical(PersonId),
    Spoil(InventoryId),
    Birth(PregnancyId),
    Age(PersonId),
    CombatRound(EncounterId),
}

/// Logical state in persistent ID order; excludes host publication counters and timings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalState {
    pub scenario: ScenarioInfo,
    pub seed: u64,
    pub time: SimTime,
    pub next_id: u64,
    pub ruler: PersonId,
    pub areas: Vec<Area>,
    // The common 10k-person storage spike favored typed rows over ECS adapters;
    // explicit commit boundaries also avoid converting persistent IDs to handles.
    pub people: Vec<Person>,
    pub inventories: BTreeMap<InventoryId, Inventory>,
    pub events: Vec<Event>,
    pub physical: Option<physical::World>,
    pub decisions: Option<decisions::World>,
    #[serde(default)]
    pub development: Option<development::World>,
    #[serde(default)]
    pub combat: Option<combat::World>,
    #[serde(default)]
    pub environment: Option<environment::World>,
    #[serde(default)]
    pub agreements: Option<agreements::World>,
    due: BTreeMap<SimTime, Vec<Due>>,
    accepted: BTreeMap<u64, (Command, CommandReceipt)>,
    next_command: u64,
    next_event: u64,
}

pub struct Simulation {
    state: CanonicalState,
    pool: rayon::ThreadPool,
    execution: Execution,
    revision: u64,
    profile: Profile,
    person_index: BTreeMap<PersonId, usize>,
    inventory_index: BTreeMap<InventoryLocation, Vec<InventoryId>>,
    pregnancy_index: BTreeMap<PersonId, PregnancyId>,
    care_index: BTreeMap<PersonId, PersonId>,
    dependent_index: BTreeMap<AreaId, BTreeSet<PersonId>>,
}

pub fn scenarios() -> Vec<ScenarioInfo> {
    vec![
        ScenarioInfo {
            id: "kernel-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "survival-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "autonomy-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "life-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "combat-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "childhood-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "farming-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "agreement-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
    ]
}

impl Simulation {
    pub fn new(scenario: &str, seed: u64, execution: Execution) -> Result<Self, Error> {
        if scenario == "agreement-fixture" {
            return Self::agreement_fixture(seed, execution);
        }
        if scenario == "farming-fixture" {
            return Self::farming_fixture(seed, execution);
        }
        if scenario == "survival-fixture" {
            return Self::survival_fixture(seed, execution);
        }
        if scenario == "autonomy-fixture" {
            return Self::autonomy_fixture(seed, execution);
        }
        if scenario == "childhood-fixture" {
            return Self::childhood_fixture(seed, execution);
        }
        if scenario == "combat-fixture" {
            return Self::combat_fixture(seed, execution);
        }
        if scenario == "life-fixture" {
            return Self::life_fixture(seed, execution);
        }
        if scenario != "kernel-fixture" {
            return Err(Error::InvalidConfiguration("unknown scenario".into()));
        }
        Self::kernel_fixture(seed, 15, execution)
    }

    pub fn kernel_fixture(
        seed: u64,
        population: usize,
        execution: Execution,
    ) -> Result<Self, Error> {
        if !(3..=100_000).contains(&population) {
            return Err(Error::InvalidConfiguration(
                "fixture population must be 3..=100000".into(),
            ));
        }
        let mut state = CanonicalState {
            scenario: scenarios().remove(0),
            seed,
            time: SimTime(0),
            next_id: 1,
            ruler: PersonId(0),
            areas: Vec::new(),
            people: Vec::new(),
            inventories: BTreeMap::new(),
            events: Vec::new(),
            due: BTreeMap::new(),
            accepted: BTreeMap::new(),
            next_command: 1,
            next_event: 1,
            physical: None,
            decisions: None,
            development: None,
            combat: None,
            environment: None,
            agreements: None,
        };
        for name in ["village", "farm", "forest"] {
            let id = AreaId(allocate(&mut state)?);
            state.areas.push(Area {
                id,
                name: name.into(),
                links: Vec::new(),
            });
        }
        for (a, b, minutes) in [(0, 1, 2), (0, 2, 3)] {
            let a_id = state.areas[a].id;
            let b_id = state.areas[b].id;
            state.areas[a].links.push(Link {
                destination: b_id,
                duration: SimDuration(minutes * 60_000),
            });
            state.areas[b].links.push(Link {
                destination: a_id,
                duration: SimDuration(minutes * 60_000),
            });
        }
        let village = state.areas[0].id;
        for i in 0..population {
            let id = PersonId(allocate(&mut state)?);
            let inventory = InventoryId(allocate(&mut state)?);
            let role = match i {
                0 => Role::Ruler,
                1 | 2 => Role::Guardian,
                _ => Role::Citizen,
            };
            if i == 0 {
                state.ruler = id;
            }
            state.people.push(Person {
                id,
                role,
                location: Location::AtArea(village),
                inventory,
                work_units: 0,
                work_remainder: 0,
                work_rate: 1_000 + i as u64 % 7,
                route: Vec::new(),
                hunger_exempt: role == Role::Guardian,
            });
            state.inventories.insert(
                inventory,
                Inventory {
                    id: inventory,
                    location: InventoryLocation::Carried(id),
                    quantity: Quantity(0),
                    capacity: Quantity(10_000),
                    access: BTreeSet::from([id]),
                    last_granted: None,
                    good: Good::Food,
                    spoils_at: None,
                },
            );
        }
        let store = InventoryId(allocate(&mut state)?);
        state.inventories.insert(
            store,
            Inventory {
                id: store,
                location: InventoryLocation::Area(village),
                quantity: Quantity(1_000),
                capacity: Quantity(1_000_000),
                access: state.people.iter().map(|p| p.id).collect(),
                last_granted: None,
                good: Good::Food,
                spoils_at: None,
            },
        );
        state.due.insert(SimTime(60_000), vec![Due::Review]);
        Self::from_state(state, execution, 0)
    }

    fn from_state(
        state: CanonicalState,
        execution: Execution,
        revision: u64,
    ) -> Result<Self, Error> {
        if !(1..=64).contains(&execution.workers) {
            return Err(Error::InvalidConfiguration(
                "worker budget must be 1..=64".into(),
            ));
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(execution.workers)
            .build()
            .map_err(|e| Error::InvalidConfiguration(e.to_string()))?;
        let person_index = state
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| (p.id, i))
            .collect();
        let mut inventory_index = BTreeMap::<_, Vec<_>>::new();
        for i in state.inventories.values() {
            inventory_index
                .entry(i.location.clone())
                .or_default()
                .push(i.id);
        }
        let pregnancy_index = state.development.as_ref().map_or_else(BTreeMap::new, |w| {
            w.pregnancies
                .values()
                .filter(|p| {
                    matches!(
                        p.state,
                        development::PregnancyState::Conceiving { .. }
                            | development::PregnancyState::Gestating { .. }
                    )
                })
                .map(|p| (p.gestator, p.id))
                .collect()
        });
        let care_index = state.physical.as_ref().map_or_else(BTreeMap::new, |w| {
            w.actors
                .iter()
                .filter_map(|(id, a)| {
                    if let Activity::Care { child, .. } | Activity::Comfort { child, .. } =
                        a.activity
                    {
                        Some((child, *id))
                    } else {
                        None
                    }
                })
                .collect()
        });
        let mut dependent_index = BTreeMap::<AreaId, BTreeSet<PersonId>>::new();
        if let Some(world) = &state.development {
            for p in &state.people {
                if let Location::AtArea(area) = p.location
                    && matches!(
                        world.lives[&p.id].stage,
                        LifeStage::Infant | LifeStage::Child
                    )
                    && state.physical.as_ref().unwrap().actors[&p.id].vitals.alive
                {
                    dependent_index.entry(area).or_default().insert(p.id);
                }
            }
        }
        let sim = Self {
            state,
            pool,
            execution,
            revision,
            profile: Profile::default(),
            person_index,
            inventory_index,
            pregnancy_index,
            care_index,
            dependent_index,
        };
        sim.validate_environment()?;
        Ok(sim)
    }

    pub fn canonical_state(&self) -> &CanonicalState {
        &self.state
    }
    pub fn profile(&self) -> &Profile {
        &self.profile
    }
    pub fn time(&self) -> SimTime {
        self.state.time
    }

    pub fn observe(&self) -> PlayerView {
        let ruler = &self.state.people[self.person_index[&self.state.ruler]];
        let origin = match ruler.location {
            Location::AtArea(a) => a,
            Location::InTransit { to, .. } => to,
        };
        let known_routes = self
            .state
            .decisions
            .as_ref()
            .map(|w| &w.minds[&ruler.id].routes);
        let known_area = |id: AreaId| {
            known_routes.is_none_or(|routes| {
                routes.contains_key(&id) || routes.values().any(|links| links.contains_key(&id))
            })
        };
        let mut available_commands: Vec<_> = self
            .state
            .areas
            .iter()
            .filter(|a| {
                a.id != origin
                    || matches!(ruler.location, Location::InTransit { .. })
                        && !ruler.route.is_empty()
            })
            .filter(|a| {
                known_routes.map_or_else(
                    || self.path(origin, a.id).is_ok(),
                    |routes| decisions::known_path(routes, origin, a.id).is_ok(),
                )
            })
            .map(|a| Command::Move { destination: a.id })
            .collect();
        if let Some(world) = &self.state.physical {
            let actor = &world.actors[&ruler.id];
            if !actor.vitals.alive || actor.vitals.health == 0 {
                available_commands.clear();
            } else if matches!(ruler.location, Location::AtArea(_)) {
                available_commands.extend([Command::Rest, Command::Cancel]);
                if let Some(view) = self.physical_view() {
                    available_commands.extend(
                        view.local_sites
                            .into_iter()
                            .map(|site| Command::Work { site }),
                    );
                    if !ruler.hunger_exempt {
                        available_commands.extend(
                            view.local_stocks
                                .iter()
                                .filter(|s| {
                                    s.can_access
                                        && s.good == Good::Food
                                        && s.quantity >= world.rules.meal_size
                                })
                                .map(|s| Command::Eat { source: s.id }),
                        );
                    }
                    for source in &view.local_stocks {
                        for destination in &view.local_stocks {
                            let carried = InventoryLocation::Carried(ruler.id);
                            if source.id != destination.id
                                && source.good == destination.good
                                && source.can_access
                                && destination.can_access
                                && source.quantity.0 >= 1_000
                                && (self.state.inventories[&source.id].location == carried
                                    || self.state.inventories[&destination.id].location == carried)
                            {
                                available_commands.push(Command::Load {
                                    source: source.id,
                                    destination: destination.id,
                                    quantity: Quantity(1_000),
                                });
                            }
                        }
                    }
                    if let Some(children) = self.dependent_index.get(&origin) {
                        for child in children {
                            if *child != ruler.id {
                                available_commands.push(Command::Comfort { child: *child });
                            }
                            for source in view.local_stocks.iter().filter(|s| {
                                s.good == Good::Food
                                    && s.can_access
                                    && s.quantity >= world.rules.meal_size
                            }) {
                                available_commands.push(Command::Care {
                                    child: *child,
                                    source: source.id,
                                });
                            }
                        }
                    }
                }
            }
        }
        if let Some(world) = &self.state.development {
            let stage = world.lives[&ruler.id].stage;
            if stage == LifeStage::Infant {
                available_commands.clear();
                if self.state.physical.as_ref().unwrap().actors[&ruler.id]
                    .vitals
                    .alive
                {
                    available_commands.push(Command::Cancel);
                }
            } else if stage == LifeStage::Child {
                available_commands.retain(|c| !matches!(c, Command::Work { .. }));
            }
            if matches!(stage, LifeStage::Infant | LifeStage::Child)
                && self.state.physical.as_ref().unwrap().actors[&ruler.id]
                    .vitals
                    .alive
                && self.state.physical.as_ref().unwrap().actors[&ruler.id]
                    .vitals
                    .health
                    > 0
                && matches!(ruler.location, Location::AtArea(_))
            {
                for preference in [
                    ChildhoodInterest::Food,
                    ChildhoodInterest::Company,
                    ChildhoodInterest::Movement,
                    ChildhoodInterest::Craft,
                ] {
                    if matches!(
                        preference,
                        ChildhoodInterest::Food | ChildhoodInterest::Company
                    ) {
                        available_commands.push(Command::Signal { preference });
                    }
                    if stage == LifeStage::Child {
                        available_commands.push(Command::Play {
                            interest: preference,
                        });
                    }
                }
            }
        }
        available_commands.extend(self.available_cultivation(ruler.id));
        available_commands.extend(self.available_agreement_commands(ruler.id));
        if self.in_combat(ruler.id) {
            available_commands.clear();
            if let Some(view) = self.tactical_view()
                && view.turn == Some(ruler.id)
            {
                for action in [
                    CombatAction::Wait,
                    CombatAction::Defend,
                    CombatAction::Surrender,
                ] {
                    available_commands.push(Command::CombatTurn {
                        encounter: view.encounter,
                        movement: None,
                        action,
                    });
                }
                for fighter in view.fighters.iter().filter(|f| f.actor != ruler.id) {
                    available_commands.push(Command::CombatTurn {
                        encounter: view.encounter,
                        movement: None,
                        action: CombatAction::Attack {
                            target: fighter.actor,
                        },
                    });
                    if fighter.incapacitated || fighter.surrendered {
                        available_commands.push(Command::CombatTurn {
                            encounter: view.encounter,
                            movement: None,
                            action: CombatAction::Capture {
                                target: fighter.actor,
                            },
                        });
                    }
                }
            }
        } else if self.combat_restricts(ruler.id) {
            available_commands.retain(|c| {
                matches!(
                    c,
                    Command::Rest | Command::Eat { .. } | Command::Tell { .. }
                )
            });
        }
        if let Some(world) = &self.state.combat {
            for target in world.captivity.keys() {
                if self.can_release(ruler.id, *target) {
                    available_commands.push(Command::FreeCaptive { target: *target });
                }
            }
        }
        PlayerView {
            agreements: self.agreement_view(ruler.id),
            environment: self.environment_view(ruler.id),
            revision: self.revision,
            time: self.state.time,
            ruler: ruler.id,
            location: ruler.location.clone(),
            known_areas: self
                .state
                .areas
                .iter()
                .filter(|a| known_area(a.id))
                .map(|a| AreaView {
                    id: a.id,
                    name: a.name.clone(),
                })
                .collect(),
            available_commands,
            physical: self.physical_view(),
            known_stocks: self.state.decisions.as_ref().map_or_else(Vec::new, |w| {
                w.minds[&ruler.id].knowledge.values().cloned().collect()
            }),
            combat: self.tactical_view(),
            life_stage: self
                .state
                .development
                .as_ref()
                .map(|w| w.lives[&ruler.id].stage),
            preference: self.state.development.as_ref().and_then(|w| {
                w.lives[&ruler.id]
                    .preference
                    .filter(|(_, until)| *until >= self.state.time)
                    .map(|(p, _)| p)
            }),
            interests: self.state.decisions.as_ref().map_or_else(Vec::new, |w| {
                w.minds[&ruler.id]
                    .interests
                    .iter()
                    .map(|(i, p)| (*i, *p))
                    .collect()
            }),
            nearby_people: self.nearby_people(0, 64),
            nearby_life_stages: self.state.development.as_ref().map_or_else(Vec::new, |w| {
                self.nearby_people(0, 64)
                    .into_iter()
                    .map(|(id, _)| (id, w.lives[&id].stage))
                    .collect()
            }),
        }
    }

    pub fn nearby_people(&self, offset: usize, limit: usize) -> Vec<(PersonId, Role)> {
        let ruler = &self.state.people[self.person_index[&self.state.ruler]];
        if self
            .state
            .physical
            .as_ref()
            .is_some_and(|w| !w.actors[&ruler.id].vitals.alive)
        {
            return Vec::new();
        }
        self.state.people.iter().filter(|p| p.id != ruler.id && matches!((&p.location, &ruler.location), (Location::AtArea(a), Location::AtArea(b)) if a == b))
            .skip(offset).take(limit.min(100)).map(|p| (p.id, p.role)).collect()
    }

    pub fn submit(&mut self, submission: Submission) -> Result<CommandReceipt, Error> {
        if let Some((command, receipt)) = self.state.accepted.get(&submission.caller_sequence) {
            return if command == &submission.command {
                Ok(receipt.clone())
            } else {
                Err(Error::SequenceConflict)
            };
        }
        if matches!(
            submission.command,
            Command::Engage { .. } | Command::CombatTurn { .. }
        ) {
            return self.submit_combat(submission);
        }
        if !matches!(submission.command, Command::Move { .. }) {
            return self.submit_physical(submission);
        }
        let Command::Move { destination } = submission.command else {
            unreachable!()
        };
        let next = self
            .state
            .next_command
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        self.start_movement(self.state.ruler, destination)?;
        let receipt = CommandReceipt {
            caller_sequence: submission.caller_sequence,
            authoritative_sequence: self.state.next_command,
            accepted_at: self.state.time,
        };
        self.state.next_command = next;
        self.state.accepted.insert(
            submission.caller_sequence,
            (submission.command, receipt.clone()),
        );
        Ok(receipt)
    }

    /// Scenario control, separate from the player's authority over the ruler.
    pub fn debug_move_actor(&mut self, actor: PersonId, destination: AreaId) -> Result<(), Error> {
        self.start_movement(actor, destination)
    }

    pub(crate) fn start_movement(
        &mut self,
        actor: PersonId,
        destination: AreaId,
    ) -> Result<(), Error> {
        let index = *self.person_index.get(&actor).ok_or(Error::Unavailable)?;
        if self.combat_restricts(actor) {
            return Err(Error::Unavailable);
        }
        if !self.independent(actor)
            || self.state.physical.as_ref().is_some_and(|w| {
                !w.actors[&actor].vitals.alive || w.actors[&actor].vitals.health == 0
            })
        {
            return Err(Error::Unavailable);
        }
        if !self.state.areas.iter().any(|a| a.id == destination) {
            return Err(Error::UnknownArea);
        }
        let origin = match self.state.people[index].location {
            Location::AtArea(a) => a,
            Location::InTransit { to, .. } => to,
        };
        let path = if let Some(world) = &self.state.decisions {
            decisions::known_path(&world.minds[&actor].routes, origin, destination)?.0
        } else {
            self.path(origin, destination)?
        };
        if path.is_empty()
            && (matches!(self.state.people[index].location, Location::AtArea(_))
                || self.state.people[index].route.is_empty())
        {
            return Err(Error::AlreadyThere);
        }
        let departure = if matches!(self.state.people[index].location, Location::AtArea(_)) {
            Some(self.departure(origin, path[0])?)
        } else {
            None
        };
        self.ensure_event_capacity(
            usize::from(departure.is_some()) + usize::from(self.state.development.is_some()),
        )?;
        self.interrupt_physical(actor);
        self.state.people[index].route = path;
        if let Some((to, arrives)) = departure {
            self.depart(index, origin, to, arrives);
        }
        self.revision += 1;
        Ok(())
    }

    fn path(&self, origin: AreaId, destination: AreaId) -> Result<Vec<AreaId>, Error> {
        let mut frontier = BTreeSet::from([(0_u64, Vec::<AreaId>::new(), origin)]);
        let mut visited = BTreeSet::new();
        while let Some((cost, path, area)) = frontier.pop_first() {
            if !visited.insert(area) {
                continue;
            }
            if area == destination {
                return Ok(path);
            }
            let record = self
                .state
                .areas
                .iter()
                .find(|a| a.id == area)
                .ok_or(Error::NoRoute)?;
            for link in &record.links {
                let mut next = path.clone();
                next.push(link.destination);
                frontier.insert((
                    cost.checked_add(link.duration.0).ok_or(Error::Overflow)?,
                    next,
                    link.destination,
                ));
            }
        }
        Err(Error::NoRoute)
    }

    fn departure(&self, from: AreaId, to: AreaId) -> Result<(AreaId, SimTime), Error> {
        let link = self
            .state
            .areas
            .iter()
            .find(|a| a.id == from)
            .and_then(|a| a.links.iter().find(|l| l.destination == to))
            .ok_or(Error::NoRoute)?;
        Ok((
            to,
            SimTime(
                self.state
                    .time
                    .0
                    .checked_add(link.duration.0)
                    .ok_or(Error::Overflow)?,
            ),
        ))
    }

    fn depart(&mut self, index: usize, from: AreaId, to: AreaId, arrives: SimTime) {
        let actor = self.state.people[index].id;
        if let Some(people) = self.dependent_index.get_mut(&from) {
            people.remove(&actor);
        }
        self.state.people[index].route.remove(0);
        self.state.people[index].location = Location::InTransit {
            from,
            to,
            departed: self.state.time,
            arrives,
        };
        self.state
            .due
            .entry(arrives)
            .or_default()
            .push(Due::Arrive(actor));
        self.emit(EventKind::Departed {
            actor,
            from,
            to,
            arrives,
        });
    }

    pub fn advance(&mut self, duration: SimDuration) -> Result<AdvanceResult, Error> {
        let requested = SimTime(
            self.state
                .time
                .0
                .checked_add(duration.0)
                .ok_or(Error::Overflow)?,
        );
        while self.state.time < requested {
            if let Some(encounter) = self.awaiting_combat_input() {
                return Ok(AdvanceResult {
                    requested,
                    reached: self.state.time,
                    revision: self.revision,
                    reason: StopReason::CombatInput { encounter },
                });
            }
            let next = self
                .state
                .due
                .first_key_value()
                .map(|(t, _)| *t)
                .unwrap_or(requested)
                .min(requested);
            let next = next.min(self.next_agreement_deadline().unwrap_or(next));
            let next = self.environment_boundary(self.physical_boundary(next));
            self.step(next)?;
        }
        Ok(AdvanceResult {
            requested,
            reached: self.state.time,
            revision: self.revision,
            reason: StopReason::TargetReached,
        })
    }

    fn step(&mut self, next: SimTime) -> Result<(), Error> {
        let started = Instant::now();
        let elapsed = next.0 - self.state.time.0;
        let input: Vec<_> = self
            .state
            .people
            .iter()
            .map(|p| compute::ProgressInput {
                units: p.work_units,
                remainder: p.work_remainder,
                units_per_minute: if matches!(p.location, Location::AtArea(_))
                    && self.physically_working(p.id)
                {
                    p.work_rate
                } else {
                    0
                },
                elapsed_ms: elapsed,
            })
            .collect();
        let output = compute::evaluate(&self.pool, &input, self.execution.reverse_jobs)?;
        let physical_output = self.evaluate_physical(elapsed)?;
        let environment_output = self.evaluate_environment(next)?;
        self.profile.evaluation_ns += started.elapsed().as_nanos();
        // Preflight boundary arithmetic before publishing any part of this interval.
        let mut due = self.state.due.get(&next).cloned().unwrap_or_default();
        due.sort_by_key(|d| match d {
            Due::Spoil(id) => (0, id.0),
            Due::Arrive(id) => (1, id.0),
            Due::Physical(id) => (2, id.0),
            Due::Review => (3, 0),
            Due::Transfers(_) => (4, 0),
            Due::Age(id) => (5, id.0),
            Due::Birth(id) => (6, id.0),
            Due::CombatRound(id) => (7, id.0),
        });
        for event in &due {
            match event {
                Due::Review => {
                    next.0
                        .checked_add(if self.state.decisions.is_some() {
                            120_000
                        } else {
                            60_000
                        })
                        .ok_or(Error::Overflow)?;
                }
                Due::Arrive(id) => {
                    let p = &self.state.people[self.person_index[id]];
                    if let Some(to) = p.route.first() {
                        let Location::InTransit { to: from, .. } = p.location else {
                            return Err(Error::InvalidSave("arrival outside transit".into()));
                        };
                        let duration = self
                            .state
                            .areas
                            .iter()
                            .find(|a| a.id == from)
                            .and_then(|a| a.links.iter().find(|l| l.destination == *to))
                            .ok_or(Error::NoRoute)?
                            .duration
                            .0;
                        next.0.checked_add(duration).ok_or(Error::Overflow)?;
                    }
                }
                _ => {}
            }
        }
        self.preflight_development(&due, next)?;
        self.preflight_combat(&due, next)?;
        self.preflight_agreements()?;
        let event_bound: usize = due
            .iter()
            .map(|d| match d {
                Due::Arrive(_) => 2,
                Due::Transfers(v) => v.len(),
                Due::Review => 0,
                Due::Physical(_) | Due::Spoil(_) => 4,
                Due::Birth(_) | Due::Age(_) => 2,
                Due::CombatRound(_) => self.state.people.len() * 8,
            })
            .sum::<usize>()
            + self.state.people.len() * 2
            + self
                .state
                .environment
                .as_ref()
                .map_or(0, |w| w.fields.len());
        self.ensure_event_capacity(event_bound)?;
        let started = Instant::now();
        let lifecycle: Vec<_> = due
            .iter()
            .filter(|d| matches!(d, Due::Age(_) | Due::Birth(_) | Due::CombatRound(_)))
            .cloned()
            .collect();
        let transfers: Vec<_> = due
            .iter()
            .filter_map(|d| {
                if let Due::Transfers(v) = d {
                    Some(v.as_slice())
                } else {
                    None
                }
            })
            .flatten()
            .cloned()
            .collect();
        let opening_stock: BTreeMap<_, _> = transfers
            .iter()
            .map(|i| (i.source, self.state.inventories[&i.source].quantity.0))
            .collect();
        self.profile.arbitration_ns += started.elapsed().as_nanos();
        let started = Instant::now();
        for (p, out) in self.state.people.iter_mut().zip(output) {
            p.work_units = out.units;
            p.work_remainder = out.remainder;
        }
        self.state.time = next;
        let actual_work: Vec<_> = self.state.physical.as_ref().map_or_else(Vec::new, |w| {
            w.actors
                .iter()
                .filter_map(|(actor, body)| {
                    if let Activity::Work { site } = body.activity {
                        Some((*actor, site))
                    } else {
                        None
                    }
                })
                .collect()
        });
        for (actor, site) in actual_work {
            self.record_agreement_work(actor, site, elapsed);
        }
        self.commit_environment(environment_output);
        self.state.due.remove(&next);
        for event in due {
            match event {
                Due::Review => self
                    .state
                    .due
                    .entry(SimTime(next.0 + 60_000))
                    .or_default()
                    .push(Due::Review),
                Due::Arrive(id) => {
                    let index = self.person_index[&id];
                    if let Location::InTransit { to, .. } = self.state.people[index].location {
                        self.state.people[index].location = Location::AtArea(to);
                        self.index_dependent(id, to);
                        self.emit(EventKind::Arrived {
                            actor: id,
                            area: to,
                        });
                        if let Some(destination) = self.state.people[index].route.first().copied() {
                            let (_, arrival) = self.departure(to, destination)?;
                            self.depart(index, to, destination, arrival);
                        }
                    }
                }
                Due::Transfers(_) => {}
                Due::Age(_) | Due::Birth(_) | Due::CombatRound(_) => {}
                Due::Physical(actor) => self.complete_physical(actor),
                Due::Spoil(inventory) => {
                    let stock = self.state.inventories.get_mut(&inventory).unwrap();
                    let quantity = stock.quantity;
                    stock.quantity = Quantity(0);
                    stock.spoils_at = None;
                    self.emit(EventKind::Spoiled {
                        inventory,
                        quantity,
                    });
                }
            }
        }
        let arbitration_started = Instant::now();
        let resolved = self.resolve_transfers_from(transfers, Some(&opening_stock));
        self.profile.arbitration_ns += arbitration_started.elapsed().as_nanos();
        for (intent, granted) in resolved {
            if granted {
                self.state
                    .inventories
                    .get_mut(&intent.source)
                    .unwrap()
                    .quantity
                    .0 -= intent.quantity.0;
                let source = self.state.inventories.get_mut(&intent.source).unwrap();
                source.last_granted = Some(intent.actor);
                self.state
                    .inventories
                    .get_mut(&intent.destination)
                    .unwrap()
                    .quantity
                    .0 += intent.quantity.0;
                self.emit(EventKind::Transferred {
                    actor: intent.actor,
                    source: intent.source,
                    destination: intent.destination,
                    quantity: intent.quantity,
                });
            } else {
                self.emit(EventKind::TransferFailed {
                    actor: intent.actor,
                    source: intent.source,
                });
            }
        }
        self.commit_physical(physical_output);
        for event in lifecycle {
            if let Due::CombatRound(encounter) = event {
                self.finish_combat_round(encounter)?;
            } else {
                self.complete_development(event);
            }
        }
        self.reconcile_agreements()?;
        self.learn_from_work();
        if self.state.time.0.is_multiple_of(60_000) {
            self.review_decisions()?;
        }
        self.revision += 1;
        self.profile.commit_ns += started.elapsed().as_nanos();
        self.profile.evaluated_people += input.len() as u64;
        self.profile.boundaries += 1;
        Ok(())
    }

    fn inventory_area(&self, inventory: &Inventory) -> Option<AreaId> {
        match inventory.location {
            InventoryLocation::Area(a) => Some(a),
            InventoryLocation::Carried(id) => {
                match self.state.people[self.person_index[&id]].location {
                    Location::AtArea(a) => Some(a),
                    _ => None,
                }
            }
        }
    }

    fn resolve_transfers(&self, intents: Vec<TransferIntent>) -> Vec<(TransferIntent, bool)> {
        self.resolve_transfers_from(intents, None)
    }

    fn resolve_transfers_from(
        &self,
        intents: Vec<TransferIntent>,
        opening_stock: Option<&BTreeMap<InventoryId, u64>>,
    ) -> Vec<(TransferIntent, bool)> {
        if intents.is_empty() {
            return Vec::new();
        }
        let balances = self
            .state
            .inventories
            .iter()
            .map(|(id, i)| {
                (
                    *id,
                    transactions::Balance {
                        stock: opening_stock
                            .and_then(|s| s.get(id))
                            .copied()
                            .unwrap_or(i.quantity.0)
                            .min(i.quantity.0),
                        capacity: i.capacity.0,
                        last_granted: i.last_granted,
                    },
                )
            })
            .collect();
        let mut carriers = BTreeMap::new();
        let mut carried = BTreeMap::<PersonId, u64>::new();
        if self.state.physical.is_some() {
            for i in self.state.inventories.values() {
                if let InventoryLocation::Carried(id) = i.location {
                    carriers.insert(i.id, id);
                    *carried.entry(id).or_default() += i.quantity.0;
                }
            }
        }
        let capacity = self
            .state
            .physical
            .as_ref()
            .map_or(0, |w| w.rules.carrying_capacity.0);
        let free_capacity = carried
            .into_iter()
            .map(|(id, qty)| (id, capacity.saturating_sub(qty)))
            .collect();
        let requests = intents
            .into_iter()
            .map(|i| {
                let source = &self.state.inventories[&i.source];
                let destination = &self.state.inventories[&i.destination];
                let actor = &self.state.people[self.person_index[&i.actor]];
                let area = if let Location::AtArea(a) = actor.location {
                    Some(a)
                } else {
                    None
                };
                transactions::Request {
                    actor: i.actor,
                    source: i.source,
                    destination: i.destination,
                    amount: i.quantity.0,
                    eligible: source.good == destination.good
                        && self.state.physical.as_ref().is_none_or(|w| {
                            w.actors[&i.actor].vitals.alive && w.actors[&i.actor].vitals.health > 0
                        })
                        && source.access.contains(&i.actor)
                        && (destination.access.contains(&i.actor)
                            || self.agreement_accepts_delivery(i.actor, i.destination, i.quantity))
                        && area.is_some()
                        && area == self.inventory_area(source)
                        && area == self.inventory_area(destination),
                }
            })
            .collect();
        transactions::arbitrate_with_capacity(&balances, requests, &carriers, &free_capacity)
            .into_iter()
            .map(|(r, granted)| {
                (
                    TransferIntent {
                        actor: r.actor,
                        source: r.source,
                        destination: r.destination,
                        quantity: Quantity(r.amount),
                    },
                    granted,
                )
            })
            .collect()
    }

    pub fn debug_schedule_transfers(
        &mut self,
        at: SimTime,
        intents: Vec<TransferIntent>,
    ) -> Result<(), Error> {
        if at <= self.state.time {
            return Err(Error::InvalidFixture(
                "transfers must be scheduled in the future".into(),
            ));
        }
        for i in &intents {
            if i.quantity.0 == 0
                || i.source == i.destination
                || !self.person_index.contains_key(&i.actor)
                || !self.state.inventories.contains_key(&i.source)
                || !self.state.inventories.contains_key(&i.destination)
            {
                return Err(Error::InvalidFixture(
                    "invalid transfer identity or amount".into(),
                ));
            }
        }
        self.state
            .due
            .entry(at)
            .or_default()
            .push(Due::Transfers(intents));
        self.revision += 1;
        Ok(())
    }

    pub fn debug_add_disconnected_area(&mut self, name: &str) -> Result<AreaId, Error> {
        let id = AreaId(allocate(&mut self.state)?);
        self.state.areas.push(Area {
            id,
            name: name.into(),
            links: Vec::new(),
        });
        Ok(id)
    }

    fn ensure_event_capacity(&self, count: usize) -> Result<(), Error> {
        self.state
            .next_event
            .checked_add(count as u64)
            .ok_or(Error::Overflow)
            .map(|_| ())
    }
    fn emit(&mut self, kind: EventKind) {
        let id = EventId(self.state.next_event);
        self.state.next_event += 1;
        self.state.events.push(Event {
            id,
            time: self.state.time,
            kind,
        });
    }
}

fn allocate(state: &mut CanonicalState) -> Result<u64, Error> {
    let id = state.next_id;
    state.next_id = id.checked_add(1).ok_or(Error::Overflow)?;
    Ok(id)
}
