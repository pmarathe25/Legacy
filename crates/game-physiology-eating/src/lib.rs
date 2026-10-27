//! Receiver-owned voluntary serving and eating transactions.
use game_body::{ConsumeEdible, Eating, Fed, Serve};
use game_core::*;
use game_sim::*;
use game_soma::{Bodies, capable};
use serde::{Deserialize, Serialize};
mod local_meals;
pub use local_meals::ServeLocalMeals;
#[derive(Clone, Serialize, Deserialize)]
pub struct Eat {
    pub person: PersonId,
    pub inventory: InventoryId,
}
impl Request for Eat {
    const KEY: &'static str = "physiology.eat";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Feed {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub inventory: InventoryId,
}
impl Request for Feed {
    const KEY: &'static str = "physiology.feed";
    type Response = ();
}
fn consume(
    tx: &mut Transaction<'_>,
    carer: PersonId,
    person: PersonId,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
    forced: bool,
) -> Result<()> {
    match consume_outcome(tx, carer, person, inventory, good, quantity, forced)? {
        OfferOutcome::Accepted => Ok(()),
        OfferOutcome::Declined => Err(Error::Rejected("offered food declined".into())),
    }
}
fn consume_outcome(
    tx: &mut Transaction<'_>,
    carer: PersonId,
    person: PersonId,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
    forced: bool,
) -> Result<OfferOutcome> {
    let Some(food) = admit(tx, carer, person, inventory, good, quantity, forced)? else {
        return Ok(OfferOutcome::Declined);
    };
    tx.call(game_property::Use {
        actor: carer,
        inventory,
        purpose: if carer == person { "eating" } else { "feeding" }.into(),
    })?;
    tx.notify(Eating { person })?;
    tx.call(game_cargo::Consume {
        inventory,
        good,
        quantity,
    })?;
    tx.call(game_digestion::Ingest {
        person,
        amount: (u128::from(quantity) * 1000 / u128::from(food.portion)).min(u128::from(u64::MAX))
            as u64,
    })?;
    let effects = if tx.registered::<game_nutrition::Meal>() {
        tx.call(game_nutrition::Meal {
            person,
            good,
            quantity,
        })?
    } else {
        game_nutrition::Effects::default()
    };
    let relief = (u128::from(food.relief) * u128::from(quantity) / u128::from(food.portion))
        .min(u128::from(u64::MAX)) as u64;
    tx.call(game_soma::Nourish {
        person,
        relief,
        health: effects.health,
        alertness: effects.alertness,
    })?;
    if tx.registered::<game_vitality::Experience>() && effects.mood != 0 {
        tx.call(game_vitality::Experience {
            person,
            stress: -effects.mood.saturating_div(2),
            satisfaction: effects.mood,
        })?;
    }
    tx.emit(
        Fed {
            inventory: Some(inventory),
            caregiver: carer,
            patient: person,
            good,
            quantity,
        },
        vec![carer, person]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
    )?;
    Ok(OfferOutcome::Accepted)
}
pub(crate) fn admit(
    tx: &mut Transaction<'_>,
    carer: PersonId,
    person: PersonId,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
    forced: bool,
) -> Result<Option<game_objects::Food>> {
    capable(tx, carer)?;
    require(
        game_vitality::can_manipulate(tx, carer),
        "feeding exceeds motor capacity",
    )?;
    require(
        tx.get::<Bodies>()?
            .people
            .get(&person)
            .is_some_and(|b| b.alive),
        "patient is dead or unknown",
    )?;
    require(
        !tx.get::<Actions>()?.0.contains_key(&carer),
        "actor is busy",
    )?;
    let area = game_transport::area(tx.get()?, person)?;
    require(
        game_transport::area(tx.get()?, carer)? == area
            && game_cargo::locality(tx, inventory)? == area,
        "feeding requires contact",
    )?;
    let food = tx
        .get::<game_objects::Objects>()?
        .by_id(good)?
        .food
        .clone()
        .ok_or_else(|| Error::Rejected("not edible".into()))?;
    require(quantity > 0, "empty meal")?;
    if forced {
        require(
            tx.registered::<Restrained>()
                && tx.call(Restrained {
                    actor: carer,
                    patient: person,
                })?,
            "administration needs physical restraint",
        )?;
        tx.cancel_action(person)?;
    }
    if carer != person && !forced {
        let action = tx.get::<Actions>()?.0.get(&person);
        require(
            action.is_none_or(|a| a.kind != "combat"),
            "recipient is in combat",
        )?;
        let commitment = action
            .filter(|a| !["rest", "express-need"].contains(&a.kind.as_str()))
            .map_or(0, |a| {
                a.until.map_or(30, |until| {
                    (until.0.saturating_sub(tx.now().0) / MINUTE.0).min(120) as i128
                }) + i128::from(game_vitality::ability(tx, person, "attention").min(16000) / 100)
            });
        let hunger = i128::from(tx.get::<Bodies>()?.people[&person].hunger.min(1000));
        let preference = game_nutrition::preference(tx, person, good, &food)
            .saturating_sub(food.relief.min(i64::MAX as u64) as i64);
        let appetite = i128::from(game_nutrition::appetite_for(tx, person, good).min(1000));
        let relief = (u128::from(food.relief) * u128::from(quantity) / u128::from(food.portion))
            .min(i128::MAX as u128) as i128;
        let preference = i128::from(preference) * i128::from(quantity) / i128::from(food.portion);
        let excess = relief
            .saturating_sub(hunger + appetite + i128::from(food.relief))
            .max(0);
        if (hunger.min(relief) * 2)
            .saturating_add(appetite)
            .saturating_add(preference)
            .saturating_sub(excess.saturating_mul(2))
            <= 10 + commitment
        {
            return Ok(None);
        }
        if tx
            .get::<Actions>()?
            .0
            .get(&person)
            .is_some_and(|a| a.kind != "rest")
        {
            tx.cancel_action(person)?;
        }
    }
    Ok(Some(food))
}
/// Typed result of an actual voluntary offer, not a prediction of private need.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OfferOutcome {
    Accepted,
    Declined,
}
/// Submit a voluntary offer while distinguishing refusal from other failures.
/// The `Serve` hooks run inside the same rollback boundary as the offer.
/// Hooks directly on `TryServe` require callers to use `Serve` instead.
#[derive(Clone, Serialize, Deserialize)]
pub struct TryServe(pub Serve);
impl Request for TryServe {
    const KEY: &'static str = "physiology.try-serve";
    type Response = OfferOutcome;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Restrained {
    pub actor: PersonId,
    pub patient: PersonId,
}
impl Request for Restrained {
    const KEY: &'static str = "physiology.restrained";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Administer(pub Serve);
impl Request for Administer {
    const KEY: &'static str = "physiology.administer";
    type Response = ();
}
pub fn register(b: &mut Builder) -> Result<()> {
    local_meals::register(b)?;
    b.request::<TryServe>(|tx, r| {
        require(
            !tx.has_before::<TryServe>(),
            "typed offer hooks require ordered execution",
        )?;
        tx.attempt_owned_request(r.0, |tx, r| {
            let outcome = consume_outcome(
                tx,
                r.caregiver,
                r.patient,
                r.inventory,
                r.good,
                r.quantity,
                false,
            )?;
            Ok(match outcome {
                OfferOutcome::Accepted => AttemptOutcome::Commit(outcome),
                OfferOutcome::Declined => AttemptOutcome::Rollback(outcome),
            })
        })
    })?;
    b.request::<Administer>(|tx, r| {
        consume(
            tx,
            r.0.caregiver,
            r.0.patient,
            r.0.inventory,
            r.0.good,
            r.0.quantity,
            true,
        )
    })?;
    b.request::<Serve>(|tx, r| {
        consume(
            tx,
            r.caregiver,
            r.patient,
            r.inventory,
            r.good,
            r.quantity,
            false,
        )
    })?;
    b.request::<ConsumeEdible>(|tx, r| {
        consume(
            tx,
            r.person,
            r.person,
            r.inventory,
            r.good,
            r.quantity,
            false,
        )
    })?;
    b.request::<Eat>(|tx, r| {
        let inventory = tx
            .get::<game_cargo::Cargo>()?
            .0
            .get(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        let (good, food) = game_nutrition::choose_meal(tx, r.person, &inventory.goods)
            .ok_or_else(|| Error::Rejected("no edible portion available".into()))?;
        tx.call(ConsumeEdible {
            person: r.person,
            inventory: r.inventory,
            good,
            quantity: food.portion,
        })
    })?;
    b.request::<Feed>(|tx, r| {
        let inventory = tx
            .get::<game_cargo::Cargo>()?
            .0
            .get(&r.inventory)
            .ok_or_else(|| Error::Missing("inventory".into()))?;
        let (good, food) = game_nutrition::choose_meal(tx, r.caregiver, &inventory.goods)
            .ok_or_else(|| Error::Rejected("no edible portion".into()))?;
        consume(
            tx,
            r.caregiver,
            r.patient,
            r.inventory,
            good,
            food.portion,
            false,
        )
    })?;
    b.subscribe::<game_nutrition::Withdrawal>("physiology.withdrawal", |tx, e| {
        if capable(tx, e.person).is_ok() {
            tx.call(game_soma::Nourish {
                person: e.person,
                relief: 0,
                health: 0,
                alertness: -(e.fatigue.min(i32::MAX as u64) as i32),
            })?;
        }
        Ok(())
    })?;
    Ok(())
}
