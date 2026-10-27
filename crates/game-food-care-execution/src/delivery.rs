//! Food delivery reports progress only after physical work or actual feeding.
use super::*;
fn retain(tx: &mut Transaction<'_>, p: PersonId, intent: Option<FoodCare>) -> Result<bool> {
    let working = intent.is_some() && tx.get::<Actions>()?.0.contains_key(&p);
    remember(tx, p, intent)?;
    Ok(working)
}
pub fn advance(
    tx: &mut Transaction<'_>,
    p: PersonId,
    mut intent: FoodCare,
    retry_alternative: &mut bool,
) -> Result<bool> {
    if tx.get::<Actions>()?.0.contains_key(&p) {
        return Ok(true);
    }
    if local(tx, p, intent.beneficiary) {
        let body = &tx.get::<game_physiology::Bodies>()?.people[&intent.beneficiary];
        if !body.alive {
            return retain(tx, p, None);
        }
        intent.observed_need = observed_need(tx, p, intent.beneficiary)?;
        intent.observed_at = tx.now();
        intent.home = game_transport::area(tx.get()?, p)?;
    }
    if concern(tx, p, intent.beneficiary, predicted(tx, p, &intent)) <= 0 {
        return retain(tx, p, None);
    }
    if !intent.returning {
        match game_provisioning::acquire(tx, p, &mut intent.plan) {
            Ok(false) => return retain(tx, p, Some(intent)),
            Err(_) => {
                game_provisioning::learn_failure(tx, p, &intent.plan);
                return retain(tx, p, None);
            }
            Ok(true) => {}
        }
        let Some(source) = intent.plan.inventory() else {
            remember(tx, p, None)?;
            prompt(tx, p)?;
            return Ok(false);
        };
        if !local(tx, p, intent.beneficiary) {
            let Some(pack) = tx.get::<game_cargo::Cargo>()?.carried(p).next() else {
                return retain(tx, p, None);
            };
            if pack != source {
                let Some((good, food)) = intended_food(tx, p, &intent.plan, source) else {
                    return retain(tx, p, None);
                };
                if tx
                    .attempt(game_property::Take {
                        actor: p,
                        transfer: game_cargo::Transfer {
                            from: source,
                            to: pack,
                            good,
                            quantity: food
                                .portion
                                .min(tx.get::<game_cargo::Cargo>()?.0[&source].amount(good)),
                        },
                        unauthorized: true,
                    })
                    .is_err()
                {
                    return retain(tx, p, None);
                }
                intent.plan = Plan::Meal {
                    inventory: pack,
                    good: Some(good),
                };
                tx.call(game_knowledge::ObserveStore {
                    person: p,
                    inventory: pack,
                })?;
            }
        }
        intent.returning = true;
    }
    if tx.get::<Actions>()?.0.contains_key(&p) {
        return retain(tx, p, Some(intent));
    }
    let here = game_transport::area(tx.get()?, p)?;
    if here != intent.home {
        let next = tx
            .get::<game_knowledge::Knowledge>()?
            .0
            .get(&p)
            .and_then(|facts| game_knowledge::route(facts, here, intent.home).ok())
            .and_then(|route| route.areas.get(1).copied());
        if let Some(destination) = next {
            remember(tx, p, Some(intent))?;
            if tx
                .attempt(game_transport::Move {
                    person: p,
                    destination,
                })
                .is_err()
            {
                remember(tx, p, None)?;
            }
            return Ok(tx.get::<Actions>()?.0.contains_key(&p));
        }
        return retain(tx, p, None);
    }
    if !local(tx, p, intent.beneficiary) {
        if tx.registered::<game_search::Seek>()
            && let Some(target) = tx.call(game_search::Seek {
                person: p,
                other: intent.beneficiary,
                last_known: intent.home,
            })?
        {
            intent.home = target;
            return retain(tx, p, Some(intent));
        }
        return retain(tx, p, None);
    }
    let inventory = intent.plan.inventory().unwrap();
    let before = intent.observed_need;
    let mut need = before;
    let mut fed = false;
    let mut withheld = false;
    let mut new_refusal = false;
    loop {
        if game_physiology::capable(tx, intent.beneficiary).is_err()
            || need == 0
            || concern(tx, p, intent.beneficiary, need) <= 0
        {
            break;
        }
        let Some((good, food)) = intended_food(tx, p, &intent.plan, inventory) else {
            break;
        };
        let held = tx.get::<game_cargo::Cargo>()?.0[&inventory].amount(good);
        let wanted =
            (u128::from(need) * u128::from(food.portion)).div_ceil(u128::from(food.relief));
        let quantity = wanted.min(u128::from(food.portion)).min(u128::from(held)) as u64;
        if quantity == 0 {
            break;
        }
        if refusals::blocked(tx, p, intent.beneficiary, good, quantity, need) {
            withheld = true;
            break;
        }
        let offer = game_physiology::Serve {
            caregiver: p,
            patient: intent.beneficiary,
            inventory,
            good,
            quantity,
        };
        let remember_refusal = tx.get::<Households>()?.food_refusal_window.is_some()
            && !tx.has_before::<game_physiology::TryServe>();
        let result = if remember_refusal {
            match tx.attempt(game_physiology::TryServe(offer)) {
                Ok(game_physiology::OfferOutcome::Accepted) => Ok(()),
                Ok(game_physiology::OfferOutcome::Declined) => {
                    refusals::remember(tx, p, intent.beneficiary, good, quantity, need)?;
                    withheld = true;
                    new_refusal = true;
                    Err(Error::Rejected("offered food declined".into()))
                }
                Err(error) => Err(error),
            }
        } else {
            tx.attempt(offer)
        };
        if let Err(error) = result {
            tx.emit(
                FeedingFailed {
                    caregiver: p,
                    patient: intent.beneficiary,
                    good,
                    quantity,
                    reason: error.to_string(),
                },
                vec![p, intent.beneficiary],
            )?;
            break;
        }
        fed = true;
        let estimated_relief = (u128::from(food.relief) * u128::from(quantity)
            / u128::from(food.portion))
        .min(u128::from(u64::MAX)) as u64;
        need = need.saturating_sub(estimated_relief);
    }
    tx.call(game_knowledge::ObserveStore {
        person: p,
        inventory,
    })?;
    let reconsider = (!withheld || new_refusal)
        && need > 0
        && need < before
        && concern(tx, p, intent.beneficiary, need) > 0;
    remember(tx, p, None)?;
    if reconsider {
        tx.schedule(tx.now().after(MINUTE)?, 45, Review { caregiver: p })?;
    }
    *retry_alternative = withheld;
    Ok(fed)
}
fn intended_food(
    tx: &Transaction<'_>,
    p: PersonId,
    plan: &Plan,
    inventory: InventoryId,
) -> Option<(GoodId, game_objects::Food)> {
    let good = tx
        .get::<game_knowledge::Knowledge>()
        .ok()?
        .0
        .get(&p)
        .and_then(|facts| plan.intended_good(facts));
    if let Some(good) = good {
        return tx
            .get::<game_objects::Objects>()
            .ok()?
            .food_rules()
            .get(&good)
            .cloned()
            .map(|food| (good, food));
    }
    game_nutrition::choose_meal(
        tx,
        p,
        &tx.get::<game_cargo::Cargo>().ok()?.0.get(&inventory)?.goods,
    )
}
