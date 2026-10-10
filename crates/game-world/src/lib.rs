//! Authoritative headless simulation. Debug fixtures are never player scenarios.
pub mod agreements;
pub mod agriculture;
pub mod armies;
#[doc(hidden)]
pub mod benchmark;
pub mod combat;
mod commands;
pub mod compute;
pub mod decisions;
pub mod development;
pub mod ecology;
pub mod enforcement;
mod engine_adapter;
pub mod environment;
mod equipment;
pub mod force;
pub mod genetics;
mod guardian_delivery;
pub mod health;
mod host;
pub mod institutions;
pub mod invocation;
pub mod lineage;
mod logistics;
pub mod missions;
mod persistence;
pub mod physical;
pub mod population;
pub mod projects;
pub mod property;
mod quantities;
pub mod realm;
pub mod scenarios;
pub mod skills;
pub mod social;
pub mod transport;

use game_api::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
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
    pub command_attempts: BTreeMap<&'static str, u64>,
    pub phases_ns: BTreeMap<&'static str, u128>,
    pub evaluation_ns: u128,
    pub arbitration_ns: u128,
    pub commit_ns: u128,
    pub evaluated_people: u64,
    pub boundaries: u64,
}

impl Profile {
    fn record_phase(&mut self, phase: &'static str, started: Instant) {
        *self.phases_ns.entry(phase).or_default() += started.elapsed().as_nanos();
    }
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

pub use game_cargo::{Inventory, InventoryLocation};

/// Explicit fixture intentions, not an interface for commanding citizens in play.
pub use game_cargo::TransferIntent;

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
    pub armies: Option<armies::World>,
    pub realm: Option<realm::World>,
    pub transport: Option<game_transport::World>,
    pub genetics: Option<game_genetics::World>,
    pub physical_traits: Option<game_physiology::traits::World>,
    pub temperament: Option<game_characters::temperament::World>,
    pub heredity: Option<game_lifecycle::heredity::Definition>,
    pub content: BTreeMap<String, game_content::Stamp>,
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
    pub agriculture: Option<game_agriculture::World>,
    #[serde(default)]
    pub agreements: Option<agreements::World>,
    #[serde(default)]
    pub lineage: Option<lineage::World>,
    #[serde(default)]
    pub property: Option<property::World>,
    #[serde(default)]
    pub institutions: Option<institutions::World>,
    #[serde(default)]
    pub skills: Option<skills::World>,
    #[serde(default)]
    pub projects: Option<projects::World>,
    #[serde(default)]
    pub invocation: Option<invocation::World>,
    #[serde(default)]
    pub force: Option<force::World>,
    #[serde(default)]
    pub missions: Option<missions::World>,
    #[serde(default)]
    pub ecology: Option<ecology::World>,
    #[serde(default)]
    pub health: Option<health::World>,
    #[serde(default)]
    pub social: Option<social::World>,
    #[serde(default)]
    pub enforcement: Option<enforcement::World>,
    due: BTreeMap<SimTime, Vec<Due>>,
    accepted: BTreeMap<u64, (Command, CommandReceipt)>,
    next_command: u64,
    next_event: u64,
}

pub struct Simulation {
    state: CanonicalState,
    engine: Arc<engine_adapter::WorldEngine>,
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
        ScenarioInfo {
            id: "lineage-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "property-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "institution-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "skills-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "project-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "mission-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "force-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "invocation-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "ecology-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "farm-enterprise-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "enforcement-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "transport-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "genetics-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "health-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "settlement-founder".into(),
            version: 1,
            debug_fixture: false,
        },
        ScenarioInfo {
            id: "established-ruler".into(),
            version: 1,
            debug_fixture: false,
        },
        ScenarioInfo {
            id: "child-successor".into(),
            version: 1,
            debug_fixture: false,
        },
        ScenarioInfo {
            id: "army-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
        ScenarioInfo {
            id: "realm-fixture".into(),
            version: 1,
            debug_fixture: true,
        },
    ]
}

