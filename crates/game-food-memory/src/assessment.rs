use game_core::*;
use game_knowledge::{Facts, known};
use game_sim::Transaction;
pub type Cache = std::collections::BTreeMap<GoodId, (u64, i64)>;
pub fn food(
    tx: &Transaction<'_>,
    person: PersonId,
    good: GoodId,
    food: &game_objects::Food,
    cache: &mut Cache,
) -> (u64, i64) {
    if let Some(value) = cache.get(&good) {
        return *value;
    }
    let appetite = game_nutrition::appetite_for(tx, person, good);
    let preference = game_nutrition::preference(tx, person, good, food);
    // Unfamiliar sources can be worth trying when the current diet is unsatisfying.
    // This reveals neither their nutrients nor their effects before ingestion.
    let novelty = tx
        .get::<game_nutrition::Nutrition>()
        .ok()
        .filter(|n| n.enabled)
        .and_then(|n| n.people.get(&person))
        .filter(|d| !d.learned.contains_key(&good))
        .map_or(0, |_| game_nutrition::appetite(tx, person).min(80) as i64);
    let value = (
        appetite,
        preference.saturating_sub(food.relief.min(i64::MAX as u64) as i64) + novelty,
    );
    cache.insert(good, value);
    value
}
pub fn store_foods(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    inventory: InventoryId,
    cache: &mut Cache,
) -> Vec<(GoodId, u64, i64)> {
    store_foods_partial(tx, person, facts, inventory, cache, false)
}
pub fn store_foods_partial(
    tx: &Transaction<'_>,
    person: PersonId,
    facts: &Facts,
    inventory: InventoryId,
    cache: &mut Cache,
    partial: bool,
) -> Vec<(GoodId, u64, i64)> {
    let Some(stock) = known::<game_knowledge::Stock>(facts, &game_knowledge::store_key(inventory))
    else {
        return vec![];
    };
    stock_foods(tx, person, &stock, cache, partial)
}
pub fn stock_foods(
    tx: &Transaction<'_>,
    person: PersonId,
    stock: &game_knowledge::Stock,
    cache: &mut Cache,
    partial: bool,
) -> Vec<(GoodId, u64, i64)> {
    let Ok(objects) = tx.get::<game_objects::Objects>() else {
        return vec![];
    };
    stock
        .iter()
        .filter_map(|(good, q)| {
            let f = objects.food_rules().get(good)?;
            (*q >= if partial { 1 } else { f.portion }).then(|| {
                let (appetite, value) = food(tx, person, *good, f, cache);
                (*good, appetite, value)
            })
        })
        .collect()
}

pub fn self_supplied(
    tx: &Transaction<'_>,
    person: PersonId,
    inventory: InventoryId,
    carrier: Option<PersonId>,
) -> bool {
    carrier == Some(person)
        || (carrier.is_none()
            && tx
                .get::<game_property::Property>()
                .ok()
                .and_then(|p| p.claims.get(&inventory))
                .is_none_or(|claims| {
                    claims.is_empty() || claims.iter().any(|c| c.claimant == person)
                }))
}

/// Introspection and known personal stores do not compete with distant source recall.
/// Expressed intelligence bounds the other remembered suppliers considered together.
pub fn sources<'a>(
    tx: &Transaction<'_>,
    person: PersonId,
    stores: &'a crate::Stores,
) -> Vec<&'a crate::Store> {
    let mut preferred: std::collections::BTreeSet<_> =
        stores.carried(person).map(|s| s.id).collect();
    if let Ok(property) = tx.get::<game_property::Property>()
        && let Some(permitted) = property.permitted_set(person)
    {
        preferred.extend(
            permitted
                .keys()
                .filter(|id| {
                    property
                        .claims
                        .get(id)
                        .is_some_and(|claims| claims.iter().any(|c| c.claimant == person))
                })
                .copied(),
        );
    }
    let mut result: Vec<_> = preferred.iter().filter_map(|id| stores.meal(*id)).collect();
    let budget = (1 + game_vitality::ability(tx, person, "intelligence") / 250).min(64) as usize;
    let cue = random(tx.seed(), person.0, tx.now().0 / DAY.0);
    result.extend(
        stores
            .meals_from(cue)
            .filter(|s| !preferred.contains(&s.id))
            .take(budget),
    );
    result
}

/// Shared numerical context for a batch without retaining a transaction or world snapshot.
pub struct Assessor<'a> {
    evaluation: game_nutrition::FoodEvaluation<'a>,
    values: Cache,
}
impl<'a> Assessor<'a> {
    pub fn new(tx: &'a Transaction<'_>, person: PersonId) -> Self {
        Self {
            evaluation: game_nutrition::food_evaluation(tx, person),
            values: Cache::new(),
        }
    }
    pub fn food(&mut self, good: GoodId, food: &game_objects::Food) -> (u64, i64) {
        *self.values.entry(good).or_insert_with(|| {
            (
                self.evaluation.appetite_for(good),
                self.evaluation
                    .preference(good, food)
                    .saturating_sub(food.relief.min(i64::MAX as u64) as i64)
                    + self.evaluation.novelty(good),
            )
        })
    }
    pub fn stock_foods(
        &mut self,
        objects: &game_objects::Objects,
        stock: &game_knowledge::Stock,
        partial: bool,
    ) -> Vec<(GoodId, u64, i64)> {
        stock
            .iter()
            .filter_map(|(good, q)| {
                let f = objects.food_rules().get(good)?;
                (*q >= if partial { 1 } else { f.portion }).then(|| {
                    let (a, v) = self.food(*good, f);
                    (*good, a, v)
                })
            })
            .collect()
    }
}
