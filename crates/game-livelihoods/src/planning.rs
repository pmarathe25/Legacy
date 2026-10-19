use super::*;
pub(super) use game_craft_planning::{MAX_STEPS, Search, Step};
pub(super) fn choose(
    tx: &Transaction<'_>,
    person: PersonId,
    inventories: &[InventoryId],
    goals: &[Goal],
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
    let perceived: BTreeMap<_, _> =
        game_food_memory::with_stores(person, facts, tx.get()?, |stores| {
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
    let mut totals: BTreeMap<GoodId, u64> = BTreeMap::new();
    let mut possible = BTreeMap::new();
    for id in inventories {
        for (good, quantity) in &perceived[id].1 {
            let total = totals.entry(*good).or_default();
            *total = total.saturating_add(*quantity);
        }
        let inputs = patches
            .iter()
            .filter(|(_, p)| p.stock > 0)
            .map(|(_, p)| p.good)
            .chain(
                perceived[id]
                    .1
                    .iter()
                    .filter(|(_, q)| **q > 0)
                    .map(|(g, _)| *g),
            );
        possible.insert(*id, workshop.reachable(inputs));
    }
    let traits = tx.get::<game_traits::Traits>().ok();
    let values = tx
        .get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.values.get(&person));
    let objects = tx.get::<game_objects::Objects>()?;
    let mut eligible = Vec::new();
    for (index, goal) in goals.iter().enumerate() {
        let good = objects.resolve(&goal.object)?;
        if totals.get(&good).copied().unwrap_or(0) < goal.quantity
            && inventories.iter().any(|id| possible[id].contains(&good))
        {
            eligible.push((index, goal));
        }
    }
    let mut affinities = BTreeMap::new();
    let mut goals: Vec<_> = eligible
        .into_iter()
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
    goals.sort_unstable_by_key(|(score, index, _)| (*score, *index));
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
            let (capacity, stock) = &perceived[inventory];
            let mut search = Search {
                recipes: &recipes,
                patches: &patches,
                stock: stock.iter().map(|(k, v)| (*k, *v)).collect(),
                steps: VecDeque::new(),
                gathered: BTreeMap::new(),
                cost: 0,
                objects: tx.get()?,
                capacity: *capacity,
                possible: &possible[inventory],
                depth: (2 + intelligence / 250).min(32) as usize,
                steps_limit: ((4 + intelligence / 50) as usize).min(MAX_STEPS),
            };
            if search
                .supply(
                    good,
                    goal.quantity - total + stock.get(&good).copied().unwrap_or(0),
                    &mut vec![],
                    &mut ((32 + intelligence / 2).min(8192) as usize),
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
