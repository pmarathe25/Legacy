use super::*;
pub(super) const MAX_STEPS: usize = 256;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Step {
    Gather { patch: u64, quantity: u64 },
    Craft { recipe: String },
}
#[derive(Clone)]
struct Search<'a> {
    recipes: &'a game_workshop::RecipeChoices,
    patches: &'a [(u64, game_ecology::Patch)],
    stock: BTreeMap<GoodId, u64>,
    steps: VecDeque<Step>,
    gathered: BTreeMap<u64, u64>,
    cost: u64,
    objects: &'a game_objects::Objects,
    capacity: u64,
    possible: &'a std::collections::BTreeSet<GoodId>,
    depth: usize,
    steps_limit: usize,
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
    fn supply(
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
            trial.cost = trial.cost.checked_add(5 * MINUTE.0)?;
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
pub(super) fn choose(
    tx: &Transaction<'_>,
    person: PersonId,
    inventories: &[InventoryId],
) -> Result<Option<Plan>> {
    let _measurement = game_diagnostics::span("work planning");
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
        return Ok(None);
    };
    let area = game_transport::area(tx.get()?, person)?;
    // Only the person's remembered local patches can motivate a plan; reality is checked on dispatch.
    let patches: Vec<_> = game_knowledge::with_prefix(facts, "patch:")
        .filter_map(|(key, fact)| {
            let patch: game_ecology::Patch = fact.value.decode()?;
            (patch.area == area).then_some((key.strip_prefix("patch:")?.parse().ok()?, patch))
        })
        .collect();
    let workshop = tx.get::<game_workshop::Workshop>()?;
    let recipes = workshop.choices();
    let cargo = tx.get::<game_cargo::Cargo>()?;
    let mut totals: BTreeMap<GoodId, u64> = BTreeMap::new();
    let mut possible = BTreeMap::new();
    for id in inventories {
        for (good, quantity) in &cargo.0[id].goods {
            let total = totals.entry(*good).or_default();
            *total = total.saturating_add(*quantity);
        }
        let inputs = patches
            .iter()
            .filter(|(_, p)| p.stock > 0)
            .map(|(_, p)| p.good)
            .chain(
                cargo.0[id]
                    .goods
                    .iter()
                    .filter(|(_, q)| **q > 0)
                    .map(|(g, _)| *g),
            );
        possible.insert(*id, workshop.reachable(inputs));
    }
    let rules = tx.get::<Livelihoods>()?.rules.as_ref().unwrap();
    let traits = tx.get::<game_traits::Traits>().ok();
    let values = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&person));
    let mut affinities = BTreeMap::new();
    let mut goals: Vec<_> = rules
        .goals
        .iter()
        .enumerate()
        .map(|(index, g)| {
            let affinity = *affinities
                .entry(g.affinity.as_str())
                .or_insert_with(|| traits.map_or(500, |t| t.disposition(person, &g.affinity)));
            let learned = values
                .and_then(|v| v.get(g.concept.as_str()))
                .copied()
                .unwrap_or(0);
            let score = i64::from(g.priority) * 10
                + affinity
                + i64::from(learned) * 8
                + (random(tx.seed(), person.0, index as u64) % 600) as i64;
            (std::cmp::Reverse(score), index, g)
        })
        .collect();
    goals.sort_by_key(|(score, index, _)| (*score, *index));
    let intelligence = game_vitality::ability(tx, person, "intelligence");
    for (_, _, goal) in goals {
        let good = tx.get::<game_objects::Objects>()?.resolve(&goal.object)?;
        // A stock goal covers all encountered accessible supplies, not one duplicate set per pack.
        let total = totals.get(&good).copied().unwrap_or(0);
        if total >= goal.quantity {
            continue;
        }
        for inventory in inventories {
            if !possible[inventory].contains(&good) {
                continue;
            }
            let source = &cargo.0[inventory];
            let mut search = Search {
                recipes: &recipes,
                patches: &patches,
                stock: source.goods.iter().map(|(k, v)| (*k, *v)).collect(),
                steps: VecDeque::new(),
                gathered: BTreeMap::new(),
                cost: 0,
                objects: tx.get()?,
                capacity: source.capacity - source.reserved_mass,
                possible: &possible[inventory],
                depth: (2 + intelligence / 250).min(32) as usize,
                steps_limit: ((4 + intelligence / 50) as usize).min(MAX_STEPS),
            };
            if search
                .supply(
                    good,
                    goal.quantity - total + source.amount(good),
                    &mut vec![],
                    &mut ((32 + intelligence / 2).min(8192) as usize),
                )
                .is_some()
                && !search.steps.is_empty()
            {
                return Ok(Some(Plan {
                    inventory: *inventory,
                    goal: good,
                    quantity: goal.quantity - total + source.amount(good),
                    concept: goal.concept.clone(),
                    steps: search.steps,
                }));
            }
        }
    }
    Ok(None)
}
