//! Reconsider only personally observed refusals; other foods and care stay live.
use super::*;

#[derive(Clone, Serialize, Deserialize)]
struct Retry {
    caregiver: PersonId,
    patient: PersonId,
    at: SimTime,
}
impl Request for Retry {
    const KEY: &'static str = "care.retry-food-refusal";
    type Response = ();
}

pub fn blocked(
    tx: &Transaction<'_>,
    caregiver: PersonId,
    patient: PersonId,
    good: GoodId,
    quantity: u64,
    need: u64,
) -> bool {
    tx.get::<Households>().is_ok_and(|state| {
        state.food_refusal_window.is_some()
            && state
                .food_refusals
                .get(&caregiver)
                .and_then(|patients| patients.get(&patient))
                .and_then(|group| group.foods.get(&good))
                .is_some_and(|rows| {
                    rows.iter().any(|refusal| {
                        tx.now() < refusal.until
                            && need <= refusal.observed_need
                            && quantity == refusal.quantity
                    })
                })
    })
}

/// Predict a serving only from the caregiver's remembered stock. Unknown and
/// forage plans stay eligible until their actual locally observed quantity exists.
pub fn known_quantity(
    tx: &Transaction<'_>,
    facts: &game_knowledge::Facts,
    plan: &Plan,
    good: GoodId,
    need: u64,
) -> Option<u64> {
    let Plan::Meal { inventory, .. } = plan else {
        return None;
    };
    let stock = game_knowledge::known::<game_knowledge::Stock>(
        facts,
        &game_knowledge::store_key(*inventory),
    )?;
    let held = *stock.get(&good)?;
    let food = tx
        .get::<game_objects::Objects>()
        .ok()?
        .food_rules()
        .get(&good)?
        .clone();
    if food.relief == 0 {
        return None;
    }
    let wanted = (u128::from(need) * u128::from(food.portion)).div_ceil(u128::from(food.relief));
    let quantity = wanted.min(u128::from(food.portion)).min(u128::from(held)) as u64;
    (quantity > 0).then_some(quantity)
}

fn schedule(
    tx: &mut Transaction<'_>,
    caregiver: PersonId,
    patient: PersonId,
    at: SimTime,
) -> Result<()> {
    let job = tx.schedule(
        at,
        45,
        Retry {
            caregiver,
            patient,
            at,
        },
    )?;
    tx.call(game_households::SetRefusalRetry {
        caregiver,
        patient,
        at,
        job,
    })
}

pub fn remember(
    tx: &mut Transaction<'_>,
    caregiver: PersonId,
    patient: PersonId,
    good: GoodId,
    quantity: u64,
    observed_need: u64,
) -> Result<()> {
    let Some(window) = tx.get::<Households>()?.food_refusal_window else {
        return Ok(());
    };
    let until = tx.now().after(window)?;
    let action = tx
        .get::<Actions>()?
        .0
        .get(&patient)
        .filter(|action| !["rest", "express-need"].contains(&action.kind.as_str()))
        .map(|action| action.id);
    let urgent = tx
        .get::<game_characters::Minds>()
        .ok()
        .is_some_and(|minds| observed_need >= minds.rules.urgent_hunger);
    tx.call(game_households::RememberFoodRefusal {
        caregiver,
        patient,
        good,
        refusal: game_households::FoodRefusal {
            observed_need,
            quantity,
            until,
            action,
            urgent,
        },
    })?;
    let group = &tx.get::<Households>()?.food_refusals[&caregiver][&patient];
    if group.retry.is_none() {
        let at = group
            .foods
            .values()
            .flatten()
            .map(|r| r.until)
            .min()
            .expect("recorded refusal");
        schedule(tx, caregiver, patient, at)?;
    }
    Ok(())
}

pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<Retry>(|tx, r| {
        let (active, next) = tx.call(game_households::ExpireFoodRefusals {
            caregiver: r.caregiver,
            patient: r.patient,
            at: r.at,
        })?;
        if !active {
            return Ok(());
        }
        if let Some(at) = next {
            schedule(tx, r.caregiver, r.patient, at)?;
        }
        prompt(tx, r.caregiver)
    })
}
pub fn finished(tx: &mut Transaction<'_>, event: &ActionFinished) -> Result<()> {
    if !tx
        .get::<Households>()
        .is_ok_and(|h| !h.food_refusals.is_empty())
    {
        return Ok(());
    }
    let caregivers = tx.call(game_households::ClearObservedFoodAction {
        patient: event.person,
        action: event.action,
    })?;
    for (caregiver, urgent) in caregivers {
        if urgent
            && tx
                .get::<game_characters::Minds>()
                .is_ok_and(|m| m.running && m.people.contains_key(&caregiver))
        {
            tx.notify(game_characters::Alert { person: caregiver })?;
        } else {
            prompt(tx, caregiver)?;
        }
    }
    Ok(())
}
