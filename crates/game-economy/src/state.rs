//! Serializable people, rules and independently owned social state.
pub use game_agreements::Agreements;
use game_autowork::Autowork;
pub use game_economy_person::{Immortal, Person, Rules};
use game_goals::Goals;
use game_lot_runtime::Snapshot as Physical;
use game_market::Market;
use game_nutrition::Nutrition;
use game_opinions::Opinions;
use game_production_model::FoodEffect;
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
    pub shared_work: Option<game_economy_work::shared::SharedWork>,
    #[serde(default)]
    pub work_tools: Vec<game_economy_work::tools::Requirement>,
    #[serde(default)]
    pub tool_wear: Option<game_economy_work::tool_wear::ToolWear>,
    #[serde(default)]
    pub work_sites: Option<game_economy_work::sites::Facilities>,
    #[serde(default)]
    pub construction: Option<game_economy_building::Construction>,
    #[serde(default)]
    pub confinement: Option<game_economy_confinement::Confinement>,
    #[serde(default)]
    pub goals: Option<Goals>,
    #[serde(default)]
    pub dreams: Option<game_economy_goals::Dreams>,
    #[serde(default)]
    pub nutrition: Option<Nutrition>,
    #[serde(default)]
    pub pending_food: Vec<FoodEffect>,
    #[serde(default)]
    pub travel: Option<game_economy_person::travel::Travel>,
    #[serde(default)]
    pub family: Option<game_economy_family::Families>,
    #[serde(default)]
    pub care: Option<game_economy_work::care::DependentCare>,
    #[serde(default)]
    pub agriculture: Option<game_economy_agriculture::Agriculture>,
    #[serde(default)]
    pub dynamics: Option<game_economy_dynamics::Dynamics>,
    #[serde(default)]
    pub missions: Option<game_economy_expeditions::Missions>,
    #[serde(default)]
    pub campaign: Option<game_economy_campaign::Campaign>,
    #[serde(default)]
    pub skills: Option<game_economy_skills::Skills>,
    #[serde(default)]
    pub knowledge: Option<game_economy_knowledge::Knowledge>,
    #[serde(default)]
    pub writing: Option<crate::writing::Writing>,
    #[serde(default)]
    pub combat: Option<game_economy_combat::Combat>,
    #[serde(default)]
    pub fatigue: Option<game_economy_fatigue::Fatigue>,
    #[serde(default)]
    pub environment: Option<game_environment::Environment>,
    #[serde(default)]
    pub purpose: Option<game_purpose::Purpose>,
    #[serde(default)]
    pub recovery: Option<crate::recovery::Rules>,
    #[serde(default)]
    pub gathering: Option<crate::gathering::Gathering>,
    #[serde(default)]
    pub fauna: Option<game_fauna::Fauna>,
    #[serde(default)]
    pub animal_care: Option<crate::animal_care::AnimalCare>,
    #[serde(default)]
    pub animal_products: Option<crate::animal_products::AnimalProducts>,
    #[serde(default)]
    pub animal_handling: Option<crate::animal_handling::Handling>,
    #[serde(default)]
    pub animal_restraints: Option<game_animal_restraint::Restraints>,
    #[serde(default)]
    pub animal_feelings: Option<crate::animal_feelings::AnimalFeelings>,
    #[serde(default)]
    pub taking: Option<game_economy_taking::Taking>,
    #[serde(default)]
    pub hunting: Option<crate::hunting::Hunting>,
    #[serde(default)]
    pub food_search: Option<crate::food_search::FoodSearch>,
    #[serde(default)]
    pub giving: Option<crate::giving::Giving>,
    #[serde(default)]
    pub medical: Option<crate::medical::Medical>,
    #[serde(default)]
    pub hauling: Option<crate::hauling::Hauling>,
    #[serde(default)]
    pub local_transport: Option<game_economy_local_transport::LocalTransport>,
}
