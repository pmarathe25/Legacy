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
    pub fn supply(
        &mut self,
        good: GoodId,
        amount: u64,
        visiting: &mut Vec<GoodId>,
        budget: &mut usize,
    ) -> Option<()> {
        let mut load = Load::default();
        for (good, quantity) in &self.stock {
            load.add(self.objects.mass(*good, *quantity).ok());
        }
        Journal {
            search: self,
            edits: vec![],
            load,
        }
        .supply(good, amount, visiting, budget)
    }
}
// Speculation changes one working stock. Only a winning branch retains its edits.
#[derive(Clone, Copy)]
enum Edit {
    Stock(GoodId, Option<u64>, u64),
    Gather(u64, Option<u64>, u64),
}
struct Journal<'s, 'a> {
    search: &'s mut Search<'a>,
    edits: Vec<Edit>,
    load: Load,
}
// A wider total can recover after an initially excessive load is consumed. Failed
// products and unknown goods remain invalid until that particular entry changes.
#[derive(Default)]
struct Load {
    total: u128,
    invalid: usize,
}
impl Load {
    fn add(&mut self, mass: Option<u64>) {
        if let Some(mass) = mass {
            self.total += u128::from(mass);
        } else {
            self.invalid += 1;
        }
    }
    fn remove(&mut self, mass: Option<u64>) {
        if let Some(mass) = mass {
            self.total -= u128::from(mass);
        } else {
            self.invalid -= 1;
        }
    }
}
struct Branch {
    edits: Vec<Edit>,
    steps: Vec<Step>,
    cost: u64,
}
impl Journal<'_, '_> {
    fn stock(&mut self, good: GoodId, value: u64) {
        let old = self.search.stock.insert(good, value);
        self.reweigh(good, old, Some(value));
        self.edits.push(Edit::Stock(good, old, value));
    }
    fn fits(&self) -> bool {
        self.load.invalid == 0 && self.load.total <= u128::from(self.search.capacity)
    }
    fn reweigh(&mut self, good: GoodId, old: Option<u64>, new: Option<u64>) {
        let unit = self.search.objects.mass(good, 1).ok();
        if let Some(old) = old {
            self.load.remove(unit.and_then(|m| m.checked_mul(old)));
        }
        if let Some(new) = new {
            self.load.add(unit.and_then(|m| m.checked_mul(new)));
        }
    }
    fn gather(&mut self, patch: u64, value: u64) {
        let old = self.search.gathered.insert(patch, value);
        self.edits.push(Edit::Gather(patch, old, value));
    }
    fn rollback(&mut self, mark: (usize, usize, u64)) {
        while self.edits.len() > mark.0 {
            match self.edits.pop().unwrap() {
                Edit::Stock(key, old, new) => {
                    self.reweigh(key, Some(new), old);
                    restore(&mut self.search.stock, key, old);
                }
                Edit::Gather(key, old, _) => restore(&mut self.search.gathered, key, old),
            }
        }
        self.search.steps.truncate(mark.1);
        self.search.cost = mark.2;
    }
    fn capture(&self, mark: (usize, usize, u64)) -> Branch {
        Branch {
            edits: self.edits[mark.0..].to_vec(),
            steps: self.search.steps.iter().skip(mark.1).cloned().collect(),
            cost: self.search.cost,
        }
    }
    fn supply(
        &mut self,
        good: GoodId,
        amount: u64,
        visiting: &mut Vec<GoodId>,
        budget: &mut usize,
    ) -> Option<()> {
        let mark = (self.edits.len(), self.search.steps.len(), self.search.cost);
        let depth = visiting.len();
        let result = self.explore(good, amount, visiting, budget);
        visiting.truncate(depth);
        if result.is_none() {
            self.rollback(mark);
        }
        result
    }
    fn explore(
        &mut self,
        good: GoodId,
        amount: u64,
        visiting: &mut Vec<GoodId>,
        budget: &mut usize,
    ) -> Option<()> {
        if *budget == 0 || !self.search.possible.contains(&good) {
            return None;
        }
        *budget -= 1;
        let held = self.search.stock.get(&good).copied().unwrap_or(0);
        if held >= amount {
            return Some(());
        }
        if visiting.len() >= self.search.depth
            || visiting.contains(&good)
            || self.search.steps.len() >= self.search.steps_limit
        {
            return None;
        }
        visiting.push(good);
        let mark = (self.edits.len(), self.search.steps.len(), self.search.cost);
        let missing = amount - held;
        let mut best: Option<Branch> = None;
        for (id, patch) in self.search.patches.iter().filter(|(_, p)| p.good == good) {
            let gathered = self.search.gathered.get(id).copied().unwrap_or(0);
            if patch.stock.saturating_sub(gathered) < missing {
                continue;
            }
            self.search.steps.push_back(Step::Gather {
                patch: *id,
                quantity: missing,
            });
            self.gather(*id, gathered + missing);
            self.stock(good, amount);
            if !self.fits() {
                self.rollback(mark);
                continue;
            }
            self.search.cost = self
                .search
                .cost
                .checked_add(game_ecology::GATHERING_TIME.0)?;
            best = Some(self.capture(mark));
            self.rollback(mark);
            break;
        }
        for (key, recipe) in self.search.recipes.get(&good).into_iter().flatten() {
            let batches = missing.div_ceil(recipe.quantity);
            if batches > 8 {
                continue;
            }
            let mut possible = true;
            for _ in 0..batches {
                for (input, quantity) in &recipe.inputs {
                    if self.supply(*input, *quantity, visiting, budget).is_none() {
                        possible = false;
                        break;
                    }
                    self.stock(*input, self.search.stock[input] - quantity);
                }
                if !possible || self.search.steps.len() >= self.search.steps_limit {
                    possible = false;
                    break;
                }
                self.search.steps.push_back(Step::Craft {
                    recipe: key.clone(),
                });
                self.stock(
                    good,
                    self.search
                        .stock
                        .get(&good)
                        .copied()
                        .unwrap_or(0)
                        .checked_add(recipe.quantity)?,
                );
                self.search.cost = self.search.cost.checked_add(recipe.duration.0)?;
                if !self.fits() {
                    possible = false;
                    break;
                }
            }
            if possible
                && self.search.stock.get(&good).copied().unwrap_or(0) >= amount
                && best.as_ref().is_none_or(|b| self.search.cost < b.cost)
            {
                best = Some(self.capture(mark));
            }
            self.rollback(mark);
        }
        visiting.pop();
        let best = best?;
        for edit in best.edits {
            match edit {
                Edit::Stock(key, _, value) => self.stock(key, value),
                Edit::Gather(key, _, value) => self.gather(key, value),
            }
        }
        self.search.steps.extend(best.steps);
        self.search.cost = best.cost;
        Some(())
    }
}
fn restore<K: Ord>(map: &mut BTreeMap<K, u64>, key: K, old: Option<u64>) {
    if let Some(value) = old {
        map.insert(key, value);
    } else {
        map.remove(&key);
    }
}
