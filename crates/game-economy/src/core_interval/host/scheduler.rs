//! Bounded event-driven advancement of the saved, limited core host.
use super::*;
use std::sync::Arc;

const SELECTED_DOMAINS: u64 = (1 << 1)
    | (1 << 2)
    | (1 << 3)
    | (1 << 4)
    | (1 << 6)
    | (1 << 7)
    | (1 << 8)
    | (1 << 9)
    | (1 << 10)
    | (1 << 12)
    | (1 << 13)
    | (1 << 15);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum DecisionPhase {
    Meals,
    Treatment {
        meal_people: Vec<u64>,
    },
    Provision {
        meal_people: Vec<u64>,
    },
    Social {
        meal_people: Vec<u64>,
    },
    Maintenance {
        meal_people: Vec<u64>,
    },
    ToolMaintenance {
        meal_people: Vec<u64>,
    },
    Delivery {
        meal_people: Vec<u64>,
    },
    Offers {
        meal_people: Vec<u64>,
    },
    Acceptance {
        meal_people: Vec<u64>,
    },
    Asking {
        meal_people: Vec<u64>,
    },
    Requests {
        meal_people: Vec<u64>,
    },
    Work {
        meal_people: Vec<u64>,
    },
    Assisting {
        meal_people: Vec<u64>,
    },
    Exchange {
        meal_people: Vec<u64>,
    },
    ExchangeProposals {
        meal_people: Vec<u64>,
    },
    ReadPages {
        meal_people: Vec<u64>,
    },
    PracticeWriting {
        meal_people: Vec<u64>,
    },
    WriteClaims {
        meal_people: Vec<u64>,
    },
    WritePages {
        meal_people: Vec<u64>,
    },
    ChooseLocal {
        meal_people: Vec<u64>,
    },
    ChosenBatch {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenExperiment {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenMeal {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenToolRepair {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenFoodPickup {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenSelfTreatment {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenMechanicalLoad {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenMechanicalDesign {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenAttack {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    },
    ChosenConstruction {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        carrying_after: bool,
        #[serde(default)]
        maintenance_after: bool,
        #[serde(default)]
        work_after: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<u64>,
    },
    ChosenCarrying {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        construction_after: bool,
        #[serde(default)]
        maintenance_after: bool,
        #[serde(default)]
        work_after: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<u64>,
    },
    ChosenMaintenance {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        construction_after: bool,
        carrying_after: bool,
        #[serde(default)]
        work_after: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<u64>,
    },
    ChosenWork {
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        construction_after: bool,
        carrying_after: bool,
        maintenance_after: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining: Option<u64>,
    },
    Construction {
        meal_people: Vec<u64>,
    },
    LocalCarrying {
        meal_people: Vec<u64>,
    },
    HaulingRequests {
        meal_people: Vec<u64>,
    },
    Hauling {
        meal_people: Vec<u64>,
    },
    FoodSearch {
        meal_people: Vec<u64>,
    },
    Travel {
        meal_people: Vec<u64>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSchedule {
    pub period_millis: u64,
    pub work_rate: u64,
    /// Offset from this quantum's opening; it may lie in a later quantum.
    pub next_decision_millis: Option<u128>,
    pub phase: DecisionPhase,
    /// Rates retained across boundaries, including temporarily paused work.
    #[serde(default)]
    pub carried_rates: Vec<(u64, u64)>,
    /// Opt-in personal choice for construction, carrying, maintenance and work starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personal_choices: Option<Arc<Vec<game_personal_choice::Person>>>,
    /// Chosen cultivation intentions awaiting the next physical boundary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_cultivation: Vec<game_economy_agriculture::opportunities::Opportunity>,
    /// Outcomes staged by the boundary and emitted by the scheduler.
    #[serde(skip)]
    pub settled_cultivation: Vec<CoreChoiceEvent>,
    /// Optional authored inclinations for actual newborns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birth_choices: Option<game_personal_choice::BirthProfile>,
}
impl CoreSchedule {
    pub(super) fn validate(&self, core: &CoreInterval) -> Result<(), String> {
        let offset = core
            .elapsed_millis()
            .ok_or("scheduled host needs an active interval")?;
        let quantum = core.physical.tick_millis();
        if self.period_millis == 0
            || self.work_rate == 0
            || offset > quantum
            || self
                .next_decision_millis
                .is_some_and(|next| next < u128::from(offset))
            || self
                .carried_rates
                .par_windows(2)
                .any(|rows| rows[0].0 >= rows[1].0)
            || self
                .carried_rates
                .par_iter()
                .any(|&(id, rate)| id == 0 || rate == 0)
        {
            return Err("invalid core decision schedule".into());
        }
        if let Some(people) = &self.personal_choices {
            game_personal_choice::validate_people(people).map_err(str::to_owned)?;
            if people.par_iter().any(|person| {
                core.people
                    .binary_search_by_key(&person.id, |row| row.id)
                    .is_err()
            }) {
                return Err("invalid personal choice people".into());
            }
        }
        if !self.pending_cultivation.is_empty() {
            let Some(people) = &self.personal_choices else {
                return Err("pending cultivation requires personal choices".into());
            };
            let mut fields: Vec<_> = self
                .pending_cultivation
                .par_iter()
                .map(|row| row.field)
                .collect();
            fields.par_sort_unstable();
            if self
                .pending_cultivation
                .par_windows(2)
                .any(|p| p[0].person >= p[1].person)
                || fields.par_windows(2).any(|p| p[0] == p[1])
                || self.pending_cultivation.par_iter().any(|row| {
                    row.person == 0
                        || row.field == 0
                        || row.region == 0
                        || row.crop == 0
                        || people
                            .binary_search_by_key(&row.person, |person| person.id)
                            .is_err()
                        || core
                            .people
                            .binary_search_by_key(&row.person, |person| person.id)
                            .is_err()
                        || match (row.phase, row.planting) {
                            (game_economy_agriculture::Phase::Fallow, Some(seed)) => {
                                seed.good == 0 || seed.quantity == 0 || seed.batch == Some(0)
                            }
                            (game_economy_agriculture::Phase::Ripe, None) => false,
                            _ => true,
                        }
                })
            {
                return Err("invalid pending cultivation".into());
            }
        }
        if let Some(profile) = &self.birth_choices {
            profile.validate().map_err(str::to_owned)?;
            if self.personal_choices.is_none() {
                return Err("newborn preferences require personal choices".into());
            }
        }
        if matches!(self.phase, DecisionPhase::ChooseLocal { .. })
            && self.personal_choices.is_none()
        {
            return Err("local choice phase requires personal choices".into());
        }
        let remaining = match &self.phase {
            DecisionPhase::ChosenBatch { remaining, .. } if *remaining == 0 => {
                return Err("empty selected activity batch".into());
            }
            DecisionPhase::ChosenBatch { remaining, .. }
            | DecisionPhase::ChosenExperiment { remaining, .. }
            | DecisionPhase::ChosenMeal { remaining, .. }
            | DecisionPhase::ChosenToolRepair { remaining, .. }
            | DecisionPhase::ChosenFoodPickup { remaining, .. }
            | DecisionPhase::ChosenSelfTreatment { remaining, .. }
            | DecisionPhase::ChosenMechanicalLoad { remaining, .. }
            | DecisionPhase::ChosenMechanicalDesign { remaining, .. }
            | DecisionPhase::ChosenAttack { remaining, .. } => Some(*remaining),
            DecisionPhase::ChosenConstruction { remaining, .. }
            | DecisionPhase::ChosenCarrying { remaining, .. }
            | DecisionPhase::ChosenMaintenance { remaining, .. }
            | DecisionPhase::ChosenWork { remaining, .. } => *remaining,
            _ => None,
        };
        if remaining.is_some_and(|mask| mask & !SELECTED_DOMAINS != 0) {
            return Err("invalid selected activity batch".into());
        }
        let current_domain = match &self.phase {
            DecisionPhase::ChosenConstruction {
                remaining: Some(_), ..
            } => Some(1),
            DecisionPhase::ChosenCarrying {
                remaining: Some(_), ..
            } => Some(2),
            DecisionPhase::ChosenMaintenance {
                remaining: Some(_), ..
            } => Some(3),
            DecisionPhase::ChosenWork {
                remaining: Some(_), ..
            } => Some(4),
            DecisionPhase::ChosenExperiment { .. } => Some(6),
            DecisionPhase::ChosenMeal { .. } => Some(7),
            DecisionPhase::ChosenToolRepair { .. } => Some(8),
            DecisionPhase::ChosenFoodPickup { .. } => Some(9),
            DecisionPhase::ChosenSelfTreatment { .. } => Some(10),
            DecisionPhase::ChosenMechanicalLoad { .. } => Some(12),
            DecisionPhase::ChosenMechanicalDesign { .. } => Some(13),
            DecisionPhase::ChosenAttack { .. } => Some(15),
            _ => None,
        };
        if current_domain
            .zip(remaining)
            .is_some_and(|(domain, mask)| mask & (1u64 << domain) != 0)
        {
            return Err("selected activity cannot be replayed".into());
        }
        if let DecisionPhase::ChosenConstruction { intents, .. }
        | DecisionPhase::ChosenCarrying { intents, .. }
        | DecisionPhase::ChosenMaintenance { intents, .. }
        | DecisionPhase::ChosenWork { intents, .. }
        | DecisionPhase::ChosenBatch { intents, .. }
        | DecisionPhase::ChosenExperiment { intents, .. }
        | DecisionPhase::ChosenMeal { intents, .. }
        | DecisionPhase::ChosenToolRepair { intents, .. }
        | DecisionPhase::ChosenFoodPickup { intents, .. }
        | DecisionPhase::ChosenSelfTreatment { intents, .. }
        | DecisionPhase::ChosenMechanicalLoad { intents, .. }
        | DecisionPhase::ChosenMechanicalDesign { intents, .. }
        | DecisionPhase::ChosenAttack { intents, .. } = &self.phase
        {
            let Some(people) = &self.personal_choices else {
                return Err("selected actions require personal choices".into());
            };
            if intents
                .par_windows(2)
                .any(|rows| rows[0].actor >= rows[1].actor)
                || intents.par_iter().any(|intent| {
                    intent.actor == 0
                        || intent.key.action == 0
                        || intent.cultivation.is_some()
                        || (intent.key.domain != personal_choice::MECHANICAL_LOAD && intent.mechanical_load.is_some())
                        || (intent.key.domain != personal_choice::MECHANICAL_DESIGN && intent.assembly.is_some())
                        || (intent.key.domain != personal_choice::MECHANICAL_WATCH && intent.watch.is_some())
                        || (intent.key.domain != personal_choice::PERSONAL_ATTACK && intent.strike.is_some())
                        || match (
                            intent.key.domain,
                            intent.movement,
                            intent.work,
                            intent.experiment,
                        ) {
                            (1..=3, Some(movement), None, None) => {
                                intent.health_value.is_some() || intent.tool_damage.is_some()
                                    || intent.meal_value.is_some()
                                    || intent.experiment_value.is_some()
                                    || movement.id != 0
                                    || movement.quantity == 0
                                    || intent
                                        .second_movement
                                        .is_some_and(|row| row.id != 0 || row.quantity == 0)
                            }
                            (4, None, Some(work), None) => {
                                intent.health_value.is_some() || intent.tool_damage.is_some()
                                    || intent.meal_value.is_some()
                                    || intent.experiment_value.is_some()
                                    || intent.second_movement.is_some()
                                    || work.region == 0
                                    || work.recipe == 0
                                    || work.units == 0
                                    || work.base_rate == 0
                                    || intent.key.action != u64::from(work.recipe)
                            }
                            (6, None, None, Some(experiment)) => {
                                intent.health_value.is_some() || intent.tool_damage.is_some()
                                    || intent.meal_value.is_some()
                                    || intent.experiment_value.is_none()
                                    || intent.second_movement.is_some()
                                    || experiment.actor != intent.actor
                                    || experiment.apparatus == 0
                                    || experiment.output_part == 0
                                    || experiment.input_force == 0
                                    || experiment.input_distance == 0
                            }
                            (7, Some(movement), None, None) => {
                                intent.health_value.is_some() || intent.tool_damage.is_some()
                                    || intent.meal_value.is_none()
                                    || intent.experiment_value.is_some()
                                    || intent.second_movement.is_some()
                                    || movement.id != 0
                                    || movement.quantity == 0
                                    || movement.from != game_lots::Place::Person(intent.actor)
                                    || movement.to.is_some()
                                    || movement.item.is_some()
                                    || intent.key.action != u64::from(movement.good)
                            }
                            (8, Some(movement), None, None) => {
                                intent.health_value.is_some() || intent.tool_damage.is_none_or(|damage| damage == 0)
                                    || intent.meal_value.is_some()
                                    || intent.experiment_value.is_some()
                                    || intent.second_movement.is_some()
                                    || movement.id != 0
                                    || movement.quantity == 0
                                    || movement.from != game_lots::Place::Person(intent.actor)
                                    || movement.to.is_some()
                                    || movement.item.is_some()
                            }
                            (9, Some(movement), None, None) => {
                                intent.health_value.is_some() || intent.tool_damage.is_some() || intent.meal_value.is_none()
                                    || intent.experiment_value.is_some() || intent.second_movement.is_some()
                                    || movement.id != 0 || movement.quantity == 0 || movement.item.is_some()
                                    || !matches!(movement.from, game_lots::Place::Stock { region, good } if region != 0 && good == movement.good)
                                    || movement.to != Some(game_lots::Place::Person(intent.actor))
                                    || intent.key.action != u64::from(movement.good)
                            }
                            (10, Some(movement), None, None) => {
                                intent.health_value.is_none_or(|value| value < 0)
                                    || intent.tool_damage.is_some() || intent.meal_value.is_some()
                                    || intent.experiment_value.is_some() || intent.second_movement.is_some()
                                    || movement.id != 0 || movement.quantity == 0 || movement.item.is_some()
                                    || movement.from != game_lots::Place::Person(intent.actor) || movement.to.is_some()
                                    || intent.key.action != u64::from(movement.good)
                            }
                            (12, None, None, None) => {
                                let Some(load) = intent.mechanical_load else { return true; };
                                intent.health_value.is_some() || intent.tool_damage.is_some() || intent.meal_value.is_some()
                                    || intent.experiment_value.is_none() || intent.second_movement.is_some()
                                    || load.actor != intent.actor || load.apparatus == 0 || load.output_part == 0
                                    || load.input_force == 0 || load.input_distance == 0 || load.required_travel == 0
                                    || load.movement.id != 0 || load.movement.quantity == 0 || load.movement.to.is_none()
                                    || !load.movement.from.accepts(load.movement.good)
                                    || load.movement.to.is_some_and(|to| !to.accepts(load.movement.good) || to == load.movement.from)
                                    || load.movement.item == Some(0) || load.movement.batch == Some(0)
                                    || (load.movement.item.is_some() && load.movement.quantity != 1)
                            }
                            (13, None, None, None) => {
                                let Some(assembly) = &intent.assembly else { return true; };
                                intent.health_value.is_some() || intent.tool_damage.is_some() || intent.meal_value.is_some()
                                    || intent.experiment_value.is_none() || intent.second_movement.is_some()
                                    || assembly.actor != intent.actor || assembly.apparatus.id != intent.key.action
                                    || assembly.apparatus.parts.is_empty()
                            }
                            (14, None, None, None) => {
                                let Some(watch) = intent.watch else { return true; };
                                intent.health_value.is_some() || intent.tool_damage.is_some() || intent.meal_value.is_some()
                                    || intent.experiment_value.is_none() || intent.second_movement.is_some()
                                    || watch.observer != intent.actor || watch.actor == 0 || watch.actor == intent.actor
                            }
                            (15, None, None, None) => {
                                let Some(strike) = intent.strike else { return true; };
                                intent.health_value.is_some() || intent.tool_damage.is_some() || intent.meal_value.is_some()
                                    || intent.experiment_value.is_none() || intent.second_movement.is_some()
                                    || strike.actor != intent.actor || strike.target == 0 || strike.target == strike.actor || strike.force == 0
                            }
                            _ => true,
                        }
                        || people
                            .binary_search_by_key(&intent.actor, |person| person.id)
                            .ok()
                            .is_none_or(|index| {
                                !personal_choice::intention_permitted(&people[index], intent)
                            })
                })
            {
                return Err("invalid selected personal actions".into());
            }
        }
        if let DecisionPhase::Treatment { meal_people }
        | DecisionPhase::Provision { meal_people }
        | DecisionPhase::Social { meal_people }
        | DecisionPhase::Maintenance { meal_people }
        | DecisionPhase::ToolMaintenance { meal_people }
        | DecisionPhase::Delivery { meal_people }
        | DecisionPhase::Offers { meal_people }
        | DecisionPhase::Acceptance { meal_people }
        | DecisionPhase::Asking { meal_people }
        | DecisionPhase::Requests { meal_people }
        | DecisionPhase::Work { meal_people }
        | DecisionPhase::Assisting { meal_people }
        | DecisionPhase::Exchange { meal_people }
        | DecisionPhase::ExchangeProposals { meal_people }
        | DecisionPhase::ReadPages { meal_people }
        | DecisionPhase::PracticeWriting { meal_people }
        | DecisionPhase::WriteClaims { meal_people }
        | DecisionPhase::WritePages { meal_people }
        | DecisionPhase::ChooseLocal { meal_people }
        | DecisionPhase::ChosenBatch { meal_people, .. }
        | DecisionPhase::ChosenExperiment { meal_people, .. }
        | DecisionPhase::ChosenMeal { meal_people, .. }
        | DecisionPhase::ChosenToolRepair { meal_people, .. }
        | DecisionPhase::ChosenFoodPickup { meal_people, .. }
        | DecisionPhase::ChosenSelfTreatment { meal_people, .. }
        | DecisionPhase::ChosenMechanicalLoad { meal_people, .. }
        | DecisionPhase::ChosenMechanicalDesign { meal_people, .. }
        | DecisionPhase::ChosenAttack { meal_people, .. }
        | DecisionPhase::ChosenConstruction { meal_people, .. }
        | DecisionPhase::ChosenCarrying { meal_people, .. }
        | DecisionPhase::ChosenMaintenance { meal_people, .. }
        | DecisionPhase::ChosenWork { meal_people, .. }
        | DecisionPhase::Construction { meal_people }
        | DecisionPhase::LocalCarrying { meal_people }
        | DecisionPhase::HaulingRequests { meal_people }
        | DecisionPhase::Hauling { meal_people }
        | DecisionPhase::FoodSearch { meal_people }
        | DecisionPhase::Travel { meal_people } = &self.phase
            && (self.next_decision_millis != Some(u128::from(offset))
                || offset == quantum
                || meal_people.par_windows(2).any(|rows| rows[0] >= rows[1])
                || meal_people
                    .par_iter()
                    .any(|id| core.people.binary_search_by_key(id, |p| p.id).is_err()))
        {
            return Err("invalid pending work decision".into());
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum CoreRunEvent {
    Arrivals(CoreArrivals),
    Deferred(Vec<u64>),
    Work(game_lot_runtime::EarlyWorkEvents),
    Meals(Vec<(u64, u32)>),
    Treatments(Vec<game_lots::TransactionOutcome>),
    Provisions(Vec<game_lots::TransactionOutcome>),
    Meetings(Vec<game_social::Encounter>),
    Disease(Vec<game_disease::Event>),
    Repairs(Vec<game_economy_building::maintenance::Event>),
    ToolRepairs(Vec<game_economy_work::tool_wear::RepairEvent>),
    Deliveries(Vec<game_lots::TransactionOutcome>),
    Offers(Vec<u64>),
    Acceptances(Vec<u64>),
    Asks(Vec<u64>),
    Requests(Vec<game_autowork::requests::Event>),
    Starts(Vec<(u64, bool)>),
    Assists(Vec<u64>),
    Adoptions(Vec<game_lot_runtime::Adoption>),
    Exchanges(Vec<u64>),
    ExchangeOffers(Vec<u64>),
    ReadPages(Vec<u64>),
    PracticedWriting(Vec<u64>),
    WrittenPages(Vec<u64>),
    Construction(Vec<game_economy_building::Event>),
    LocalCarries(Vec<game_economy_local_transport::Event>),
    PersonalChoices(Vec<CoreChoiceEvent>),
    MechanicalExperiments(Vec<game_mechanics::Observation>),
    MechanicalLoads(Vec<MechanicalLoadResult>),
    MechanicalAssemblies(Vec<game_mechanics::AssemblyOutcome>),
    PersonalStrikes {
        events: Vec<game_personal_conflict::StrikeEvent>,
        harm: Vec<PersonalHarm>,
    },
    HaulingRequests(Vec<u64>),
    HaulingTransfers(Vec<game_lots::TransactionOutcome>),
    HaulingDepartures(Vec<travel::DepartureOutcome>),
    Departures(Vec<travel::DepartureOutcome>),
    FoodSearch(Vec<travel::DepartureOutcome>),
    Boundary(Box<Events>),
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CoreRun {
    pub elapsed_millis: u64,
    pub boundary: bool,
    pub boundaries: u64,
    pub decisions: u64,
    pub events: Vec<CoreRunEvent>,
    /// Earlier events and elapsed time remain committed when an action fails.
    pub failure: Option<String>,
}
impl CoreHost {
    fn after_selected_batch(
        &self,
        meal_people: Vec<u64>,
        intents: Arc<Vec<CoreChoiceIntent>>,
        remaining: u64,
    ) -> DecisionPhase {
        if remaining == 0 {
            self.after_local_carrying(meal_people)
        } else {
            DecisionPhase::ChosenBatch {
                meal_people,
                intents,
                remaining,
            }
        }
    }
    fn after_treatment(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self.food_search.is_some() {
            DecisionPhase::Provision { meal_people }
        } else {
            self.after_provision(meal_people)
        }
    }
    fn after_provision(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self.social.is_some() || self.health.is_some() {
            DecisionPhase::Social { meal_people }
        } else {
            self.after_food(meal_people)
        }
    }
    fn after_food(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self.sites.is_some() {
            DecisionPhase::Maintenance { meal_people }
        } else if self
            .tool_wear
            .as_ref()
            .is_some_and(|owner| !owner.repairs.is_empty())
        {
            DecisionPhase::ToolMaintenance { meal_people }
        } else if self.agreements.is_some() {
            DecisionPhase::Delivery { meal_people }
        } else if self
            .autowork
            .as_ref()
            .is_some_and(|owner| owner.requests.is_some())
        {
            DecisionPhase::Asking { meal_people }
        } else {
            DecisionPhase::Work { meal_people }
        }
    }
    fn after_exchange(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self.writing.is_some() {
            DecisionPhase::ReadPages { meal_people }
        } else if self.shared_work.is_some() {
            DecisionPhase::Assisting { meal_people }
        } else {
            self.after_assisting(meal_people)
        }
    }
    fn after_assisting(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self
            .schedule
            .as_ref()
            .is_some_and(|schedule| schedule.personal_choices.is_some())
            && (self.construction.is_some()
                || self.local_transport.is_some()
                || self.sites.is_some()
                || self.autowork.is_some()
                || self.mechanics.is_some()
                || self.tool_wear.is_some()
                || self.food_search.is_some()
                || self
                    .health
                    .as_ref()
                    .is_some_and(|owner| !owner.treatment.is_empty())
                || self
                    .schedule
                    .as_ref()
                    .and_then(|s| s.personal_choices.as_ref())
                    .is_some_and(|people| {
                        people.par_iter().any(|p| {
                            p.preferences
                                .binary_search_by_key(&personal_choice::MEAL, |r| r.domain)
                                .is_ok()
                        })
                    }))
        {
            DecisionPhase::ChooseLocal { meal_people }
        } else if self.construction.is_some() {
            DecisionPhase::Construction { meal_people }
        } else {
            self.after_construction(meal_people)
        }
    }
    fn after_construction(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self.local_transport.is_some() {
            DecisionPhase::LocalCarrying { meal_people }
        } else {
            self.after_local_carrying(meal_people)
        }
    }
    fn after_local_carrying(&self, meal_people: Vec<u64>) -> DecisionPhase {
        if self.hauling.is_some() {
            DecisionPhase::HaulingRequests { meal_people }
        } else {
            DecisionPhase::FoodSearch { meal_people }
        }
    }
    /// Begin periodic personal decisions at the current partial offset.
    pub fn enable_schedule(&mut self, period_millis: u64, work_rate: u64) -> Result<(), String> {
        if self.schedule.is_some() {
            return Err("core schedule already active".into());
        }
        let offset = self.core.elapsed_millis().ok_or("no physical interval")?;
        let mut carried_rates = self
            .core
            .physical
            .work_interval()
            .expect("active interval")
            .rates
            .clone();
        carried_rates.extend(self.paused_work_rates.iter().copied());
        carried_rates.par_sort_unstable_by_key(|row| row.0);
        let schedule = CoreSchedule {
            period_millis,
            work_rate,
            next_decision_millis: Some(u128::from(offset)),
            phase: DecisionPhase::Meals,
            carried_rates,
            personal_choices: None,
            pending_cultivation: Vec::new(),
            settled_cultivation: Vec::new(),
            birth_choices: None,
        };
        schedule.validate(&self.core)?;
        self.schedule = Some(schedule);
        Ok(())
    }
    /// Stop scheduling between completed personal decision batches.
    pub fn disable_schedule(&mut self) -> Result<(), String> {
        let schedule = self.schedule.as_ref().ok_or("no core schedule")?;
        if !matches!(schedule.phase, DecisionPhase::Meals)
            || !schedule.pending_cultivation.is_empty()
        {
            return Err("finish the pending work decision first".into());
        }
        self.schedule = None;
        Ok(())
    }

    /// Advance within this quantum, publishing due work and arrivals and
    /// running scheduled personal choices. The host supplies sorted occupied
    /// people from owners not included in this limited core.
    pub fn advance_scheduled(
        &mut self,
        requested_millis: u64,
        blocked: &[u64],
    ) -> Result<CoreRun, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        let external_blocked = blocked;
        let occupied = self.fatigue.as_ref().map(|owner| {
            let mut people = blocked.to_vec();
            people.extend(owner.fatigue.resting());
            people.par_sort_unstable();
            people.dedup();
            people
        });
        let blocked = occupied.as_deref().unwrap_or(blocked);
        self.schedule
            .as_ref()
            .ok_or("no core schedule")?
            .validate(&self.core)?;
        let start = self.core.elapsed_millis().ok_or("no physical interval")?;
        let quantum = self.core.physical.tick_millis();
        let mut run = CoreRun::default();
        macro_rules! attempt {
            ($label:lifetime, $operation:expr) => {
                match $operation {
                    Ok(value) => value,
                    Err(error) => {
                        run.failure = Some(error);
                        break $label;
                    }
                }
            };
        }
        'run: loop {
            let pending_busy = self
                .schedule
                .as_ref()
                .filter(|schedule| !schedule.pending_cultivation.is_empty())
                .map(|schedule| {
                    let mut busy = blocked.to_vec();
                    busy.par_extend(
                        schedule
                            .pending_cultivation
                            .par_iter()
                            .map(|row| row.person),
                    );
                    busy.par_sort_unstable();
                    busy.dedup();
                    busy
                });
            let blocked = pending_busy.as_deref().unwrap_or(blocked);
            let offset = self.core.elapsed_millis().expect("active interval");
            if offset == quantum {
                run.boundary = true;
                break;
            }
            attempt!('run, self.sync_work_rates(blocked));
            let deadline = attempt!('run, self.core.next_deadline(None));
            if deadline.after_millis == 0 {
                let mut handled = false;
                if deadline.arrival
                    && let Some(arrivals) = attempt!('run, self.publish_arrivals())
                {
                    run.events.push(CoreRunEvent::Arrivals(arrivals));
                    handled = true;
                }
                if deadline.work {
                    let perishable: HashSet<_> = self
                        .core
                        .physical
                        .goods()
                        .par_iter()
                        .filter_map(|good| {
                            good.food
                                .as_ref()
                                .and_then(|food| food.shelf_days.map(|_| good.id))
                        })
                        .collect();
                    let rates = &self
                        .core
                        .physical
                        .work_interval()
                        .expect("active interval")
                        .rates;
                    let defer: Vec<_> = self
                        .core
                        .physical
                        .work()
                        .par_iter()
                        .filter(|row| {
                            row.phase == game_lot_runtime::Phase::Working
                                && row.job.remaining == 0
                                && rates
                                    .binary_search_by_key(&row.job.id, |rate| rate.0)
                                    .is_ok()
                                && row
                                    .job
                                    .outputs
                                    .iter()
                                    .any(|output| perishable.contains(&output.good))
                        })
                        .map(|row| row.job.id)
                        .collect();
                    if !defer.is_empty() {
                        attempt!('run, self.defer_work(self.core.physical.tick(), offset, &defer));
                        run.events.push(CoreRunEvent::Deferred(defer));
                        handled = true;
                    }
                    if let Some(events) = attempt!('run, self.publish_early_work()) {
                        run.events.push(CoreRunEvent::Work(events));
                        handled = true;
                    }
                }
                if !handled {
                    run.failure = Some("unresolved core deadline".into());
                    break;
                }
                continue;
            }
            let schedule = self.schedule.as_ref().expect("active schedule").clone();
            // Periodic physiological owners advance even when the next personal
            // decision lies beyond this entire physical tick.
            if offset == 0
                && schedule.next_decision_millis != Some(0)
                && self.core.physical.tick() > 0
            {
                let tick = self.core.physical.tick();
                let due = self
                    .social
                    .as_ref()
                    .is_some_and(|owner| owner.last_tick != Some(tick))
                    || self
                        .health
                        .as_ref()
                        .is_some_and(|owner| owner.last_tick != Some(tick));
                if due {
                    let dynamics = attempt!('run, self.advance_dynamics_autonomously(blocked));
                    if !dynamics.meetings.is_empty() {
                        run.events.push(CoreRunEvent::Meetings(dynamics.meetings));
                    }
                    if !dynamics.disease_events.is_empty() {
                        run.events
                            .push(CoreRunEvent::Disease(dynamics.disease_events));
                    }
                    continue;
                }
            }
            if schedule.next_decision_millis == Some(u128::from(offset)) {
                match schedule.phase {
                    DecisionPhase::Meals => {
                        let (stopped, choices) = if let Some(people) = &schedule.personal_choices {
                            attempt!('run, self.reconsider_ordinary_work(people, external_blocked))
                        } else {
                            (Vec::new(), Vec::new())
                        };
                        if !choices.is_empty() {
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        attempt!('run, self.release_shared_helpers());
                        let deferred_meals: Vec<_> = schedule
                            .personal_choices
                            .as_ref()
                            .map_or_else(Vec::new, |people| {
                                people
                                    .par_iter()
                                    .filter(|p| {
                                        p.preferences
                                            .binary_search_by_key(&personal_choice::MEAL, |r| {
                                                r.domain
                                            })
                                            .is_ok()
                                    })
                                    .map(|p| p.id)
                                    .collect()
                            });
                        let occupied =
                            (!stopped.is_empty() || !deferred_meals.is_empty()).then(|| {
                                let mut occupied = external_blocked.to_vec();
                                occupied.extend(stopped.iter().copied());
                                occupied.extend(deferred_meals.iter().copied());
                                occupied.par_sort_unstable();
                                occupied.dedup();
                                occupied
                            });
                        let meals = attempt!('run, self.eat_autonomously(occupied.as_deref().unwrap_or(external_blocked)));
                        let mut people = stopped;
                        people.extend(meals.iter().map(|meal| meal.person));
                        people.par_sort_unstable();
                        people.dedup();
                        if !meals.is_empty() {
                            run.events.push(CoreRunEvent::Meals(
                                meals.iter().map(|meal| (meal.person, meal.good)).collect(),
                            ));
                        }
                        let next = if self
                            .health
                            .as_ref()
                            .is_some_and(|owner| !owner.treatment.is_empty())
                        {
                            DecisionPhase::Treatment {
                                meal_people: people,
                            }
                        } else {
                            self.after_treatment(people)
                        };
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::Treatment { mut meal_people } => {
                        let mut busy = external_blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        if let Some(people) = &schedule.personal_choices {
                            busy.par_extend(
                                people
                                    .par_iter()
                                    .filter(|person| {
                                        person
                                            .preferences
                                            .binary_search_by_key(
                                                &personal_choice::SELF_TREATMENT,
                                                |p| p.domain,
                                            )
                                            .is_ok()
                                    })
                                    .map(|person| person.id),
                            );
                        }
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.treat_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.outcomes.is_empty() {
                            run.events.push(CoreRunEvent::Treatments(result.outcomes));
                        }
                        let next = self.after_treatment(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::Provision { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        if let Some(people) = &schedule.personal_choices {
                            busy.par_extend(
                                people
                                    .par_iter()
                                    .filter(|person| {
                                        person
                                            .preferences
                                            .binary_search_by_key(
                                                &personal_choice::FOOD_PICKUP,
                                                |p| p.domain,
                                            )
                                            .is_ok()
                                    })
                                    .map(|person| person.id),
                            );
                        }
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.acquire_food_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.outcomes.is_empty() {
                            run.events.push(CoreRunEvent::Provisions(result.outcomes));
                        }
                        let next = self.after_provision(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::Social { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let dynamics = attempt!('run, self.advance_dynamics_autonomously(&busy));
                        let meetings = dynamics.meetings;
                        if !meetings.is_empty() {
                            meal_people.par_extend(
                                meetings
                                    .par_iter()
                                    .flat_map_iter(|event| [event.a, event.b]),
                            );
                            meal_people.par_sort_unstable();
                            meal_people.dedup();
                            run.events.push(CoreRunEvent::Meetings(meetings));
                        }
                        if !dynamics.disease_events.is_empty() {
                            run.events
                                .push(CoreRunEvent::Disease(dynamics.disease_events));
                        }
                        let next = self.after_food(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::Maintenance { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        if let Some(people) = &schedule.personal_choices {
                            busy.par_extend(
                                people
                                    .par_iter()
                                    .filter(|person| {
                                        person
                                            .preferences
                                            .binary_search_by_key(
                                                &personal_choice::MAINTENANCE,
                                                |preference| preference.domain,
                                            )
                                            .is_ok()
                                    })
                                    .map(|person| person.id),
                            );
                        }
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.repair_sites_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::Repairs(result.events));
                        }
                        self.schedule.as_mut().expect("active schedule").phase = if self
                            .tool_wear
                            .as_ref()
                            .is_some_and(|owner| !owner.repairs.is_empty())
                        {
                            DecisionPhase::ToolMaintenance { meal_people }
                        } else if self.agreements.is_some() {
                            DecisionPhase::Delivery { meal_people }
                        } else if self
                            .autowork
                            .as_ref()
                            .is_some_and(|owner| owner.requests.is_some())
                        {
                            DecisionPhase::Asking { meal_people }
                        } else {
                            DecisionPhase::Work { meal_people }
                        };
                    }
                    DecisionPhase::ToolMaintenance { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        if let Some(people) = &schedule.personal_choices {
                            busy.par_extend(
                                people
                                    .par_iter()
                                    .filter(|person| {
                                        person
                                            .preferences
                                            .binary_search_by_key(
                                                &personal_choice::TOOL_REPAIR,
                                                |p| p.domain,
                                            )
                                            .is_ok()
                                    })
                                    .map(|person| person.id),
                            );
                        }
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.repair_tools_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::ToolRepairs(result.events));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            if self.agreements.is_some() {
                                DecisionPhase::Delivery { meal_people }
                            } else if self
                                .autowork
                                .as_ref()
                                .is_some_and(|owner| owner.requests.is_some())
                            {
                                DecisionPhase::Asking { meal_people }
                            } else {
                                DecisionPhase::Work { meal_people }
                            };
                    }
                    DecisionPhase::Delivery { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.deliver_agreements_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.outcomes.is_empty() {
                            run.events.push(CoreRunEvent::Deliveries(result.outcomes));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Offers { meal_people };
                    }
                    DecisionPhase::Offers { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.propose_agreements_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.agreements.is_empty() {
                            run.events.push(CoreRunEvent::Offers(result.agreements));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Acceptance { meal_people };
                    }
                    DecisionPhase::Acceptance { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.accept_agreements_autonomously(&busy));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.agreements.is_empty() {
                            run.events
                                .push(CoreRunEvent::Acceptances(result.agreements));
                        }
                        self.schedule.as_mut().expect("active schedule").phase = if self
                            .autowork
                            .as_ref()
                            .is_some_and(|owner| owner.requests.is_some())
                        {
                            DecisionPhase::Asking { meal_people }
                        } else {
                            DecisionPhase::Work { meal_people }
                        };
                    }
                    DecisionPhase::Asking { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let actors = attempt!('run, self.ask_for_work(&busy));
                        if !actors.is_empty() {
                            meal_people.extend(actors.iter().copied());
                            meal_people.par_sort_unstable();
                            meal_people.dedup();
                            run.events.push(CoreRunEvent::Asks(actors));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Requests { meal_people };
                    }
                    DecisionPhase::Requests { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.respond_to_work_requests(&busy, schedule.work_rate));
                        meal_people.extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::Requests(result.events));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Work { meal_people };
                    }
                    DecisionPhase::Work { mut meal_people } => {
                        let stopping = self
                            .shared_work
                            .as_ref()
                            .and_then(|shared| shared.owner.succession.as_ref())
                            .map_or_else(Vec::new, |owner| {
                                let continuing = owner.continuing(self.opinions.as_ref());
                                owner
                                    .engagements
                                    .par_iter()
                                    .filter(|engagement| {
                                        continuing.binary_search(&engagement.person).is_err()
                                            && self
                                                .core
                                                .physical
                                                .work()
                                                .binary_search_by_key(&engagement.job, |work| {
                                                    work.job.id
                                                })
                                                .ok()
                                                .is_some_and(|i| {
                                                    self.core.physical.work()[i].job.remaining > 0
                                                })
                                    })
                                    .map(|engagement| (engagement.job, engagement.person))
                                    .collect::<Vec<_>>()
                            });
                        if !stopping.is_empty() {
                            attempt!('run, self.suspend_shared_work(&stopping));
                        }
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let adoptions = attempt!('run, self.take_over_work_autonomously(&busy, schedule.work_rate));
                        if !adoptions.is_empty() {
                            meal_people.par_extend(adoptions.par_iter().map(|row| row.worker));
                            meal_people.par_sort_unstable();
                            meal_people.dedup();
                            run.events.push(CoreRunEvent::Adoptions(adoptions));
                        }
                        let mut work_busy = busy;
                        if let Some(people) = &schedule.personal_choices {
                            work_busy.par_extend(
                                people
                                    .par_iter()
                                    .filter(|person| {
                                        person
                                            .preferences
                                            .binary_search_by_key(
                                                &personal_choice::WORK,
                                                |preference| preference.domain,
                                            )
                                            .is_ok()
                                    })
                                    .map(|person| person.id),
                            );
                            work_busy.par_sort_unstable();
                            work_busy.dedup();
                        }
                        let starts = attempt!('run, self.start_autonomous_work(&work_busy, schedule.work_rate));
                        if !starts.is_empty() {
                            run.events.push(CoreRunEvent::Starts(starts));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            if self.market.is_some() {
                                DecisionPhase::Exchange { meal_people }
                            } else {
                                self.after_exchange(meal_people)
                            };
                    }
                    DecisionPhase::Exchange { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run, self.respond_to_personal_exchanges(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.offers.is_empty() {
                            run.events.push(CoreRunEvent::Exchanges(result.offers));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::ExchangeProposals { meal_people };
                    }
                    DecisionPhase::ExchangeProposals { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result =
                            attempt!('run, self.propose_personal_exchanges_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.offers.is_empty() {
                            run.events.push(CoreRunEvent::ExchangeOffers(result.offers));
                        }
                        let next = self.after_exchange(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::ReadPages { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.read_texts_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.items.is_empty() {
                            run.events.push(CoreRunEvent::ReadPages(result.items));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::PracticeWriting { meal_people };
                    }
                    DecisionPhase::PracticeWriting { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.practice_writing_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.practiced.is_empty() {
                            run.events
                                .push(CoreRunEvent::PracticedWriting(result.practiced));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::WriteClaims { meal_people };
                    }
                    DecisionPhase::WriteClaims { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.write_claims_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.items.is_empty() {
                            run.events.push(CoreRunEvent::WrittenPages(result.items));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::WritePages { meal_people };
                    }
                    DecisionPhase::WritePages { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.write_texts_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.items.is_empty() {
                            run.events.push(CoreRunEvent::WrittenPages(result.items));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            if self.shared_work.is_some() {
                                DecisionPhase::Assisting { meal_people }
                            } else {
                                self.after_assisting(meal_people)
                            };
                    }
                    DecisionPhase::Assisting { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let helpers = attempt!('run, self.assist_work_autonomously(&busy));
                        if !helpers.is_empty() {
                            meal_people.extend(helpers.iter().copied());
                            meal_people.par_sort_unstable();
                            meal_people.dedup();
                            run.events.push(CoreRunEvent::Assists(helpers));
                        }
                        let next = self.after_assisting(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::ChooseLocal { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (intents, idle) =
                            attempt!('run, self.choose_local_actions(&busy, people));
                        let (actors, pending) = self.queue_cultivation(&intents);
                        meal_people.par_extend(actors.into_par_iter());
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !pending.is_empty() {
                            run.events.push(CoreRunEvent::PersonalChoices(pending));
                        }
                        let intents = Arc::new(
                            intents
                                .into_par_iter()
                                .filter(|intent| intent.key.domain != personal_choice::CULTIVATION)
                                .collect::<Vec<_>>(),
                        );
                        if !idle.is_empty() {
                            run.events.push(CoreRunEvent::PersonalChoices(idle));
                        }
                        let round = self.core.physical.tick().rotate_left(21) ^ offset;
                        let has_maintenance = people.par_iter().any(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&personal_choice::MAINTENANCE, |preference| {
                                    preference.domain
                                })
                                .is_ok()
                        });
                        let has_work = people.par_iter().any(|person| {
                            person
                                .preferences
                                .binary_search_by_key(&personal_choice::WORK, |preference| {
                                    preference.domain
                                })
                                .is_ok()
                        });
                        let has_experiments = intents.par_iter().any(|intent| {
                            matches!(
                                intent.key.domain,
                                personal_choice::EXPERIMENT | personal_choice::MECHANICAL_WATCH
                            )
                        });
                        let has_meals = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::MEAL);
                        let has_tool_repairs = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::TOOL_REPAIR);
                        let has_food_pickups = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::FOOD_PICKUP);
                        let has_treatments = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::SELF_TREATMENT);
                        let has_mechanical_loads = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::MECHANICAL_LOAD);
                        let has_mechanical_designs = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::MECHANICAL_DESIGN);
                        let has_attacks = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::PERSONAL_ATTACK);
                        let random = game_genetics::random(self.core.seed, 0x63686f696365, round);
                        let first = if has_work {
                            random % 4
                        } else if has_maintenance {
                            random % 3
                        } else {
                            random & 1
                        };
                        self.schedule.as_mut().expect("active schedule").phase = if has_experiments
                            || has_meals
                            || has_tool_repairs
                            || has_food_pickups
                            || has_treatments
                            || has_mechanical_loads
                            || has_mechanical_designs
                            || has_attacks
                        {
                            let mut remaining = 0;
                            if has_experiments {
                                remaining |= 1 << 6;
                            }
                            if has_meals {
                                remaining |= 1 << 7;
                            }
                            if has_tool_repairs {
                                remaining |= 1 << 8;
                            }
                            if has_food_pickups {
                                remaining |= 1 << 9;
                            }
                            if has_treatments {
                                remaining |= 1 << 10;
                            }
                            if has_mechanical_loads {
                                remaining |= 1 << 12;
                            }
                            if has_mechanical_designs {
                                remaining |= 1 << 13;
                            }
                            if has_attacks {
                                remaining |= 1 << 15;
                            }
                            if self.construction.is_some() {
                                remaining |= 1 << 1;
                            }
                            if self.local_transport.is_some() {
                                remaining |= 1 << 2;
                            }
                            if has_maintenance && self.sites.is_some() {
                                remaining |= 1 << 3;
                            }
                            if has_work && self.autowork.is_some() {
                                remaining |= 1 << 4;
                            }
                            DecisionPhase::ChosenBatch {
                                meal_people,
                                intents,
                                remaining,
                            }
                        } else if first == 0 {
                            DecisionPhase::ChosenConstruction {
                                meal_people,
                                intents,
                                carrying_after: true,
                                maintenance_after: has_maintenance,
                                work_after: has_work,
                                remaining: None,
                            }
                        } else if first == 1 {
                            DecisionPhase::ChosenCarrying {
                                meal_people,
                                intents,
                                construction_after: true,
                                maintenance_after: has_maintenance,
                                work_after: has_work,
                                remaining: None,
                            }
                        } else if first == 2 {
                            DecisionPhase::ChosenMaintenance {
                                meal_people,
                                intents,
                                construction_after: true,
                                carrying_after: true,
                                work_after: has_work,
                                remaining: None,
                            }
                        } else {
                            DecisionPhase::ChosenWork {
                                meal_people,
                                intents,
                                construction_after: true,
                                carrying_after: true,
                                maintenance_after: has_maintenance,
                                remaining: None,
                            }
                        };
                    }
                    DecisionPhase::ChosenBatch {
                        meal_people,
                        intents,
                        remaining,
                    } => {
                        let round = self.core.physical.tick().rotate_left(21) ^ offset;
                        let domain = [1u32, 2, 3, 4, 6, 7, 8, 9, 10, 12, 13, 15]
                            .into_iter()
                            .filter(|domain| remaining & (1u64 << domain) != 0)
                            .min_by_key(|domain| {
                                game_genetics::random(
                                    self.core.seed,
                                    0x6163746976697479,
                                    round ^ u64::from(*domain),
                                )
                            })
                            .expect("validated nonempty batch");
                        let remaining = remaining & !(1u64 << domain);
                        self.schedule.as_mut().expect("active schedule").phase = match domain {
                            1 => DecisionPhase::ChosenConstruction {
                                meal_people,
                                intents,
                                carrying_after: false,
                                maintenance_after: false,
                                work_after: false,
                                remaining: Some(remaining),
                            },
                            2 => DecisionPhase::ChosenCarrying {
                                meal_people,
                                intents,
                                construction_after: false,
                                maintenance_after: false,
                                work_after: false,
                                remaining: Some(remaining),
                            },
                            3 => DecisionPhase::ChosenMaintenance {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                work_after: false,
                                remaining: Some(remaining),
                            },
                            4 => DecisionPhase::ChosenWork {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                maintenance_after: false,
                                remaining: Some(remaining),
                            },
                            6 => DecisionPhase::ChosenExperiment {
                                meal_people,
                                intents,
                                remaining,
                            },
                            7 => DecisionPhase::ChosenMeal {
                                meal_people,
                                intents,
                                remaining,
                            },
                            8 => DecisionPhase::ChosenToolRepair {
                                meal_people,
                                intents,
                                remaining,
                            },
                            9 => DecisionPhase::ChosenFoodPickup {
                                meal_people,
                                intents,
                                remaining,
                            },
                            10 => DecisionPhase::ChosenSelfTreatment {
                                meal_people,
                                intents,
                                remaining,
                            },
                            12 => DecisionPhase::ChosenMechanicalLoad {
                                meal_people,
                                intents,
                                remaining,
                            },
                            13 => DecisionPhase::ChosenMechanicalDesign {
                                meal_people,
                                intents,
                                remaining,
                            },
                            15 => DecisionPhase::ChosenAttack {
                                meal_people,
                                intents,
                                remaining,
                            },
                            _ => unreachable!("validated domain"),
                        };
                    }
                    DecisionPhase::ChosenAttack {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::PERSONAL_ATTACK));
                        let strikes: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::PERSONAL_ATTACK
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .map(|intent| intent.strike.expect("validated strike"))
                            .collect();
                        let mut occupied_defenders = busy.clone();
                        occupied_defenders
                            .par_extend(intents.par_iter().map(|intent| intent.actor));
                        occupied_defenders.par_sort_unstable();
                        occupied_defenders.dedup();
                        let active: HashSet<_> = self
                            .core
                            .physical
                            .work()
                            .par_iter()
                            .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                            .map(|row| row.job.worker)
                            .collect();
                        let defenses: Vec<_> = self
                            .personal_conflict
                            .as_ref()
                            .expect("attack owner")
                            .defenses
                            .par_iter()
                            .filter(|policy| {
                                policy.willing
                                    && occupied_defenders.binary_search(&policy.person).is_err()
                                    && !active.contains(&policy.person)
                                    && self
                                        .core
                                        .people
                                        .binary_search_by_key(&policy.person, |person| person.id)
                                        .ok()
                                        .is_some_and(|index| {
                                            !self.core.people[index].controlled
                                                && self.core.people[index].health > 0
                                        })
                            })
                            .map(|policy| game_personal_conflict::Defense {
                                person: policy.person,
                                reduction: policy.reduction,
                            })
                            .collect();
                        let harm = attempt!('run, self.strike_people(&strikes, &defenses, &busy));
                        choices.par_extend(strikes.par_iter().map(|strike| CoreChoiceEvent {
                            tick: self.core.physical.tick(),
                            offset_millis: offset,
                            actor: strike.actor,
                            key: personal_choice::selected_key(&intents, strike.actor),
                            outcome: CoreChoiceOutcome::Accepted,
                        }));
                        meal_people.par_extend(strikes.par_iter().map(|strike| strike.actor));
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !strikes.is_empty() {
                            run.events.push(CoreRunEvent::PersonalStrikes {
                                events: self
                                    .personal_conflict
                                    .as_ref()
                                    .expect("attack owner")
                                    .events
                                    .clone(),
                                harm,
                            });
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenMechanicalDesign {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::MECHANICAL_DESIGN));
                        let selected: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::MECHANICAL_DESIGN
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .map(|intent| intent.assembly.clone().expect("validated assembly"))
                            .collect();
                        let results = attempt!('run, self.assemble_mechanical_apparatus(&selected, &blocked_choice));
                        choices.par_extend(results.par_iter().map(|row| CoreChoiceEvent {
                            tick: self.core.physical.tick(),
                            offset_millis: offset,
                            actor: row.actor,
                            key: personal_choice::selected_key(&intents, row.actor),
                            outcome: if row.accepted {
                                CoreChoiceOutcome::Accepted
                            } else {
                                CoreChoiceOutcome::Denied
                            },
                        }));
                        meal_people.par_extend(selected.par_iter().map(|row| row.actor));
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !results.is_empty() {
                            run.events.push(CoreRunEvent::MechanicalAssemblies(results));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenMechanicalLoad {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::MECHANICAL_LOAD));
                        let selected: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::MECHANICAL_LOAD
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .map(|intent| {
                                intent.mechanical_load.expect("validated mechanical load")
                            })
                            .collect();
                        let results =
                            attempt!('run, self.move_mechanical_loads(&selected, &blocked_choice));
                        choices.par_extend(results.par_iter().map(|row| CoreChoiceEvent {
                            tick: self.core.physical.tick(),
                            offset_millis: offset,
                            actor: row.actor,
                            key: personal_choice::selected_key(&intents, row.actor),
                            outcome: if matches!(
                                row.outcome,
                                MechanicalLoadOutcome::Unavailable
                                    | MechanicalLoadOutcome::Contended
                            ) {
                                CoreChoiceOutcome::Denied
                            } else {
                                CoreChoiceOutcome::Accepted
                            },
                        }));
                        meal_people.par_extend(selected.par_iter().map(|row| row.actor));
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !results.is_empty() {
                            run.events.push(CoreRunEvent::MechanicalLoads(results));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenSelfTreatment {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::SELF_TREATMENT));
                        let (result, action_ids) = if let Some(health) = &self.health {
                            let policies: Vec<_> = health
                                .treatment
                                .par_iter()
                                .filter(|policy| {
                                    blocked_choice.binary_search(&policy.person).is_err()
                                        && personal_choice::selected_key(&intents, policy.person)
                                            .is_some_and(|key| {
                                                key.domain == personal_choice::SELF_TREATMENT
                                                    && key.action == u64::from(policy.medicine)
                                            })
                                })
                                .cloned()
                                .map(|mut policy| {
                                    if policy.health_at_most.is_none() {
                                        policy.health_at_most = Some(u32::MAX);
                                    }
                                    policy
                                })
                                .collect();
                            let view =
                                attempt!('run, self.core.physical.view(self.core.physical.tick()));
                            let plan = attempt!('run, game_economy_treatment::prepare_at(Some(&health.owner), &policies, &self.core.people, &blocked_choice, &view, 1,
                            (game_economy_treatment::Choice { manual: None, max_health: self.core.rules.max_health }, self.core.physical.tick())));
                            let action_ids: std::collections::HashMap<_, _> = plan
                                .actors
                                .par_iter()
                                .zip(plan.actions.par_iter())
                                .map(|(&actor, action)| (actor, action.id))
                                .collect();
                            let result = attempt!('run, self.commit_self_treatment(plan));
                            (result, action_ids)
                        } else {
                            (
                                CoreTreatmentResults::default(),
                                std::collections::HashMap::new(),
                            )
                        };
                        let mut outcomes = result.outcomes;
                        outcomes.par_sort_unstable_by_key(|row| row.id);
                        choices.par_extend(result.actors.par_iter().map(|&actor| {
                            CoreChoiceEvent {
                                tick: self.core.physical.tick(),
                                offset_millis: offset,
                                actor,
                                key: personal_choice::selected_key(&intents, actor),
                                outcome: if action_ids
                                    .get(&actor)
                                    .and_then(|id| {
                                        outcomes.binary_search_by_key(id, |row| row.id).ok()
                                    })
                                    .is_some_and(|i| outcomes[i].accepted)
                                {
                                    CoreChoiceOutcome::Accepted
                                } else {
                                    CoreChoiceOutcome::Denied
                                },
                            }
                        }));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !outcomes.is_empty() {
                            run.events.push(CoreRunEvent::Treatments(outcomes));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenFoodPickup {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::FOOD_PICKUP));
                        let chosen: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::FOOD_PICKUP
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .collect();
                        let plan = game_economy_person::provision::Plan {
                            actors: chosen.par_iter().map(|intent| intent.actor).collect(),
                            actions: chosen
                                .par_iter()
                                .map(|intent| {
                                    let mut movement =
                                        intent.movement.expect("validated food pickup");
                                    movement.id = intent.actor;
                                    game_lots::Transaction {
                                        id: intent.actor,
                                        movements: vec![movement],
                                        creations: vec![],
                                    }
                                })
                                .collect(),
                        };
                        let result = attempt!('run, self.commit_provision(plan));
                        let mut outcomes = result.outcomes;
                        outcomes.par_sort_unstable_by_key(|row| row.id);
                        choices.par_extend(result.actors.par_iter().map(|&actor| {
                            CoreChoiceEvent {
                                tick: self.core.physical.tick(),
                                offset_millis: offset,
                                actor,
                                key: personal_choice::selected_key(&intents, actor),
                                outcome: if outcomes
                                    .binary_search_by_key(&actor, |row| row.id)
                                    .ok()
                                    .is_some_and(|i| outcomes[i].accepted)
                                {
                                    CoreChoiceOutcome::Accepted
                                } else {
                                    CoreChoiceOutcome::Denied
                                },
                            }
                        }));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !outcomes.is_empty() {
                            run.events.push(CoreRunEvent::Provisions(outcomes));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenToolRepair {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::TOOL_REPAIR));
                        let repairs = if let Some(owner) = &self.tool_wear {
                            let rows = attempt!('run, owner.prepare_repairs(&self.core.physical, &self.core.people, &blocked_choice, &[], u32::MAX, 1));
                            rows.into_par_iter()
                                .filter(|(actor, item, _)| {
                                    personal_choice::selected_key(&intents, *actor).is_some_and(
                                        |key| {
                                            key.domain == personal_choice::TOOL_REPAIR
                                                && key.action == *item
                                        },
                                    )
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        let result = attempt!('run, self.commit_tool_repairs(repairs));
                        choices.par_extend(result.events.par_iter().map(|event| CoreChoiceEvent {
                            tick: self.core.physical.tick(),
                            offset_millis: offset,
                            actor: event.person,
                            key: personal_choice::selected_key(&intents, event.person),
                            outcome: if event.accepted {
                                CoreChoiceOutcome::Accepted
                            } else {
                                CoreChoiceOutcome::Denied
                            },
                        }));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::ToolRepairs(result.events));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenMeal {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::MEAL));
                        let meals: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::MEAL
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .map(|intent| game_lot_diet::Meal {
                                action: intent.actor,
                                person: intent.actor,
                                good: intent.movement.expect("validated meal").good,
                            })
                            .collect();
                        attempt!('run, self.eat_selected(&meals));
                        choices.par_extend(meals.par_iter().map(|meal| CoreChoiceEvent {
                            tick: self.core.physical.tick(),
                            offset_millis: offset,
                            actor: meal.person,
                            key: personal_choice::selected_key(&intents, meal.person),
                            outcome: CoreChoiceOutcome::Accepted,
                        }));
                        meal_people.par_extend(meals.par_iter().map(|meal| meal.person));
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !meals.is_empty() {
                            run.events.push(CoreRunEvent::Meals(
                                meals.iter().map(|meal| (meal.person, meal.good)).collect(),
                            ));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenExperiment {
                        mut meal_people,
                        intents,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let has_experiments = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::EXPERIMENT);
                        let has_watchers = intents
                            .par_iter()
                            .any(|intent| intent.key.domain == personal_choice::MECHANICAL_WATCH);
                        let (blocked_choice, mut choices) = if has_experiments {
                            attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::EXPERIMENT))
                        } else {
                            (Vec::new(), Vec::new())
                        };
                        let (blocked_watch, watch_choices) = if has_watchers {
                            attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::MECHANICAL_WATCH))
                        } else {
                            (Vec::new(), Vec::new())
                        };
                        choices.par_extend(watch_choices);
                        let selected: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::EXPERIMENT
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .map(|intent| intent.experiment.expect("validated experiment"))
                            .collect();
                        let watching: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::MECHANICAL_WATCH
                                    && blocked_watch.binary_search(&intent.actor).is_err()
                            })
                            .map(|intent| intent.watch.expect("validated watch"))
                            .collect();
                        let admitted_watchers: Vec<_> = watching
                            .par_iter()
                            .filter(|watch| {
                                selected
                                    .binary_search_by_key(&watch.actor, |row| row.actor)
                                    .is_ok()
                            })
                            .copied()
                            .collect();
                        let observations = if admitted_watchers.is_empty() {
                            attempt!('run, self.commit_chosen_mechanics(&selected))
                        } else {
                            attempt!('run, self.experiment_mechanics_witnessed(&selected, &admitted_watchers, &busy))
                        };
                        choices.par_extend(observations.par_iter().map(|row| CoreChoiceEvent {
                            tick: self.core.physical.tick(),
                            offset_millis: offset,
                            actor: row.actor,
                            key: personal_choice::selected_key(&intents, row.actor),
                            outcome: if row.outcome == game_mechanics::Outcome::Contended {
                                CoreChoiceOutcome::Denied
                            } else {
                                CoreChoiceOutcome::Accepted
                            },
                        }));
                        choices.par_extend(watching.par_iter().map(|watch| {
                            let seen = observations
                                .binary_search_by_key(&watch.actor, |row| row.actor)
                                .ok()
                                .is_some_and(|index| {
                                    observations[index].outcome
                                        != game_mechanics::Outcome::Contended
                                });
                            CoreChoiceEvent {
                                tick: self.core.physical.tick(),
                                offset_millis: offset,
                                actor: watch.observer,
                                key: personal_choice::selected_key(&intents, watch.observer),
                                outcome: if seen {
                                    CoreChoiceOutcome::Accepted
                                } else {
                                    CoreChoiceOutcome::Denied
                                },
                            }
                        }));
                        meal_people.par_extend(selected.par_iter().map(|row| row.actor));
                        meal_people.par_extend(watching.par_iter().map(|row| row.observer));
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !observations.is_empty() {
                            run.events
                                .push(CoreRunEvent::MechanicalExperiments(observations));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            self.after_selected_batch(meal_people, intents, remaining);
                    }
                    DecisionPhase::ChosenConstruction {
                        mut meal_people,
                        intents,
                        carrying_after,
                        maintenance_after,
                        work_after,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::CONSTRUCTION));
                        #[cfg(feature = "profile")]
                        let commit_span = game_diagnostics::span("choice.commit.construction");
                        let result = attempt!('run, self.build_sites_with_hunger_limit(&blocked_choice, u32::MAX));
                        #[cfg(feature = "profile")]
                        drop(commit_span);
                        choices.par_extend(
                            intents
                                .par_iter()
                                .filter(|intent| {
                                    intent.key.domain == personal_choice::CONSTRUCTION
                                        && blocked_choice.binary_search(&intent.actor).is_err()
                                })
                                .map(|intent| {
                                    let outcome = result
                                        .events
                                        .binary_search_by_key(&intent.actor, |event| event.person)
                                        .ok()
                                        .map(|i| &result.events[i])
                                        .map_or(CoreChoiceOutcome::Stale, |event| {
                                            if event.site.is_some() {
                                                CoreChoiceOutcome::Accepted
                                            } else {
                                                CoreChoiceOutcome::Denied
                                            }
                                        });
                                    CoreChoiceEvent {
                                        tick: self.core.physical.tick(),
                                        offset_millis: offset,
                                        actor: intent.actor,
                                        key: Some(intent.key),
                                        outcome,
                                    }
                                }),
                        );
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::Construction(result.events));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        let next = if let Some(remaining) = remaining {
                            self.after_selected_batch(meal_people, intents, remaining)
                        } else if carrying_after {
                            DecisionPhase::ChosenCarrying {
                                meal_people,
                                intents,
                                construction_after: false,
                                maintenance_after,
                                work_after,
                                remaining: None,
                            }
                        } else if maintenance_after {
                            DecisionPhase::ChosenMaintenance {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                work_after,
                                remaining: None,
                            }
                        } else if work_after {
                            DecisionPhase::ChosenWork {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                maintenance_after: false,
                                remaining: None,
                            }
                        } else {
                            self.after_local_carrying(meal_people)
                        };
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::ChosenCarrying {
                        mut meal_people,
                        intents,
                        construction_after,
                        maintenance_after,
                        work_after,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(&busy, people, &intents, personal_choice::CARRYING));
                        #[cfg(feature = "profile")]
                        let commit_span = game_diagnostics::span("choice.commit.carrying");
                        let result = attempt!('run, self.carry_locally_with_hunger_limit(&blocked_choice, u32::MAX));
                        #[cfg(feature = "profile")]
                        drop(commit_span);
                        choices.par_extend(
                            intents
                                .par_iter()
                                .filter(|intent| {
                                    intent.key.domain == personal_choice::CARRYING
                                        && blocked_choice.binary_search(&intent.actor).is_err()
                                })
                                .map(|intent| {
                                    let outcome = result
                                        .events
                                        .binary_search_by_key(&intent.actor, |event| event.person)
                                        .ok()
                                        .map(|i| &result.events[i])
                                        .map_or(CoreChoiceOutcome::Stale, |event| {
                                            if event.moved {
                                                CoreChoiceOutcome::Accepted
                                            } else {
                                                CoreChoiceOutcome::Denied
                                            }
                                        });
                                    CoreChoiceEvent {
                                        tick: self.core.physical.tick(),
                                        offset_millis: offset,
                                        actor: intent.actor,
                                        key: Some(intent.key),
                                        outcome,
                                    }
                                }),
                        );
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::LocalCarries(result.events));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        let next = if let Some(remaining) = remaining {
                            self.after_selected_batch(meal_people, intents, remaining)
                        } else if construction_after {
                            DecisionPhase::ChosenConstruction {
                                meal_people,
                                intents,
                                carrying_after: false,
                                maintenance_after,
                                work_after,
                                remaining: None,
                            }
                        } else if maintenance_after {
                            DecisionPhase::ChosenMaintenance {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                work_after,
                                remaining: None,
                            }
                        } else if work_after {
                            DecisionPhase::ChosenWork {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                maintenance_after: false,
                                remaining: None,
                            }
                        } else {
                            self.after_local_carrying(meal_people)
                        };
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::ChosenMaintenance {
                        mut meal_people,
                        intents,
                        construction_after,
                        carrying_after,
                        work_after,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        // Unprofiled people already had their ordinary maintenance phase.
                        busy.par_extend(
                            self.core
                                .people
                                .par_iter()
                                .filter(|person| {
                                    people
                                        .binary_search_by_key(&person.id, |row| row.id)
                                        .is_err()
                                })
                                .map(|person| person.id),
                        );
                        busy.par_sort_unstable();
                        busy.dedup();
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(
                            &busy, people, &intents, personal_choice::MAINTENANCE
                        ));
                        let result = attempt!('run, self.repair_sites_with_hunger_limit(&blocked_choice, u32::MAX));
                        let outcomes: std::collections::HashMap<_, _> = result
                            .events
                            .iter()
                            .map(|event| (event.person, event.repaired))
                            .collect();
                        choices.par_extend(
                            intents
                                .par_iter()
                                .filter(|intent| {
                                    intent.key.domain == personal_choice::MAINTENANCE
                                        && blocked_choice.binary_search(&intent.actor).is_err()
                                })
                                .map(|intent| CoreChoiceEvent {
                                    tick: self.core.physical.tick(),
                                    offset_millis: offset,
                                    actor: intent.actor,
                                    key: Some(intent.key),
                                    outcome: if outcomes.get(&intent.actor) == Some(&true) {
                                        CoreChoiceOutcome::Accepted
                                    } else {
                                        CoreChoiceOutcome::Denied
                                    },
                                }),
                        );
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::Repairs(result.events));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        let next = if let Some(remaining) = remaining {
                            self.after_selected_batch(meal_people, intents, remaining)
                        } else if construction_after {
                            DecisionPhase::ChosenConstruction {
                                meal_people,
                                intents,
                                carrying_after,
                                maintenance_after: false,
                                work_after,
                                remaining: None,
                            }
                        } else if carrying_after {
                            DecisionPhase::ChosenCarrying {
                                meal_people,
                                intents,
                                construction_after: false,
                                maintenance_after: false,
                                work_after,
                                remaining: None,
                            }
                        } else if work_after {
                            DecisionPhase::ChosenWork {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                maintenance_after: false,
                                remaining: None,
                            }
                        } else {
                            self.after_local_carrying(meal_people)
                        };
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::ChosenWork {
                        mut meal_people,
                        intents,
                        construction_after,
                        carrying_after,
                        maintenance_after,
                        remaining,
                    } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let people = schedule.personal_choices.as_ref().expect("choice profile");
                        let (blocked_choice, mut choices) = attempt!('run, self.choice_blocked(
                            &busy, people, &intents, personal_choice::WORK
                        ));
                        let selected: Vec<_> = intents
                            .par_iter()
                            .filter(|intent| {
                                intent.key.domain == personal_choice::WORK
                                    && blocked_choice.binary_search(&intent.actor).is_err()
                            })
                            .cloned()
                            .collect();
                        let starts = attempt!('run, self.start_selected_work(&selected, &busy));
                        choices.par_extend(starts.par_iter().map(|&(actor, _, accepted)| {
                            CoreChoiceEvent {
                                tick: self.core.physical.tick(),
                                offset_millis: offset,
                                actor,
                                key: selected
                                    .binary_search_by_key(&actor, |intent| intent.actor)
                                    .ok()
                                    .map(|i| selected[i].key),
                                outcome: if accepted {
                                    CoreChoiceOutcome::Accepted
                                } else {
                                    CoreChoiceOutcome::Denied
                                },
                            }
                        }));
                        meal_people.par_extend(starts.par_iter().map(|row| row.0));
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !starts.is_empty() {
                            run.events.push(CoreRunEvent::Starts(
                                starts.par_iter().map(|row| (row.1, row.2)).collect(),
                            ));
                        }
                        if !choices.is_empty() {
                            choices.par_sort_unstable_by_key(|event| event.actor);
                            run.events.push(CoreRunEvent::PersonalChoices(choices));
                        }
                        let next = if let Some(remaining) = remaining {
                            self.after_selected_batch(meal_people, intents, remaining)
                        } else if construction_after {
                            DecisionPhase::ChosenConstruction {
                                meal_people,
                                intents,
                                carrying_after,
                                maintenance_after,
                                work_after: false,
                                remaining: None,
                            }
                        } else if carrying_after {
                            DecisionPhase::ChosenCarrying {
                                meal_people,
                                intents,
                                construction_after: false,
                                maintenance_after,
                                work_after: false,
                                remaining: None,
                            }
                        } else if maintenance_after {
                            DecisionPhase::ChosenMaintenance {
                                meal_people,
                                intents,
                                construction_after: false,
                                carrying_after: false,
                                work_after: false,
                                remaining: None,
                            }
                        } else {
                            self.after_local_carrying(meal_people)
                        };
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::Construction { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.build_sites_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::Construction(result.events));
                        }
                        let next = self.after_construction(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::LocalCarrying { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.carry_locally_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.events.is_empty() {
                            run.events.push(CoreRunEvent::LocalCarries(result.events));
                        }
                        let next = self.after_local_carrying(meal_people);
                        self.schedule.as_mut().expect("active schedule").phase = next;
                    }
                    DecisionPhase::HaulingRequests { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let actors = attempt!('run,self.request_hauling_autonomously(&busy));
                        meal_people.par_extend(actors.par_iter().copied());
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !actors.is_empty() {
                            run.events.push(CoreRunEvent::HaulingRequests(actors));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Hauling { meal_people };
                    }
                    DecisionPhase::Hauling { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let result = attempt!('run,self.haul_autonomously(&busy));
                        meal_people.par_extend(result.actors);
                        meal_people.par_sort_unstable();
                        meal_people.dedup();
                        if !result.transfers.is_empty() {
                            run.events
                                .push(CoreRunEvent::HaulingTransfers(result.transfers));
                        }
                        if !result.departures.is_empty() {
                            run.events
                                .push(CoreRunEvent::HaulingDepartures(result.departures));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::FoodSearch { meal_people };
                    }
                    DecisionPhase::FoodSearch { mut meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people.iter().copied());
                        busy.par_sort_unstable();
                        busy.dedup();
                        let departures = attempt!('run, self.seek_food_autonomously(&busy));
                        if !departures.is_empty() {
                            meal_people.extend(departures.iter().map(|row| row.person));
                            meal_people.par_sort_unstable();
                            meal_people.dedup();
                            run.events.push(CoreRunEvent::FoodSearch(departures));
                        }
                        self.schedule.as_mut().expect("active schedule").phase =
                            DecisionPhase::Travel { meal_people };
                    }
                    DecisionPhase::Travel { meal_people } => {
                        let mut busy = blocked.to_vec();
                        busy.extend(meal_people);
                        busy.par_sort_unstable();
                        busy.dedup();
                        let departures = attempt!('run, self.depart_autonomously(&busy));
                        if !departures.is_empty() {
                            run.events.push(CoreRunEvent::Departures(departures));
                        }
                        run.decisions += 1;
                        let next = Some(u128::from(offset) + u128::from(schedule.period_millis));
                        let schedule = self.schedule.as_mut().expect("active schedule");
                        schedule.next_decision_millis = next;
                        schedule.phase = DecisionPhase::Meals;
                    }
                }
                continue;
            }
            let spent = offset - start;
            if spent >= requested_millis {
                break;
            }
            let until_decision = schedule.next_decision_millis.map_or(u64::MAX, |next| {
                (next - u128::from(offset)).min(u128::from(u64::MAX)) as u64
            });
            let step = (requested_millis - spent)
                .min(deadline.after_millis)
                .min(until_decision);
            if step == 0 {
                run.failure = Some("core schedule made no progress".into());
                break;
            }
            attempt!('run, self.progress_with_activity(step, external_blocked));
        }
        run.elapsed_millis = self.core.elapsed_millis().expect("active interval") - start;
        Ok(run)
    }

    /// Continue the limited host across boundaries, retaining decision timing
    /// and work rates. Boundary events are emitted in order. A failure after a
    /// boundary leaves the settled boundary committed and reports that failure.
    pub fn advance_continuous(
        &mut self,
        requested_millis: u64,
        blocked: &[u64],
    ) -> Result<CoreRun, String> {
        if blocked.par_windows(2).any(|rows| rows[0] >= rows[1]) {
            return Err("invalid occupied people".into());
        }
        self.schedule
            .as_ref()
            .ok_or("no core schedule")?
            .validate(&self.core)?;
        let mut combined = CoreRun::default();
        loop {
            let remaining = requested_millis - combined.elapsed_millis;
            let mut segment = self.advance_scheduled(remaining, blocked)?;
            combined.elapsed_millis += segment.elapsed_millis;
            combined.events.append(&mut segment.events);
            let Some(decisions) = combined.decisions.checked_add(segment.decisions) else {
                combined.failure = Some("decision count overflow".into());
                combined.boundary = segment.boundary;
                break;
            };
            combined.decisions = decisions;
            if let Some(error) = segment.failure {
                combined.failure = Some(error);
                combined.boundary = segment.boundary;
                break;
            }
            if !segment.boundary {
                break;
            }
            let mut schedule = self.schedule.clone().expect("active schedule");
            let quantum = self.core.physical.tick_millis();
            let mut current_rates = self
                .core
                .physical
                .work_interval()
                .expect("active interval")
                .rates
                .clone();
            self.base_shared_rates(&mut current_rates);
            let events = match self.settle_with_schedule(blocked, Some(&mut schedule)) {
                Ok(events) => events,
                Err(error) => {
                    combined.failure = Some(error);
                    combined.boundary = true;
                    break;
                }
            };
            combined
                .events
                .push(CoreRunEvent::Boundary(Box::new(events)));
            if !schedule.settled_cultivation.is_empty() {
                combined
                    .events
                    .push(CoreRunEvent::PersonalChoices(std::mem::take(
                        &mut schedule.settled_cultivation,
                    )));
            }
            let Some(boundaries) = combined.boundaries.checked_add(1) else {
                combined.failure = Some("boundary count overflow".into());
                combined.boundary = true;
                break;
            };
            combined.boundaries = boundaries;
            schedule.next_decision_millis = schedule
                .next_decision_millis
                .map(|next| next - u128::from(quantum));
            let mut candidates: Vec<_> = schedule
                .carried_rates
                .par_iter()
                .map(|&(id, rate)| (id, 0u8, rate))
                .collect();
            candidates.par_extend(current_rates.par_iter().map(|&(id, rate)| (id, 1u8, rate)));
            candidates.par_extend(
                self.paused_work_rates
                    .par_iter()
                    .map(|&(id, rate)| (id, 2u8, rate)),
            );
            candidates.par_sort_unstable_by_key(|row| (row.0, row.1));
            schedule.carried_rates = candidates
                .par_chunk_by(|a, b| a.0 == b.0)
                .filter_map(|group| {
                    let &(id, _, rate) = group.last().expect("nonempty group");
                    self.core
                        .physical
                        .work()
                        .binary_search_by_key(&id, |row| row.job.id)
                        .ok()
                        .filter(|&index| {
                            let row = &self.core.physical.work()[index];
                            row.phase == game_lot_runtime::Phase::Working && row.job.remaining > 0
                        })
                        .map(|_| (id, rate))
                })
                .collect();
            let rates: Vec<_> = schedule
                .carried_rates
                .par_iter()
                .copied()
                .filter(|(id, _)| {
                    if self
                        .paused_work_rates
                        .binary_search_by_key(id, |row| row.0)
                        .is_ok()
                    {
                        return false;
                    }
                    let index = self
                        .core
                        .physical
                        .work()
                        .binary_search_by_key(id, |row| row.job.id)
                        .expect("carried job");
                    let job = &self.core.physical.work()[index].job;
                    self.core
                        .people
                        .binary_search_by_key(&job.worker, |person| person.id)
                        .ok()
                        .is_some_and(|person| {
                            let person = &self.core.people[person];
                            person.health > 0
                                && person.hunger <= self.core.rules.max_work_hunger
                                && person.region == Some(job.region)
                                && blocked.binary_search(&person.id).is_err()
                        })
                })
                .collect();
            let next = CoreInterval::begin(self.core.snapshot(), &rates).and_then(|core| {
                schedule.validate(&core)?;
                Ok(core)
            });
            match next {
                Ok(core) => {
                    self.core = core;
                    self.schedule = Some(schedule);
                    self.work_checked_revision = None;
                }
                Err(error) => {
                    combined.failure = Some(error);
                    combined.boundary = true;
                    break;
                }
            }
        }
        Ok(combined)
    }
}
