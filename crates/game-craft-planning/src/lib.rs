use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
pub const MAX_STEPS: usize = 256;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Step {
    Gather { patch: u64, quantity: u64 },
    Craft { recipe: String },
}
#[derive(Clone)]
pub struct Search<'a> {
    pub recipes: &'a game_workshop::RecipeChoices,
    pub patches: &'a [(u64, game_ecology::Patch)],
    pub stock: BTreeMap<GoodId, u64>,
    pub steps: VecDeque<Step>,
    pub gathered: BTreeMap<u64, u64>,
    pub cost: u64,
    pub objects: &'a game_objects::Objects,
    pub capacity: u64,
    pub possible: &'a std::collections::BTreeSet<GoodId>,
    pub depth: usize,
    pub steps_limit: usize,
}
impl Search<'_> {
    fn fits(&self) -> bool {
        self.stock
            .iter()
            .try_fold(0u64, |mass, (good, quantity)| {
                mass.checked_add(self.objects.mass(*good, *quantity).ok()?)
            })
            .is_some_and(|mass| mass <= self.capacity)
    }
    pub fn supply(
        &mut self,
        good: GoodId,
        amount: u64,
        visiting: &mut Vec<GoodId>,
        budget: &mut usize,
    ) -> Option<()> {
        if *budget == 0 || !self.possible.contains(&good) {
            return None;
        }
        *budget -= 1;
        let held = self.stock.get(&good).copied().unwrap_or(0);
        if held >= amount {
            return Some(());
        }
        if visiting.len() >= self.depth
            || visiting.contains(&good)
            || self.steps.len() >= self.steps_limit
        {
            return None;
        }
        visiting.push(good);
        let missing = amount - held;
        let mut best: Option<Self> = None;
        for (id, patch) in self.patches.iter().filter(|(_, p)| p.good == good) {
            if patch
                .stock
                .saturating_sub(*self.gathered.get(id).unwrap_or(&0))
                < missing
            {
                continue;
            }
            let mut trial = self.clone();
            trial.steps.push_back(Step::Gather {
                patch: *id,
                quantity: missing,
            });
            *trial.gathered.entry(*id).or_default() += missing;
            trial.stock.insert(good, amount);
            if !trial.fits() {
                continue;
            }
            trial.cost = trial.cost.checked_add(game_ecology::GATHERING_TIME.0)?;
            best = Some(trial);
            break;
        }
        for (key, recipe) in self.recipes.get(&good).into_iter().flatten() {
            let batches = missing.div_ceil(recipe.quantity);
            if batches > 8 {
                continue;
            }
            let mut trial = self.clone();
            let mut possible = true;
            for _ in 0..batches {
                for (input, quantity) in &recipe.inputs {
                    if trial.supply(*input, *quantity, visiting, budget).is_none() {
                        possible = false;
                        break;
                    }
                    *trial.stock.get_mut(input).unwrap() -= *quantity;
                }
                if !possible || trial.steps.len() >= self.steps_limit {
                    possible = false;
                    break;
                }
                trial.steps.push_back(Step::Craft {
                    recipe: key.clone(),
                });
                *trial.stock.entry(good).or_default() = trial
                    .stock
                    .get(&good)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(recipe.quantity)?;
                trial.cost = trial.cost.checked_add(recipe.duration.0)?;
                if !trial.fits() {
                    possible = false;
                    break;
                }
            }
            if possible && best.as_ref().is_none_or(|b| trial.cost < b.cost) {
                best = Some(trial);
            }
        }
        visiting.pop();
        *self = best?;
        Some(())
    }
}
