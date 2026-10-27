//! Opt-in self-meal wave: ordered admission and stock, then disjoint body rows.
use super::*;

/// Meals settle before post-meal observers are notified. This phase model is
/// distinct from calling ConsumeEdible actor by actor.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConsumeSelfMeals {
    pub meals: Vec<ConsumeEdible>,
    pub minimum_parallel: usize,
}
impl Request for ConsumeSelfMeals {
    const KEY: &'static str = "physiology.consume-self-meals";
    type Response = ();
}

/// Strict wave: rejection leaves fallback to the caller's enclosing attempt.
#[derive(Clone, Serialize, Deserialize)]
pub struct TryConsumeSelfMeals(pub ConsumeSelfMeals);
impl Request for TryConsumeSelfMeals {
    const KEY: &'static str = "physiology.self-meal-wave";
    type Response = ();
}

fn scalar(tx: &mut Transaction<'_>, meals: Vec<ConsumeEdible>) -> Result<()> {
    for meal in meals {
        tx.call(meal)?;
    }
    Ok(())
}

pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<ConsumeSelfMeals>(|tx, batch| {
        require(batch.meals.len() <= 100_000, "too many self meals")?;
        if tx.attempt(TryConsumeSelfMeals(batch.clone())).is_err() {
            return scalar(tx, batch.meals);
        }
        Ok(())
    })?;
    b.request::<TryConsumeSelfMeals>(|tx, TryConsumeSelfMeals(batch)| {
        require(batch.meals.len() <= 100_000, "too many self meals")?;
        let people: std::collections::BTreeSet<_> = batch.meals.iter().map(|m| m.person).collect();
        require(
            people.len() == batch.meals.len()
                && !tx.has_before::<ConsumeEdible>()
                && !tx.has_before::<game_digestion::Ingest>()
                && !tx.has_before::<game_nutrition::Meal>()
                && !tx.has_before::<game_soma::Nourish>()
                && !tx.has_before::<game_soma::NourishMany>()
                && !tx.has_before::<game_vitality::Experience>()
                && !tx.has_before::<CheckSelfMeal>(),
            "self meal needs ordered execution",
        )?;
        let initial: Vec<_> = batch
            .meals
            .iter()
            .map(|meal| {
                tx.get::<Bodies>()
                    .ok()
                    .and_then(|b| b.people.get(&meal.person).copied())
            })
            .collect();
        let mut nourishment = Vec::with_capacity(batch.meals.len());
        let mut moods = Vec::with_capacity(batch.meals.len());
        let mut body_versions = Vec::with_capacity(batch.meals.len());
        let mut areas = Vec::with_capacity(batch.meals.len());
        for (meal, original) in batch.meals.iter().zip(initial) {
            if tx.registered::<CheckSelfMeal>() {
                require(
                    tx.call(CheckSelfMeal {
                        person: meal.person,
                        inventory: meal.inventory,
                        good: meal.good,
                        quantity: meal.quantity,
                    })?,
                    "self meal admission changed",
                )?;
            }
            require(
                tx.get::<Bodies>()?.people.get(&meal.person).copied() == original,
                "self meal body changed",
            )?;
            capable(tx, meal.person)?;
            require(
                game_vitality::can_manipulate(tx, meal.person),
                "feeding exceeds motor capacity",
            )?;
            require(
                !tx.get::<Actions>()?.0.contains_key(&meal.person),
                "actor is busy",
            )?;
            let area = game_transport::area(tx.get()?, meal.person)?;
            require(
                game_cargo::locality(tx, meal.inventory)? == area,
                "feeding requires contact",
            )?;
            let food = tx
                .get::<game_objects::Objects>()?
                .by_id(meal.good)?
                .food
                .clone()
                .ok_or_else(|| Error::Rejected("not edible".into()))?;
            require(meal.quantity > 0, "empty meal")?;
            tx.call(game_property::Use {
                actor: meal.person,
                inventory: meal.inventory,
                purpose: "eating".into(),
            })?;
            tx.notify(Eating {
                person: meal.person,
            })?;
            tx.call(game_cargo::Consume {
                inventory: meal.inventory,
                good: meal.good,
                quantity: meal.quantity,
            })?;
            tx.call(game_digestion::Ingest {
                person: meal.person,
                amount: (u128::from(meal.quantity) * 1000 / u128::from(food.portion))
                    .min(u128::from(u64::MAX)) as u64,
            })?;
            let effects = if tx.registered::<game_nutrition::Meal>() {
                tx.call(game_nutrition::Meal {
                    person: meal.person,
                    good: meal.good,
                    quantity: meal.quantity,
                })?
            } else {
                game_nutrition::Effects::default()
            };
            nourishment.push(game_soma::Nourish {
                person: meal.person,
                relief: (u128::from(food.relief) * u128::from(meal.quantity)
                    / u128::from(food.portion))
                .min(u128::from(u64::MAX)) as u64,
                health: effects.health,
                alertness: effects.alertness,
            });
            moods.push(effects.mood);
            body_versions.push(tx.get::<Bodies>()?.people[&meal.person]);
            areas.push(game_transport::area(tx.get()?, meal.person)?);
        }
        // Death and any intervening invalidation require the original full path.
        let bodies = tx.get::<Bodies>()?;
        for (((effect, prior), meal), area) in nourishment
            .iter()
            .zip(&body_versions)
            .zip(&batch.meals)
            .zip(areas)
        {
            let body = bodies
                .people
                .get(&effect.person)
                .ok_or_else(|| Error::Missing("body".into()))?;
            let mut projected = *body;
            game_body::apply_nourishment(&mut projected, body.maximum(&bodies.rules), effect);
            require(
                body == prior && body.alive && projected.health > 0,
                "self meal needs ordered body handling",
            )?;
            require(
                !tx.get::<Actions>()?.0.contains_key(&meal.person),
                "self meal actor became busy",
            )?;
            require(
                game_transport::area(tx.get()?, meal.person).ok() == Some(area)
                    && game_cargo::locality(tx, meal.inventory).ok() == Some(area)
                    && game_vitality::can_manipulate(tx, meal.person),
                "self meal access changed",
            )?;
        }
        tx.call(game_soma::NourishMany {
            effects: nourishment,
            minimum_parallel: batch.minimum_parallel,
        })?;
        for (meal, &mood) in batch.meals.iter().zip(&moods) {
            if tx.registered::<game_vitality::Experience>() && mood != 0 {
                tx.call(game_vitality::Experience {
                    person: meal.person,
                    stress: -mood.saturating_div(2),
                    satisfaction: mood,
                })?;
            }
        }
        for meal in batch.meals {
            tx.emit(
                Fed {
                    inventory: Some(meal.inventory),
                    caregiver: meal.person,
                    patient: meal.person,
                    good: meal.good,
                    quantity: meal.quantity,
                },
                vec![meal.person],
            )?;
        }
        Ok(())
    })
}
