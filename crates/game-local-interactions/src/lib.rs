//! Transaction-local local interaction frames; feature owners retain authority.
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod resolution;
pub use resolution::{register, resolve};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meal {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
    /// The caregiver's perceived need, never authoritative patient state.
    pub observed_need: u64,
}
impl Meal {
    pub fn serve(&self) -> game_physiology::Serve {
        game_physiology::Serve {
            caregiver: self.caregiver,
            patient: self.patient,
            inventory: self.inventory,
            good: self.good,
            quantity: self.quantity,
        }
    }
}
/// Registered by the food-care owner. Unsupported selections use ordinary execution.
#[derive(Clone, Serialize, Deserialize)]
pub struct DescribeCare {
    pub person: PersonId,
    pub request: Pending,
}
impl Request for DescribeCare {
    const KEY: &'static str = "care.describe-local-interaction";
    type Response = Option<Meal>;
}
/// Complete an actual offer, including a typed refusal, under the care owner.
#[derive(Clone, Serialize, Deserialize)]
pub struct CompleteCare {
    pub meal: Meal,
    pub accepted: bool,
}
impl Request for CompleteCare {
    const KEY: &'static str = "care.complete-local-interaction";
    type Response = ();
}