impl Simulation {
    pub fn new(scenario: &str, seed: u64, execution: Execution) -> Result<Self, Error> {
        if scenarios::PLAYABLE.contains(&scenario) || scenario == "random" {
            return Self::playable_scenario(
                scenario,
                seed,
                execution,
                scenarios::Config::default(),
            );
        }
        if scenario == "ecology-fixture" {
            return Self::ecology_fixture(seed, execution);
        }
        if scenario == "farm-enterprise-fixture" {
            return Self::farm_enterprise_fixture(seed, execution);
        }
        if scenario == "mission-fixture" {
            return Self::mission_fixture(seed, execution);
        }
        if scenario == "army-fixture" {
            return Self::army_fixture(seed, execution);
        }
        if scenario == "force-fixture" {
            return Self::force_fixture(seed, execution);
        }
        if scenario == "invocation-fixture" {
            return Self::invocation_fixture(seed, execution);
        }
        if scenario == "project-fixture" {
            return Self::project_fixture(seed, execution);
        }
        if scenario == "enforcement-fixture" {
            return Self::enforcement_fixture(seed, execution);
        }
        if scenario == "realm-fixture" {
            return Self::realm_fixture(seed, execution);
        }
        if scenario == "transport-fixture" {
            return Self::transport_fixture(seed, execution);
        }
        if scenario == "genetics-fixture" {
            return Self::genetics_fixture(seed, execution);
        }
        if scenario == "health-fixture" {
            return Self::health_fixture(seed, execution);
        }
        if scenario == "skills-fixture" {
            return Self::skills_fixture(seed, execution);
        }
        if scenario == "institution-fixture" {
            return Self::institution_fixture(seed, execution);
        }
        if scenario == "property-fixture" {
            return Self::property_fixture(seed, execution);
        }
        if scenario == "lineage-fixture" {
            return Self::lineage_fixture(seed, execution);
        }
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
            transport: None,
            genetics: None,
            physical_traits: None,
            temperament: None,
            heredity: None,
            content: BTreeMap::new(),
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
            agriculture: None,
            agreements: None,
            lineage: None,
            property: None,
            institutions: None,
            skills: None,
            projects: None,
            armies: None,
            realm: None,
            invocation: None,
            force: None,
            missions: None,
            ecology: None,
            health: None,
            social: None,
            enforcement: None,
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
        let engine = Arc::new(engine_adapter::build(&execution)?);
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
            engine,
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
        sim.validate_agriculture()?;
        sim.validate_realm()?;
        sim.validate_armies()?;
        Ok(sim)
    }

