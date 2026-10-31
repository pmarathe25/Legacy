//! Serializable people, rules and independently owned social state.
pub use game_parallel_agreements::Agreements;
use game_parallel_autowork::Autowork;
pub use game_parallel_economy_person::{Person, Rules};
use game_parallel_goals::Goals;
use game_parallel_lot_runtime::Snapshot as Physical;
use game_parallel_market::Market;
use game_parallel_nutrition::Nutrition;
use game_parallel_opinions::Opinions;
use game_parallel_production_model::FoodEffect;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub seed: u64,
    pub physical: Physical,
    pub people: Vec<Person>,
    pub rules: Rules,
    #[serde(default)]
    pub market: Option<Market>,
    #[serde(default)]
    pub agreements: Option<Agreements>,
    #[serde(default)]
    pub opinions: Option<Opinions>,
    #[serde(default)]
    pub autowork: Option<Autowork>,
    #[serde(default)]
    pub work_tools: Vec<game_parallel_economy_work::tools::Requirement>,
    #[serde(default)]
    pub work_sites: Option<game_parallel_economy_work::sites::Facilities>,
    #[serde(default)]
    pub construction: Option<game_parallel_economy_building::Construction>,
    #[serde(default)]
    pub goals: Option<Goals>,
    #[serde(default)]
    pub dreams: Option<game_parallel_economy_goals::Dreams>,
    #[serde(default)]
    pub nutrition: Option<Nutrition>,
    #[serde(default)]
    pub pending_food: Vec<FoodEffect>,
    #[serde(default)]
    pub travel: Option<game_parallel_economy_person::travel::Travel>,
    #[serde(default)]
    pub family: Option<game_parallel_economy_family::Families>,
    #[serde(default)]
    pub care: Option<game_parallel_economy_work::care::DependentCare>,
    #[serde(default)]
    pub agriculture: Option<game_parallel_economy_agriculture::Agriculture>,
    #[serde(default)]
    pub dynamics: Option<game_parallel_economy_dynamics::Dynamics>,
    #[serde(default)]
    pub missions: Option<game_parallel_economy_expeditions::Missions>,
    #[serde(default)]
    pub campaign: Option<game_parallel_economy_campaign::Campaign>,
    #[serde(default)]
    pub skills: Option<game_parallel_economy_skills::Skills>,
    #[serde(default)]
    pub knowledge: Option<game_parallel_economy_knowledge::Knowledge>,
    #[serde(default)]
    pub combat: Option<game_parallel_economy_combat::Combat>,
    #[serde(default)]
    pub fatigue: Option<game_parallel_economy_fatigue::Fatigue>,
    #[serde(default)]
    pub environment: Option<game_parallel_environment::Environment>,
    #[serde(default)]
    pub purpose: Option<game_parallel_purpose::Purpose>,
    #[serde(default)]
    pub recovery: Option<crate::recovery::Rules>,
    #[serde(default)]
    pub gathering: Option<crate::gathering::Gathering>,
    #[serde(default)]
    pub food_search: Option<crate::food_search::FoodSearch>,
    #[serde(default)]
    pub giving: Option<crate::giving::Giving>,
    #[serde(default)]
    pub medical: Option<crate::medical::Medical>,
    #[serde(default)]
    pub hauling: Option<crate::hauling::Hauling>,
    #[serde(default)]
    pub local_transport: Option<game_parallel_economy_local_transport::LocalTransport>,
}
