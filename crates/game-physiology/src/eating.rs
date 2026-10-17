use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub struct ConsumeEdible {
    pub person: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for ConsumeEdible {
    const KEY: &'static str = "physiology.consume-edible";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Serve {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub inventory: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Serve {
    const KEY: &'static str = "physiology.serve";
    type Response = ();
}
fn consume(
    tx: &mut Transaction<'_>,
    carer: PersonId,
    person: PersonId,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
) -> Result<()> {
    capable(tx, carer)?;
    capable(tx, person)?;
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
    if carer != person {
        require(
            tx.get::<Actions>()?
                .0
                .get(&person)
                .is_none_or(|a| a.kind == "rest"),
            "recipient is occupied",
        )?;
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
        require(
            (hunger.min(relief) * 2)
                .saturating_add(appetite)
                .saturating_add(preference)
                .saturating_sub(excess.saturating_mul(2))
                > 10,
            "offered food declined",
        )?;
    }
    tx.call(game_property::Use {
        actor: carer,
        inventory,
        purpose: if carer == person { "eating" } else { "feeding" }.into(),
    })?;
    tx.call(game_cargo::Consume {
        inventory,
        good,
        quantity,
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
    )
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Serve>(|tx, r| {
        consume(tx, r.caregiver, r.patient, r.inventory, r.good, r.quantity)
    })?;
    b.request::<ConsumeEdible>(|tx, r| {
        consume(tx, r.person, r.person, r.inventory, r.good, r.quantity)
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
        consume(tx, r.caregiver, r.patient, r.inventory, good, food.portion)
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
