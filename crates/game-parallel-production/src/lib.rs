//! Data-defined local goods, reserved work and physical outputs.
mod phase;
pub use phase::Draft;
mod validation;
use game_parallel_people::Region;
use game_parallel_transport::Traveler;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub use game_parallel_production_model::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Production {
    pub max_work_hunger: u32,
    pub goods: Vec<Good>,
    pub recipes: Vec<Recipe>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recipe_categories: Vec<RecipeCategory>,
    pub inventories: Vec<Inventory>,
    #[serde(default)]
    pub jobs: Vec<Job>,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Production {
    /// Validate and prepare every reservation before mutating this owner.
    pub fn prepare_start(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
        requests: &[Start],
    ) -> Result<game_parallel_production_admission::Draft, String> {
        let people = self.validate_people(regions, travelers, now)?;
        game_parallel_production_admission::Context {
            jobs: &self.jobs,
            recipes: &self.recipes,
            inventories: &self.inventories,
            people: &people,
        }
        .prepare(requests)
    }
    pub fn start(
        &self,
        regions: &[Region],
        travelers: &[Traveler],
        now: u64,
        requests: &[Start],
    ) -> Result<Self, String> {
        let draft = self.prepare_start(regions, travelers, now, requests)?;
        let mut next = self.clone();
        draft.commit(&mut next.jobs, &mut next.inventories);
        Ok(next)
    }
    pub fn busy_people(&self) -> Vec<u64> {
        self.jobs
            .par_iter()
            .filter(|j| j.phase == JobPhase::Working)
            .map(|j| j.worker)
            .collect()
    }
}
