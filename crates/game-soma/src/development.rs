use super::*;
pub fn maximum_health(b: &Bodies, p: PersonId) -> u64 {
    b.people
        .get(&p)
        .map_or(b.rules.max_health, |p| p.maximum(&b.rules))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Express {
    pub person: PersonId,
}
impl Request for Express {
    const KEY: &'static str = "physiology.express";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Express>(|tx, r| {
        if !tx.get::<game_vitality::Vitality>().is_ok_and(|s| s.enabled) {
            if tx.registered::<game_nutrition::Register>() {
                tx.call(game_nutrition::Register { person: r.person })?;
            }
            return Ok(());
        }
        let body = tx.get::<Bodies>()?.people[&r.person];
        let max = body.maximum(&tx.get::<Bodies>()?.rules);
        if !tx
            .get::<game_vitality::Vitality>()?
            .people
            .contains_key(&r.person)
        {
            tx.call(game_vitality::Register {
                person: r.person,
                born: body.born,
                base_health: max,
                span: body
                    .life_limit
                    .map(|limit| SimDuration(limit.0.saturating_sub(body.born.0).max(1))),
            })?;
        }
        if let Some(cap) = tx.call(game_vitality::Develop { person: r.person })? {
            let p = tx.edit::<Bodies>()?.people.get_mut(&r.person).unwrap();
            p.health = (u128::from(p.health) * u128::from(cap.health) / u128::from(max))
                .min(u128::from(u64::MAX)) as u64;
            p.maximum_health = Some(cap.health);
            p.life_limit = cap.life_limit;
        }
        if tx.registered::<game_nutrition::Register>() {
            tx.call(game_nutrition::Register { person: r.person })?;
        }
        Ok(())
    })
}
pub(super) fn advance(
    body: &Body,
    rules: &Rules,
    at: SimTime,
    p: PersonId,
    traits: Option<&game_traits::Traits>,
    vitality: Option<&game_vitality::Vitality>,
    environment: game_body::Rates,
) -> Body {
    if !body.alive {
        return advance_body(body, rules, at);
    }
    let Some((traits, life)) = traits.zip(vitality.and_then(|v| v.people.get(&p))) else {
        return advance_body(body, rules, at);
    };
    let mut body = *body;
    if body.alive {
        let previous = body.maximum(rules);
        let maximum = game_vitality::maximum_health(traits, p, life, at);
        // Growth preserves the health fraction; it does not restore an injury.
        if maximum > previous {
            body.health = ((u128::from(body.health) * u128::from(maximum)
                + u128::from(previous) / 2)
                / u128::from(previous)) as u64;
        }
        body.maximum_health = Some(maximum);
        body.health = body.health.min(maximum);
    }
    game_body::advance_rates(
        &body,
        rules,
        at,
        game_metabolism::rates(traits, life, p, at, environment),
    )
}
pub(super) fn condition(tx: &mut Transaction<'_>) -> Result<()> {
    if !tx.get::<game_vitality::Vitality>().is_ok_and(|v| v.enabled) {
        return Ok(());
    }
    if tx.registered::<game_nutrition::SetDemand>() {
        let demands: Vec<_> = tx
            .get::<Bodies>()?
            .living()
            .filter(|(_, b)| b.kind == Kind::Mortal)
            .map(|(person, b)| (*person, b.metabolic_rates.energy.max(1)))
            .collect();
        for (person, per_mille) in demands {
            tx.call(game_nutrition::SetDemand { person, per_mille })?;
        }
    }
    let bodies = tx.get::<Bodies>()?;
    let rows = bodies
        .living()
        .map(|(id, p)| {
            let health =
                (u128::from(p.health) * 1000 / u128::from(p.maximum(&bodies.rules))) as u32;
            (
                *id,
                health
                    .saturating_sub(p.hunger.min(120) as u32 * 2 + p.fatigue.min(100) as u32 * 2)
                    .min(game_nutrition::status(tx, *id).quality),
            )
        })
        .collect();
    tx.call(game_vitality::Conditions(rows))
}
