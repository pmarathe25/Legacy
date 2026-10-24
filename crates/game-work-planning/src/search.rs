use super::*;
use std::collections::BTreeMap;
/// A private read view: the search receives no transaction or mutation capability.
pub(crate) struct Inputs<'a> {
    person: PersonId,
    seed: u64,
    intelligence: u64,
    inventories: &'a [InventoryId],
    goals: &'a [Goal],
    patches: Vec<(u64, game_ecology::Patch)>,
    workshop: &'a game_workshop::Workshop,
    objects: &'a game_objects::Objects,
    perceived: BTreeMap<InventoryId, (u64, game_knowledge::Stock)>,
    affinities: BTreeMap<&'a str, i64>,
    values: Option<&'a game_storage::Table<std::sync::Arc<str>, i32>>,
}
pub(crate) fn capture<'a>(
    tx: &'a Transaction<'_>,
    person: PersonId,
    inventories: &'a [InventoryId],
    goals: &'a [Goal],
) -> Result<Option<Inputs<'a>>> {
    let Some(facts) = tx.get::<game_knowledge::Knowledge>()?.0.get(&person) else {
        return Ok(None);
    };
    let area = game_transport::area(tx.get()?, person)?;
    let patches = game_knowledge::with_prefix(facts, "patch:")
        .filter_map(|(key, fact)| {
            let patch: game_ecology::Patch = fact.value.decode()?;
            (patch.area == area).then_some((key.strip_prefix("patch:")?.parse().ok()?, patch))
        })
        .collect();
    let workshop = tx.get::<game_workshop::Workshop>()?;
    let perceived = game_food_memory::with_stores(person, facts, tx.get()?, |stores| {
        inventories
            .iter()
            .filter_map(|id| {
                let store = stores.get(*id)?;
                Some((
                    *id,
                    (store.location.as_ref()?.capacity, store.stock.clone()?),
                ))
            })
            .collect()
    });
    let traits = tx.get::<game_traits::Traits>().ok();
    let values = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&person));
    let objects = tx.get::<game_objects::Objects>()?;
    let affinities = goals
        .iter()
        .map(|g| {
            (
                g.affinity.as_str(),
                traits.map_or(500, |t| t.disposition(person, &g.affinity)),
            )
        })
        .collect();
    Ok(Some(Inputs {
        person,
        seed: tx.seed(),
        intelligence: game_vitality::ability(tx, person, "intelligence"),
        inventories,
        goals,
        patches,
        workshop,
        objects,
        perceived,
        affinities,
        values,
    }))
}
impl Inputs<'_> {
    pub(crate) fn run(&self) -> Result<Option<Plan>> {
        let recipes = self.workshop.choices();
        let mut totals: BTreeMap<GoodId, u64> = BTreeMap::new();
        let mut possible = BTreeMap::new();
        for id in self.inventories {
            for (good, quantity) in &self.perceived[id].1 {
                let total = totals.entry(*good).or_default();
                *total = total.saturating_add(*quantity);
            }
            let inputs = self
                .patches
                .iter()
                .filter(|(_, p)| p.stock > 0)
                .map(|(_, p)| p.good)
                .chain(
                    self.perceived[id]
                        .1
                        .iter()
                        .filter(|(_, q)| **q > 0)
                        .map(|(g, _)| *g),
                );
            possible.insert(*id, self.workshop.reachable(inputs));
        }
        let mut eligible = Vec::new();
        for (index, goal) in self.goals.iter().enumerate() {
            let good = self.objects.resolve(&goal.object)?;
            if totals.get(&good).copied().unwrap_or(0) < goal.quantity
                && self
                    .inventories
                    .iter()
                    .any(|id| possible[id].contains(&good))
            {
                eligible.push((index, goal));
            }
        }
        let mut ranked: Vec<_> = eligible
            .into_iter()
            .map(|(index, g)| {
                let affinity = self.affinities[g.affinity.as_str()];
                let learned = self
                    .values
                    .and_then(|v| v.get(g.concept.as_str()))
                    .copied()
                    .unwrap_or(0);
                let score = i64::from(g.priority) * 10
                    + affinity
                    + i64::from(learned) * 8
                    + (random(self.seed, self.person.0, index as u64) % 600) as i64;
                (std::cmp::Reverse(score), index, g)
            })
            .collect();
        ranked.sort_unstable_by_key(|(score, index, _)| (*score, *index));
        for (_, _, goal) in ranked {
            let good = self.objects.resolve(&goal.object)?;
            // A stock goal covers all encountered accessible supplies, not one duplicate set per pack.
            let total = totals.get(&good).copied().unwrap_or(0);
            if total >= goal.quantity {
                continue;
            }
            for inventory in self.inventories {
                if !possible[inventory].contains(&good) {
                    continue;
                }
                let (capacity, stock) = &self.perceived[inventory];
                let mut search = Search {
                    recipes: &recipes,
                    patches: &self.patches,
                    stock: stock.iter().map(|(k, v)| (*k, *v)).collect(),
                    steps: VecDeque::new(),
                    gathered: BTreeMap::new(),
                    cost: 0,
                    objects: self.objects,
                    capacity: *capacity,
                    possible: &possible[inventory],
                    depth: (2 + self.intelligence / 250).min(32) as usize,
                    steps_limit: ((4 + self.intelligence / 50) as usize).min(MAX_STEPS),
                };
                if search
                    .supply(
                        good,
                        goal.quantity - total + stock.get(&good).copied().unwrap_or(0),
                        &mut vec![],
                        &mut ((32 + self.intelligence / 2).min(8192) as usize),
                    )
                    .is_some()
                    && !search.steps.is_empty()
                {
                    return Ok(Some(Plan {
                        inventory: *inventory,
                        goal: good,
                        quantity: goal.quantity - total + stock.get(&good).copied().unwrap_or(0),
                        concept: goal.concept.clone(),
                        steps: search.steps,
                    }));
                }
            }
        }
        Ok(None)
    }
}
