//! Local interaction boundary: ordered physical stock, disjoint personal effects.
use super::*;
use std::collections::BTreeSet;

/// Strict local-meal phase. Call inside an enclosing attempt before choosing
/// ordered fallback. All personal effects settle before any Fed listeners run.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServeLocalMeals {
    pub meals: Vec<Serve>,
    pub minimum_parallel: usize,
}
impl Request for ServeLocalMeals {
    const KEY: &'static str = "physiology.serve-local-meals";
    type Response = Vec<OfferOutcome>;
}
struct Accepted {
    meal: Serve,
    food: game_objects::Food,
}
fn prepare(tx: &mut Transaction<'_>, meal: Serve) -> Result<AttemptOutcome<Option<Accepted>>> {
    if meal.caregiver == meal.patient && tx.registered::<game_body::CheckSelfMeal>() {
        require(
            tx.call(game_body::CheckSelfMeal {
                person: meal.patient,
                inventory: meal.inventory,
                good: meal.good,
                quantity: meal.quantity,
            })?,
            "local meal admission changed",
        )?;
    }
    if tx.registered::<game_body::CheckLocalMeal>() {
        require(
            tx.call(game_body::CheckLocalMeal(meal.clone()))?,
            "local meal intention changed",
        )?;
    }
    require(
        !tx.get::<Actions>()?.0.contains_key(&meal.patient),
        "local meal recipient became busy",
    )?;
    let Some(food) = admit(
        tx,
        meal.caregiver,
        meal.patient,
        meal.inventory,
        meal.good,
        meal.quantity,
        false,
    )?
    else {
        return Ok(AttemptOutcome::Rollback(None));
    };
    // Admission callbacks may themselves affect actions; no cancellation is
    // permitted in this slice, including for the recipient of a care offer.
    require(
        !tx.get::<Actions>()?.0.contains_key(&meal.patient),
        "local meal recipient became busy",
    )?;
    tx.call(game_property::Use {
        actor: meal.caregiver,
        inventory: meal.inventory,
        purpose: if meal.caregiver == meal.patient {
            "eating"
        } else {
            "feeding"
        }
        .into(),
    })?;
    tx.notify(Eating {
        person: meal.patient,
    })?;
    tx.call(game_cargo::Consume {
        inventory: meal.inventory,
        good: meal.good,
        quantity: meal.quantity,
    })?;
    Ok(AttemptOutcome::Commit(Some(Accepted { meal, food })))
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<ServeLocalMeals>(|tx, batch| {
        require(batch.meals.len() <= 100_000, "too many local meals")?;
        require(
            !tx.has_before::<TryServe>()
                && !tx.has_before::<ConsumeEdible>()
                && !tx.has_before::<game_digestion::Ingest>()
                && !tx.has_before::<game_digestion::IngestMany>()
                && !tx.has_before::<game_nutrition::Meal>()
                && !tx.has_before::<game_nutrition::Meals>()
                && !tx.has_before::<game_nutrition::Register>()
                && !tx.has_before::<game_soma::Nourish>()
                && !tx.has_before::<game_soma::NourishMany>()
                && !tx.has_before::<game_vitality::Experience>()
                && !tx.has_before::<game_vitality::Experiences>(),
            "personal effect hooks require ordered execution",
        )?;
        let mut people = BTreeSet::new();
        for meal in &batch.meals {
            require(
                people.insert(meal.caregiver),
                "overlapping local meal participants",
            )?;
            if meal.patient != meal.caregiver {
                require(
                    people.insert(meal.patient),
                    "overlapping local meal participants",
                )?;
            }
        }
        let initial: Vec<_> = people
            .into_iter()
            .map(|person| {
                require(
                    !tx.get::<Actions>()?.0.contains_key(&person),
                    "local meal participant busy",
                )?;
                capable(tx, person)?;
                let area = game_transport::area(tx.get()?, person)?;
                Ok((person, tx.get::<Bodies>()?.people[&person], area))
            })
            .collect::<Result<_>>()?;
        let mut accepted = Vec::new();
        let mut outcomes = Vec::with_capacity(batch.meals.len());
        for meal in batch.meals {
            let prepared = tx.attempt_owned_request(meal, prepare)?;
            if let Some(prepared) = prepared {
                outcomes.push(OfferOutcome::Accepted);
                accepted.push(prepared);
            } else {
                outcomes.push(OfferOutcome::Declined);
            }
        }
        // A hook that changed another participant must use the ordered path.
        for (person, body, area) in &initial {
            require(
                tx.get::<Bodies>()?.people.get(person) == Some(body)
                    && !tx.get::<Actions>()?.0.contains_key(person)
                    && game_transport::area(tx.get()?, *person).ok() == Some(*area),
                "local meal participant changed",
            )?;
        }
        for entry in &accepted {
            require(
                game_cargo::locality(tx, entry.meal.inventory)?
                    == game_transport::area(tx.get()?, entry.meal.patient)?
                    && game_vitality::can_manipulate(tx, entry.meal.caregiver),
                "local meal access changed",
            )?;
            require(
                admit(
                    tx,
                    entry.meal.caregiver,
                    entry.meal.patient,
                    entry.meal.inventory,
                    entry.meal.good,
                    entry.meal.quantity,
                    false,
                )?
                .is_some(),
                "local meal assent changed",
            )?;
        }
        tx.call(game_digestion::IngestMany {
            effects: accepted
                .iter()
                .map(|entry| game_digestion::Ingest {
                    person: entry.meal.patient,
                    amount: (u128::from(entry.meal.quantity) * 1000
                        / u128::from(entry.food.portion))
                    .min(u128::from(u64::MAX)) as u64,
                })
                .collect(),
            minimum_parallel: batch.minimum_parallel,
        })?;
        let effects = if tx.registered::<game_nutrition::Meals>() {
            tx.call(game_nutrition::Meals {
                meals: accepted
                    .iter()
                    .map(|entry| game_nutrition::Meal {
                        person: entry.meal.patient,
                        good: entry.meal.good,
                        quantity: entry.meal.quantity,
                    })
                    .collect(),
                minimum_parallel: batch.minimum_parallel,
            })?
        } else {
            require(
                !tx.registered::<game_nutrition::Meal>(),
                "nutrition batch owner unavailable",
            )?;
            vec![game_nutrition::Effects::default(); accepted.len()]
        };
        let nourishment: Vec<_> = accepted
            .iter()
            .zip(&effects)
            .map(|(entry, effect)| game_soma::Nourish {
                person: entry.meal.patient,
                relief: (u128::from(entry.food.relief) * u128::from(entry.meal.quantity)
                    / u128::from(entry.food.portion))
                .min(u128::from(u64::MAX)) as u64,
                health: effect.health,
                alertness: effect.alertness,
            })
            .collect();
        for effect in &nourishment {
            let bodies = tx.get::<Bodies>()?;
            let mut projected = bodies.people[&effect.person];
            let maximum = projected.maximum(&bodies.rules);
            game_body::apply_nourishment(&mut projected, maximum, effect);
            require(
                projected.alive && projected.health > 0,
                "lethal local meal requires ordered handling",
            )?;
        }
        tx.call(game_soma::NourishMany {
            effects: nourishment,
            minimum_parallel: batch.minimum_parallel,
        })?;
        if tx.registered::<game_vitality::Experiences>() {
            tx.call(game_vitality::Experiences {
                effects: accepted
                    .iter()
                    .zip(effects)
                    .filter(|(_, effect)| effect.mood != 0)
                    .map(|(entry, effect)| game_vitality::Experience {
                        person: entry.meal.patient,
                        stress: -effect.mood.saturating_div(2),
                        satisfaction: effect.mood,
                    })
                    .collect(),
                minimum_parallel: batch.minimum_parallel,
            })?;
        } else {
            require(
                !tx.registered::<game_vitality::Experience>(),
                "vitality batch owner unavailable",
            )?;
        }
        for entry in accepted {
            let meal = entry.meal;
            tx.emit(
                Fed {
                    inventory: Some(meal.inventory),
                    caregiver: meal.caregiver,
                    patient: meal.patient,
                    good: meal.good,
                    quantity: meal.quantity,
                },
                BTreeSet::from([meal.caregiver, meal.patient])
                    .into_iter()
                    .collect(),
            )?;
        }
        Ok(outcomes)
    })
}
