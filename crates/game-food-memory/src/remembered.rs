use super::*;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{Arc, Weak},
};
type Rules = BTreeMap<GoodId, game_objects::Food>;
#[derive(Default)]
struct Cache {
    rules: Weak<Rules>,
    meals: BTreeMap<usize, (Weak<game_knowledge::FactValue>, bool)>,
}
thread_local! {static CACHE:RefCell<Cache>=RefCell::new(Cache::default());}
/// Only immutable personal evidence and authored food rules affect this projection.
pub fn remembered_meal(
    objects: &game_objects::Objects,
    value: &Arc<game_knowledge::FactValue>,
) -> bool {
    CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        if cache.rules.as_ptr() != Arc::as_ptr(objects.food_rules()) {
            cache.rules = Arc::downgrade(objects.food_rules());
            cache.meals.clear();
        }
        let key = Arc::as_ptr(value) as usize;
        if let Some((_, meal)) = cache.meals.get(&key) {
            return *meal;
        }
        let meal = value
            .decode::<game_knowledge::Stock>()
            .is_some_and(|s| objects.has_meal(&s));
        if cache.meals.len() >= 4096 {
            cache.meals.clear();
        }
        cache.meals.insert(key, (Arc::downgrade(value), meal));
        meal
    })
}
