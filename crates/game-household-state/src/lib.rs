//! Household records and transactional refusal bookkeeping.
use game_core::*;
use game_provisioning::FoodCare;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod index;
mod refusals;
pub use refusals::{
    ClearNewFoodUrgency, ClearObservedFoodAction, ConfigureFoodRefusals, ExpireFoodRefusals,
    FoodRefusal, RefusalGroup, RememberFoodRefusal, SetRefusalRetry, forget_person,
    register as register_refusals,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "index::Saved")]
pub struct Households {
    pub enabled: bool,
    pub nursery: Table<PersonId, AreaId>,
    pub partners: Table<PersonId, PersonId>,
    pub children: Table<PersonId, Vec<PersonId>>,
    pub next: Table<PersonId, SimTime>,
    pub caregivers: Table<PersonId, Table<PersonId, ()>>,
    pub self_reliant: Table<PersonId, ()>,
    pub review: Option<ScheduleId>,
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub food_care: Table<PersonId, FoodCare>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub food_refusal_window: Option<SimDuration>,
    #[serde(default, skip_serializing_if = "Table::is_empty")]
    pub food_refusals: Table<PersonId, Table<PersonId, RefusalGroup>>,
    #[serde(skip)]
    pub wards: Table<PersonId, Table<PersonId, ()>>,
}
impl Resource for Households {
    const KEY: &'static str = "households.kin";
    fn validate(&self) -> Result<()> {
        require(
            self.food_care.iter().all(|(p, i)| i.valid_for(*p)),
            "invalid food care",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        require(
            self.food_care
                .changed(&old.food_care)
                .all(|(p, i)| i.valid_for(*p)),
            "invalid food care",
        )
    }
}
impl Households {
    pub fn wards_valid(&self) -> bool {
        self.wards == index::build(&self.caregivers)
    }
}
