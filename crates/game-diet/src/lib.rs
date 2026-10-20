//! Pure nutrient reserves, learned food effects, dependence and dietary evaluations.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
mod diet;
mod evaluation;
pub use diet::{Diet, Habit, Learned, Status};
pub use evaluation::Evaluation;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effects {
    pub health: i32,
    pub mood: i32,
    pub alertness: i32,
}
