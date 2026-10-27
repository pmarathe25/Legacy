//! Independent personal diets settle on private rows before owner publication.
use super::*;
use rayon::prelude::*;

#[derive(Clone, Serialize, Deserialize)]
pub struct Meals {
    pub meals: Vec<Meal>,
    pub minimum_parallel: usize,
}
impl Request for Meals {
    const KEY: &'static str = "nutrition.meals";
    type Response = Vec<Effects>;
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Meals>(|tx, batch| {
        require(batch.meals.len() <= 100_000, "too many meals")?;
        require(
            !tx.has_before::<Meal>() && !tx.has_before::<Register>(),
            "meal hooks require ordered execution",
        )?;
        let unique: std::collections::BTreeSet<_> = batch.meals.iter().map(|m| m.person).collect();
        require(
            unique.len() == batch.meals.len(),
            "overlapping meal recipients",
        )?;
        if !tx.get::<Nutrition>()?.enabled {
            game_diagnostics::interaction_profile!({
                game_diagnostics::count("interaction.owner.nutrition.disabled.calls", 1);
                game_diagnostics::count(
                    "interaction.owner.nutrition.disabled.rows",
                    batch.meals.len() as u64,
                );
            });
            return Ok(vec![Effects::default(); batch.meals.len()]);
        }
        let mut rows = Vec::with_capacity(batch.meals.len());
        for meal in batch.meals {
            let food = tx
                .get::<game_objects::Objects>()?
                .by_id(meal.good)?
                .food
                .clone()
                .ok_or_else(|| Error::Rejected("not edible".into()))?;
            require(meal.quantity > 0, "empty meal")?;
            tx.call(Register {
                person: meal.person,
            })?;
            rows.push((
                meal.clone(),
                food,
                tx.get::<Nutrition>()?.people[&meal.person].clone(),
                Effects::default(),
            ));
        }
        let now = tx.now();
        let needs = &tx.get::<Nutrition>()?.needs;
        let apply =
            |(meal, food, diet, effects): &mut (Meal, game_objects::Food, Diet, Effects)| {
                *effects = diet.consume(needs, meal.good, food, meal.quantity, now);
            };
        if rows.len() >= batch.minimum_parallel && tx.compute().current_num_threads() > 1 {
            game_diagnostics::interaction_profile!(batch "nutrition", rows.len(), true);
            tx.compute().install(|| rows.par_iter_mut().for_each(apply));
        } else {
            game_diagnostics::interaction_profile!(batch "nutrition", rows.len(), false);
            rows.iter_mut().for_each(apply);
        }
        let state = tx.edit::<Nutrition>()?;
        let mut effects = Vec::with_capacity(rows.len());
        for (meal, _, diet, effect) in rows {
            state.people.insert(meal.person, diet);
            effects.push(effect);
        }
        Ok(effects)
    })
}