    pub(crate) fn has_stock_access(&self, actor: PersonId, inventory: &Inventory) -> bool {
        inventory.access.contains(&actor) || self.institution_allows_access(actor, inventory.id)
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
        let ruler_location = self.resolved_location(ruler.id);
        let origin = match ruler_location {
            Location::AtArea(a) => a,
            Location::InTransit { to, .. } => to,
            Location::StoppedOnRoad { from, .. } => from,
            Location::Carried { .. } => unreachable!("resolved carrier position"),
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
            } else if matches!(
                ruler_location,
                Location::AtArea(_) | Location::StoppedOnRoad { .. }
            ) {
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
                    if let Some(development) = &self.state.development {
                        let children = development.lives.iter().filter_map(|(id, life)| {
                            (self.dependent_signal_visible(ruler.id, *id)
                                && matches!(life.stage, LifeStage::Infant | LifeStage::Child))
                            .then_some(*id)
                        });
                        for child in children {
                            if child != ruler.id {
                                available_commands.push(Command::Comfort { child });
                            }
                            for source in view
                                .local_stocks
                                .iter()
                                .filter(|s| self.prepare_care_source(ruler.id, child, s.id).is_ok())
                            {
                                available_commands.push(Command::Care {
                                    child,
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
                    if stage == LifeStage::Child && matches!(ruler_location, Location::AtArea(_)) {
                        available_commands.push(Command::Play {
                            interest: preference,
                        });
                    }
                }
            }
        }
        if !matches!(
            ruler.location,
            Location::AtArea(_) | Location::InTransit { .. }
        ) {
            available_commands.retain(|command| !matches!(command, Command::Move { .. }));
        }
        if matches!(ruler_location, Location::StoppedOnRoad { .. }) {
            available_commands
                .retain(|command| !matches!(command, Command::Work { .. } | Command::Load { .. }));
        }
        available_commands.extend(self.available_army_commands(ruler.id));
        available_commands.extend(self.available_realm_commands(ruler.id));
        available_commands.extend(self.available_transport_commands(ruler.id));
        available_commands.extend(self.available_cultivation(ruler.id));
        available_commands.extend(self.available_agreement_commands(ruler.id));
        available_commands.extend(self.available_property_commands(ruler.id));
        available_commands.extend(self.available_institution_commands(ruler.id));
        available_commands.extend(self.available_lesson_commands(ruler.id));
        available_commands.extend(self.available_project_commands(ruler.id));
        available_commands.extend(self.available_invocation_commands(ruler.id));
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
        available_commands.extend(self.available_social_commands(ruler.id));
        available_commands.extend(self.available_enforcement_commands(ruler.id));
        available_commands.extend(self.available_health_commands(ruler.id));
        available_commands.extend(self.available_ecology_access_commands(ruler.id));
        available_commands.extend(self.available_force_commands(ruler.id));
        available_commands.extend(self.available_mission_commands(ruler.id));
        let mut quantity_opportunities = self.cargo_quantity_opportunities(ruler.id);
        quantity_opportunities.extend(self.ecology_quantity_opportunities(ruler.id));
        for opportunity in &quantity_opportunities {
            for quantity in &opportunity.suggested {
                let command = opportunity.action.command(*quantity);
                if !available_commands.contains(&command) {
                    available_commands.push(command);
                }
            }
        }
        if !self.independent(ruler.id) {
            quantity_opportunities.clear();
            available_commands
                .retain(|command| matches!(command, Command::Signal { .. } | Command::Cancel));
        }
        if self.past_life_controls_actor(ruler.id) {
            available_commands.clear();
            quantity_opportunities.clear();
        }
        PlayerView {
            armies: self.army_view(ruler.id),
            realm: self.realm_view(ruler.id),
            transport: self.transport_view(ruler.id),
            traits: self.inherited_traits_view(ruler.id),
            social: self.social_view(ruler.id),
            enforcement: self.enforcement_view(ruler.id),
            health: self.health_view(ruler.id),
            ecology: self.ecology_view(ruler.id),
            missions: self.mission_view(ruler.id),
            force: self.force_view(self.state.ruler),
            invocation: self.invocation_view(),
            projects: self.project_view(ruler.id),
            skills: self.own_skill_view(ruler.id),
            institutions: self.institution_view(ruler.id),
            social_cases: self.social_case_view(ruler.id),
            property: self.property_view(ruler.id),
            lineage: self.lineage_view(),
            agreements: self.agreement_view(ruler.id),
            environment: self.environment_view(ruler.id),
            agriculture: self.agriculture_view(ruler.id),
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
            quantity_opportunities,
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

    pub fn lineage_view(&self) -> Option<LineageView> {
        let world = self.state.lineage.as_ref()?;
        let current = world.current();
        let local = self
            .nearby_people(0, 100)
            .into_iter()
            .map(|(id, _)| id)
            .collect::<BTreeSet<_>>();
        let learning = current
            .and_then(|id| world.invocation_learning.get(&id))
            .copied()
            .unwrap_or(0);
        Some(LineageView {
            invocation_learning: learning,
            invocation_ready: learning >= world.rules.invocation_access_threshold,
            current,
            incarnations: world.incarnations.iter().map(|life| life.person).collect(),
            waiting_since: world.waiting.as_ref().map(|w| w.died_at),
            guardians: world
                .guardians
                .iter()
                .filter(|(id, _)| current.is_none() || local.contains(id))
                .map(|(id, guardian)| GuardianSearchView {
                    ready: current
                        .and_then(|id| guardian.assessments.get(&id))
                        .is_some_and(|a| a.ready),
                    assessment: current
                        .and_then(|id| guardian.assessments.get(&id))
                        .map_or_else(
                            || "Readiness remains unassessed".into(),
                            |a| a.explanation.clone(),
                        ),
                    explanation: guardian.explanation.clone(),
                    guardian: *id,
                    location: self.state.people[self.person_index[id]].location.clone(),
                    target: guardian.target,
                    goal: match guardian.goal {
                        lineage::GuardianGoal::Waiting => GuardianGoalView::Waiting,
                        lineage::GuardianGoal::Searching => GuardianGoalView::Searching,
                        lineage::GuardianGoal::Mentoring => GuardianGoalView::Mentoring,
                        lineage::GuardianGoal::Confined => GuardianGoalView::Confined,
                        lineage::GuardianGoal::Recovering => GuardianGoalView::Recovering,
                    },
                })
                .collect(),
        })
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
        if self.past_life_controls_actor(self.state.ruler) {
            return Err(Error::Unavailable);
        }
        if matches!(
            submission.command,
            Command::Engage { .. } | Command::CombatTurn { .. }
        ) {
            return self.submit_combat(submission);
        }
        if !matches!(submission.command, Command::Move { .. }) {
            return self.submit_actor_action(submission);
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
        if self.combat_restricts(actor) || !self.transport_can_depart(actor) {
            return Err(Error::Unavailable);
        }
        if !self.independent(actor)
            || self.state.physical.as_ref().is_some_and(|w| {
                !w.actors[&actor].vitals.alive
                    || w.actors[&actor].vitals.health == 0
                    || w.actors[&actor].vitals.fatigue >= w.rules.fatigue_limit
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
            Location::StoppedOnRoad { .. } | Location::Carried { .. } => {
                return Err(Error::Unavailable);
            }
        };
        let path = if let Some(routes) = self.invocation_routes(actor) {
            decisions::known_path(&routes, origin, destination)?.0
        } else if let Some(world) = &self.state.decisions {
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
        self.interrupt_actor_activity(actor);
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
        self.leave_shelter_on_movement(actor);
        if let Some(people) = self.dependent_index.get_mut(&from) {
            people.remove(&actor);
        }
        self.state.people[index].route.remove(0);
        self.state.people[index].location = Location::InTransit {
            from,
            to,
            departed: self.state.time,
            arrives,
            road_start: None,
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
        self.transport_departed(self.state.people[index].id);
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
            if let Some(warning) = self.required_invocation_input() {
                return Ok(AdvanceResult {
                    requested,
                    reached: self.state.time,
                    revision: self.revision,
                    reason: StopReason::InvocationWarning { warning },
                });
            }
            let encounters: Vec<_> = self.state.combat.as_ref().map_or_else(Vec::new, |w| {
                w.encounters
                    .values()
                    .filter(|e| e.active)
                    .map(|e| e.id)
                    .collect()
            });
            for encounter in encounters {
                self.drive_combat_npcs(encounter)?;
            }
            if let Some(encounter) = self.awaiting_combat_input() {
                return Ok(AdvanceResult {
                    requested,
                    reached: self.state.time,
                    revision: self.revision,
                    reason: StopReason::CombatInput { encounter },
                });
            }
            let next = self.engine.next_boundary(self, requested)?;
            let controlled = self.state.ruler;
            let prepared = self.prepare_world_step(next)?;
            self.commit_prepared_step(prepared)?;
            if controlled != self.state.ruler {
                return Ok(AdvanceResult {
                    requested,
                    reached: self.state.time,
                    revision: self.revision,
                    reason: StopReason::SuccessorBorn {
                        ruler: self.state.ruler,
                    },
                });
            }
            if let Some(warning) = self.required_invocation_input() {
                return Ok(AdvanceResult {
                    requested,
                    reached: self.state.time,
                    revision: self.revision,
                    reason: StopReason::InvocationWarning { warning },
                });
            }
        }
        Ok(AdvanceResult {
            requested,
            reached: self.state.time,
            revision: self.revision,
            reason: StopReason::TargetReached,
        })
    }

    fn prepare_world_step(
        &self,
        next: SimTime,
    ) -> Result<engine_adapter::PreparedWorldStep, Error> {
        let started = Instant::now();
        let elapsed = next.0 - self.state.time.0;
        let equipment = self.equipment_availability();
        let input: Vec<_> = self
            .state
            .people
            .iter()
            .map(|p| compute::ProgressInput {
                units: p.work_units,
                remainder: p.work_remainder,
                units_per_minute: if matches!(p.location, Location::AtArea(_))
                    && self.physically_working(p.id)
                    && equipment.get(&p.id).copied().unwrap_or(true)
                {
                    p.work_rate
                } else {
                    0
                },
                elapsed_ms: elapsed,
            })
            .collect();
        let output = compute::evaluate(
            self.engine.executor().pool(),
            &input,
            self.execution.reverse_jobs,
        )?;
        let physical_output = self.evaluate_physical(elapsed)?;
        let skill_output = self.evaluate_skills(next)?;
        let project_output = self.evaluate_projects(next)?;
        let invocation_output = self.evaluate_invocation(next)?;
        let environment_output = self.evaluate_environment(next)?;
        let elapsed_day = environment_output
            .as_ref()
            .and_then(|step| step.elapsed_local_day.as_ref());
        let agriculture_output = self.evaluate_agriculture(next, elapsed_day)?;
        let ecology_output = self.evaluate_ecology(elapsed_day)?;
        let health_output = self.evaluate_health(next)?;
        let evaluation_ns = started.elapsed().as_nanos();
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
        let mut possible_outputs = self.possible_work_outputs(&physical_output);
        if let Some(agriculture) = &self.state.agriculture {
            for deadline in &due {
                if let Due::Physical(actor) = deadline
                    && let Activity::Cultivate {
                        field,
                        operation: FarmOperation::Harvest,
                        ..
                    } = self.state.physical.as_ref().unwrap().actors[actor].activity
                {
                    possible_outputs.push((
                        field,
                        Good::Food,
                        agriculture.rules_for(field)?.food_yield,
                    ));
                    possible_outputs.push((
                        field,
                        Good::Seed,
                        agriculture.rules_for(field)?.seed_yield,
                    ));
                }
            }
        }
        self.preflight_output_shares(&possible_outputs)?;
        self.preflight_prepared_maintenance(next, &possible_outputs)?;
        let births = due.iter().filter(|d| matches!(d, Due::Birth(_))).count() as u64;
        let population = (self.state.people.len() as u64)
            .checked_add(births)
            .ok_or(Error::Overflow)?;
        let (ledger_demand, direct_cases) =
            self.closing_agreement_demand(next, &due, &possible_outputs)?;
        let society_demand = self.closing_institution_demand(
            population,
            next.0.is_multiple_of(60_000),
            ledger_demand,
            direct_cases,
        )?;
        let funding_proposals = self.project_funding_proposal_bound(next)?;
        let mut resources = engine_adapter::ResourceEnvelope::default();
        for demand in [
            self.project_allocation_bound(next)?,
            births.checked_mul(3).ok_or(Error::Overflow)?,
            self.invocation_allocation_bound(next),
            self.force_allocation_bound(&due),
            self.realm_allocation_bound(),
            self.army_allocation_bound(),
            self.mission_allocation_bound(),
            self.health_allocation_bound(next),
            self.enforcement_allocation_bound(),
            society_demand.ids,
            funding_proposals,
        ] {
            resources.add_ids(demand)?;
        }
        self.preflight_development(&due, next)?;
        self.preflight_combat(&due, next)?;
        self.preflight_force(&due)?;
        self.preflight_health(next)?;
        self.preflight_armies_at(next)?;
        self.preflight_missions()?;
        self.preflight_institutions_at(next)?;
        let mut event_bound = 0u64;
        for event in &due {
            event_bound = event_bound
                .checked_add(match event {
                    Due::Arrive(_) => 2,
                    Due::Transfers(transfers) => transfers.len() as u64,
                    Due::Review => 0,
                    Due::Physical(_) | Due::Spoil(_) => 4,
                    Due::Birth(_) | Due::Age(_) => 2,
                    Due::CombatRound(_) => population.checked_mul(8).ok_or(Error::Overflow)?,
                })
                .ok_or(Error::Overflow)?;
        }
        resources.add_events(event_bound)?;
        for demand in [
            population.checked_mul(2).ok_or(Error::Overflow)?,
            self.mission_event_bound() as u64,
            self.army_event_bound() as u64,
            self.health_event_bound() as u64,
            self.state
                .agriculture
                .as_ref()
                .map_or(0, |world| world.fields().count() as u64),
            ledger_demand.events()?,
            society_demand.events,
            invocation_output.event_demand(),
            funding_proposals.checked_mul(2).ok_or(Error::Overflow)?,
        ] {
            resources.add_events(demand)?;
        }
        if funding_proposals > 0 {
            // A native funding proposal may flush pending work; that work is
            // already in ledger demand, and its admission guard also needs a
            // three-event continuation margin after earlier publications.
            resources.add_events(3)?;
        }
        resources.check(self.state.next_id, self.state.next_event)?;
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
        let arbitration_ns = started.elapsed().as_nanos();
        Ok(engine_adapter::PreparedWorldStep {
            next,
            elapsed,
            equipment,
            output,
            physical_output,
            skill_output,
            project_output,
            invocation_output,
            environment_output,
            agriculture_output,
            ecology_output,
            health_output,
            due,
            lifecycle,
            transfers,
            opening_stock,
            evaluated_people: input.len(),
            evaluation_ns,
            arbitration_ns,
        })
    }

    fn preflight_prepared_maintenance(
        &self,
        next: SimTime,
        outputs: &[(SiteId, Good, Quantity)],
    ) -> Result<(), Error> {
        if let Some(world) = &self.state.social
            && next.0 >= world.last_review.0.saturating_add(world.rules.review.0)
        {
            next.0
                .checked_add(world.rules.review.0)
                .ok_or(Error::Overflow)?;
        }
        if let Some(world) = &self.state.physical_traits
            && world.next_expression <= next
        {
            world.prepare_refresh_time(next)?;
            if let Some(lives) = &self.state.development {
                for actor in world.people.keys() {
                    lives.lives[actor].age_at(next)?;
                }
            }
        }
        self.preflight_project_accounting(outputs)?;
        Ok(())
    }

    fn commit_prepared_step(
        &mut self,
        prepared: engine_adapter::PreparedWorldStep,
    ) -> Result<(), Error> {
        let engine_adapter::PreparedWorldStep {
            next,
            elapsed,
            equipment,
            output,
            physical_output,
            skill_output,
            project_output,
            invocation_output,
            environment_output,
            agriculture_output,
            ecology_output,
            health_output,
            due,
            lifecycle,
            transfers,
            opening_stock,
            evaluated_people,
            evaluation_ns,
            arbitration_ns,
        } = prepared;
        if let Some(step) = &environment_output {
            self.state
                .environment
                .as_ref()
                .ok_or(Error::Unavailable)?
                .admit(step)?;
        }
        if let Some(batch) = &agriculture_output {
            self.state
                .agriculture
                .as_ref()
                .ok_or(Error::Unavailable)?
                .validate_advance(batch)?;
        }
        self.profile.evaluation_ns += evaluation_ns;
        self.profile.arbitration_ns += arbitration_ns;
        let mut continued_journeys = Vec::new();
        let mut production_completions = Vec::new();
        let mut force_completions = Vec::new();
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
                        equipment
                            .get(actor)
                            .copied()
                            .unwrap_or(false)
                            .then_some((*actor, site))
                    } else if let Activity::Cultivate { field, .. } = body.activity {
                        equipment
                            .get(actor)
                            .copied()
                            .unwrap_or(false)
                            .then_some((*actor, field))
                    } else {
                        None
                    }
                })
                .collect()
        });
        for (actor, site) in actual_work {
            self.record_agreement_work(actor, site, elapsed);
        }
        let builders: Vec<_> = self.state.projects.as_ref().map_or_else(Vec::new, |w| {
            w.jobs
                .iter()
                .map(|(actor, job)| (*actor, job.project))
                .collect()
        });
        for (actor, project) in builders {
            self.record_agreement_construction(actor, project, elapsed);
        }
        // Install elapsed clinical progress before closing intake effects.
        // Water care and other completed actions then update the progressed
        // state once, rather than being overwritten by an opening snapshot.
        self.commit_health(health_output)?;
        self.commit_agriculture(agriculture_output)?;
        self.commit_projects(project_output);
        self.commit_environment(environment_output)?;
        self.commit_ecology(ecology_output);
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
                    if self.transport_arrive(id) {
                        continue;
                    }
                    let index = self.person_index[&id];
                    if let Location::InTransit { to, .. } = self.state.people[index].location {
                        self.state.people[index].location = Location::AtArea(to);
                        self.index_dependent(id, to);
                        self.emit(EventKind::Arrived {
                            actor: id,
                            area: to,
                        });
                        if let Some(destination) = self.state.people[index].route.first().copied() {
                            continued_journeys.push((id, to, destination));
                        }
                    }
                }
                Due::Transfers(_) => {}
                Due::Age(_) | Due::Birth(_) | Due::CombatRound(_) => {}
                Due::Physical(actor) => {
                    if self.state.physical.as_ref().is_some_and(|w| {
                        matches!(
                            w.actors[&actor].activity,
                            Activity::Gather { .. }
                                | Activity::Cultivate { .. }
                                | Activity::Construct { .. }
                        )
                    }) {
                        production_completions.push(actor);
                    } else if self.state.physical.as_ref().is_some_and(|w| {
                        matches!(
                            w.actors[&actor].activity,
                            Activity::Escape { .. }
                                | Activity::Restrain { .. }
                                | Activity::Lift { .. }
                                | Activity::RealmTalk { .. }
                        )
                    }) {
                        force_completions.push(actor);
                    } else {
                        self.complete_physical(actor)?;
                    }
                }
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
        let mut resolved = self
            .prepare_cargo_transfers(transfers, Some(&opening_stock))?
            .begin(&self.state.inventories)?;
        self.profile.arbitration_ns += arbitration_started.elapsed().as_nanos();
        while let Some((intent, granted)) = resolved.apply_next(&mut self.state.inventories) {
            if granted {
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
        // Closing allocations are shared across all workers before any completion
        // releases a tool. A released or newly produced toolkit cannot fund an
        // interval which already elapsed.
        let closing_equipment = self.equipment_availability();
        for actor in production_completions {
            if closing_equipment.get(&actor).copied().unwrap_or(false) {
                self.complete_physical(actor)?;
            } else {
                self.interrupt_actor_activity(actor);
                self.emit(EventKind::Interrupted { actor });
            }
        }
        let phase_started = Instant::now();
        self.commit_physical(physical_output, &closing_equipment)?;
        self.profile
            .record_phase("commit.commit_physical", phase_started);
        self.reconcile_transport();
        for actor in force_completions {
            if self.state.physical.as_ref().is_some_and(|w| {
                matches!(
                    w.actors[&actor].activity,
                    Activity::Escape { .. }
                        | Activity::Restrain { .. }
                        | Activity::Lift { .. }
                        | Activity::RealmTalk { .. }
                )
            }) {
                self.complete_physical(actor)?;
            }
        }
        self.commit_skills(skill_output);
        let phase_started = Instant::now();
        self.reconcile_health(&closing_equipment)?;
        self.profile
            .record_phase("commit.reconcile_health", phase_started);
        self.reconcile_social();
        self.observe_invocation_deaths()?;
        self.commit_invocation(invocation_output)?;
        self.observe_lineage_deaths();
        for event in lifecycle
            .iter()
            .filter(|d| !matches!(d, Due::CombatRound(_)))
        {
            self.complete_development(event.clone());
        }
        self.reconcile_transport();
        // A closing arrival cannot grant another leg before death, exhaustion,
        // or incapacity at that same timestamp has been committed.
        for (actor, origin, destination) in continued_journeys {
            let ready = self.state.physical.as_ref().is_none_or(|world| {
                let body = &world.actors[&actor];
                body.vitals.alive
                    && body.vitals.health > 0
                    && body.vitals.fatigue < world.rules.fatigue_limit
            }) && !self.combat_restricts(actor)
                && self.independent(actor)
                && self.transport_can_depart(actor);
            let index = self.person_index[&actor];
            if ready
                && self.state.people[index].location == Location::AtArea(origin)
                && self.state.people[index].route.first() == Some(&destination)
            {
                let (_, arrival) = self.departure(origin, destination)?;
                self.depart(index, origin, destination, arrival);
            } else {
                self.state.people[index].route.clear();
                self.emit(EventKind::Interrupted { actor });
            }
        }
        self.reconcile_missions()?;
        self.reconcile_armies()?;
        self.admit_due_mission_reinforcements();
        for event in lifecycle {
            if let Due::CombatRound(encounter) = event {
                self.finish_combat_round(encounter)?;
            }
        }
        self.observe_invocation_deaths()?;
        self.update_force();
        self.reconcile_guardian_deliveries()?;
        self.reconcile_missions()?;
        self.reconcile_armies()?;
        let phase_started = Instant::now();
        self.update_projects()?;
        self.profile
            .record_phase("commit.update_projects", phase_started);
        let phase_started = Instant::now();
        self.update_property_events()?;
        self.profile
            .record_phase("commit.update_property_events", phase_started);
        let phase_started = Instant::now();
        self.observe_ecology();
        self.profile
            .record_phase("commit.observe_ecology", phase_started);
        let phase_started = Instant::now();
        self.observe_health();
        self.profile
            .record_phase("commit.observe_health", phase_started);
        let phase_started = Instant::now();
        self.observe_enforcement();
        self.profile
            .record_phase("commit.observe_enforcement", phase_started);
        let phase_started = Instant::now();
        self.reconcile_agreements()?;
        self.profile
            .record_phase("commit.reconcile_agreements", phase_started);
        let phase_started = Instant::now();
        self.observe_agreement_events();
        self.profile
            .record_phase("commit.observe_agreement_events", phase_started);
        let phase_started = Instant::now();
        self.observe_realm()?;
        self.profile
            .record_phase("commit.observe_realm", phase_started);
        let phase_started = Instant::now();
        self.reconcile_institutions()?;
        self.profile
            .record_phase("commit.reconcile_institutions", phase_started);
        let phase_started = Instant::now();
        self.learn_from_work();
        self.profile
            .record_phase("commit.learn_from_work", phase_started);
        let phase_started = Instant::now();
        self.refresh_traits()?;
        self.profile
            .record_phase("commit.refresh_traits", phase_started);
        let phase_started = Instant::now();
        self.review_social()?;
        self.profile
            .record_phase("commit.review_social", phase_started);
        if self.state.time.0.is_multiple_of(60_000) {
            self.review_decisions()?;
            let phase_started = Instant::now();
            self.review_invocation()?;
            self.profile
                .record_phase("commit.review_invocation", phase_started);
        }
        self.revision += 1;
        self.profile.commit_ns += started.elapsed().as_nanos();
        self.profile.evaluated_people += evaluated_people as u64;
        self.profile.boundaries += 1;
        Ok(())
    }

    fn inventory_area(&self, inventory: &Inventory) -> Option<AreaId> {
        match inventory.location {
            InventoryLocation::Area(a) => Some(a),
            InventoryLocation::Carried(id) => match self.resolved_location(id) {
                Location::AtArea(a) => Some(a),
                _ => None,
            },
        }
    }

    fn cargo_closing_facts(&self, intent: &TransferIntent) -> game_cargo::ClosingFacts {
        let source = self.state.inventories.get(&intent.source);
        let destination = self.state.inventories.get(&intent.destination);
        let body = self
            .state
            .physical
            .as_ref()
            .and_then(|w| w.actors.get(&intent.actor));
        game_cargo::ClosingFacts {
            alive: self.state.physical.is_none() || body.is_some_and(|body| body.vitals.alive),
            health: if self.state.physical.is_none() {
                1
            } else {
                body.map_or(0, |body| body.vitals.health)
            },
            source_contact: source.is_some_and(|stock| self.stock_here(intent.actor, stock)),
            source_access: source.is_some_and(|stock| self.has_stock_access(intent.actor, stock)),
            receiver_contact: destination.is_some_and(|stock| self.stock_here(intent.actor, stock)),
            receiver_access: destination
                .is_some_and(|stock| self.has_stock_access(intent.actor, stock)),
            receiver_accepted: self.agreement_accepts_delivery(
                intent.actor,
                intent.destination,
                intent.quantity,
            ) || self.guardian_accepts_delivery(
                intent.actor,
                intent.source,
                intent.destination,
                intent.quantity,
            ),
            record_permission: self.record_transfer_allowed(intent.source, intent.quantity),
        }
    }
    fn prepare_cargo_transfers(
        &self,
        intents: Vec<TransferIntent>,
        opening: Option<&BTreeMap<InventoryId, u64>>,
    ) -> Result<game_cargo::PreparedTransfers, Error> {
        let requests = intents
            .into_iter()
            .map(|intent| {
                let facts = self.cargo_closing_facts(&intent);
                game_cargo::TransferRequest { intent, facts }
            })
            .collect();
        self.prepare_cargo_requests(requests, opening)
    }
    fn prepare_cargo_requests(
        &self,
        requests: Vec<game_cargo::TransferRequest>,
        opening: Option<&BTreeMap<InventoryId, u64>>,
    ) -> Result<game_cargo::PreparedTransfers, Error> {
        if requests.is_empty() {
            return game_cargo::prepare_transfers(&BTreeMap::new(), None, requests, None, |_| true);
        }
        let stocks = requests
            .iter()
            .flat_map(|request| [request.intent.source, request.intent.destination])
            .filter_map(|id| self.state.inventories.get(&id).map(|stock| (id, stock)))
            .collect();
        let free = self.state.physical.as_ref().map(|_| {
            let holders: BTreeSet<_> = requests
                .iter()
                .flat_map(|request| [request.intent.source, request.intent.destination])
                .filter_map(|id| self.state.inventories.get(&id))
                .filter_map(|stock| match stock.location {
                    InventoryLocation::Carried(holder) => Some(holder),
                    InventoryLocation::Area(_) => None,
                })
                .collect();
            holders
                .into_iter()
                .map(|holder| {
                    let held: u128 = self
                        .inventory_index
                        .get(&InventoryLocation::Carried(holder))
                        .into_iter()
                        .flatten()
                        .map(|id| u128::from(self.state.inventories[id].quantity.0))
                        .sum();
                    (
                        holder,
                        u128::from(self.body_rules(holder).carrying_capacity.0).saturating_sub(held)
                            as u64,
                    )
                })
                .collect()
        });
        let mut transport_bodies = if self
            .state
            .transport
            .as_ref()
            .is_some_and(|world| !world.groups.is_empty())
        {
            self.transport_bodies().ok()
        } else {
            None
        };
        game_cargo::prepare_transfers(&stocks, opening, requests, free.as_ref(), |intent| {
            let Some(world) = &self.state.transport else {
                return true;
            };
            if world.groups.is_empty() {
                return true;
            }
            let InventoryLocation::Carried(holder) =
                self.state.inventories[&intent.destination].location
            else {
                return true;
            };
            let Some(bodies) = &mut transport_bodies else {
                return false;
            };
            if !game_transport::transfer_capacity(&world.groups, bodies, holder, intent.quantity)
                .unwrap_or(false)
            {
                return false;
            }
            let Some(body) = bodies.get_mut(&holder) else {
                return false;
            };
            let Some(quantity) = body.cargo.0.checked_add(intent.quantity.0) else {
                return false;
            };
            body.cargo = Quantity(quantity);
            true
        })
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
        if let EventKind::Died { actor } = kind {
            self.transport_death(actor);
        }
        self.on_health_event(&kind);
        self.on_social_event(&kind);
        self.on_trait_event(&kind);
        self.update_water_event(&kind);
        self.witness_lineage_event(&kind);
        self.on_skill_event(&kind);
        if let EventKind::Transferred {
            source,
            destination,
            quantity,
            ..
        } = &kind
        {
            self.relocate_record_transfer(*source, *destination, *quantity);
        }
        let id = EventId(self.state.next_event);
        self.on_delivery_event(&kind, id);
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
