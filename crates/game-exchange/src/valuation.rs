use super::*;
fn unit(tx: &Transaction<'_>, p: PersonId, g: GoodId) -> Result<i64> {
    let o = tx.get::<game_objects::Objects>()?.by_id(g)?;
    let body = &tx.get::<game_physiology::Bodies>()?.people[&p];
    let use_value = if let Some(food) = &o.food {
        (40 + body.hunger.min(200) as i64 + game_nutrition::preference(tx, p, g, food).max(0))
            / food.portion.max(1) as i64
    } else if o.tags.contains("medical")
        && body.health < game_physiology::maximum_health(tx.get()?, p)
    {
        80
    } else if let Some(warmth) = o
        .tags
        .iter()
        .find_map(|t| t.strip_prefix("warmth:")?.parse::<i32>().ok())
    {
        let cold = game_transport::area(tx.get()?, p)
            .ok()
            .and_then(|a| game_environment::at(tx, a).ok())
            .map_or(0, |s| (10 - s.warmth).max(0));
        5 + i64::from(cold.min(warmth).max(0)) * 5
    } else {
        10
    };
    let interest = tx
        .get::<game_relationships::Social>()?
        .values
        .get(&p)
        .and_then(|v| v.get(o.name.as_str()))
        .copied()
        .unwrap_or(0);
    Ok((use_value + i64::from(interest) + game_demand::resale(tx, p, g)).max(1))
}
fn utility(tx: &Transaction<'_>, p: PersonId, unit: i64, quantity: u64) -> i64 {
    let horizon = u128::from((16 + game_vitality::ability(tx, p, "foresight") / 20).min(1024));
    (unit as u128 * u128::from(quantity) * horizon / (horizon + u128::from(quantity)))
        .min(i64::MAX as u128) as i64
}
pub(super) fn gain(
    tx: &Transaction<'_>,
    p: PersonId,
    inv: InventoryId,
    give: GoodId,
    give_q: u64,
    get: GoodId,
    get_q: u64,
) -> Result<i64> {
    let stock = &tx.get::<game_cargo::Cargo>()?.0[&inv];
    let held = stock.amount(give);
    if held < give_q {
        return Ok(i64::MIN);
    }
    let receive = stock.amount(get);
    let after = receive.checked_add(get_q).ok_or(Error::Overflow)?;
    let giving = unit(tx, p, give)?;
    let receiving = unit(tx, p, get)?;
    let cost = utility(tx, p, giving, held) - utility(tx, p, giving, held - give_q);
    let benefit = utility(tx, p, receiving, after) - utility(tx, p, receiving, receive);
    Ok(benefit - cost - game_sentiment::relinquishing(tx, p, inv, give, give_q))
}

/// Reuse one person's estimates only while enumerating an unchanged offer snapshot.
/// No estimate survives a transaction or exposes the other party's private values.
pub(super) struct Offers {
    person: PersonId,
    inventory: InventoryId,
    units: std::collections::BTreeMap<GoodId, (u64, i64)>,
    outgoing: std::collections::BTreeMap<(GoodId, u64), (i64, i64)>,
    incoming: std::collections::BTreeMap<(GoodId, u64), i64>,
}
impl Offers {
    pub(super) fn new(person: PersonId, inventory: InventoryId) -> Self {
        Self {
            person,
            inventory,
            units: Default::default(),
            outgoing: Default::default(),
            incoming: Default::default(),
        }
    }
    fn quote(&mut self, tx: &Transaction<'_>, good: GoodId) -> Result<(u64, i64)> {
        if let Some(quote) = self.units.get(&good) {
            return Ok(*quote);
        }
        let held = tx.get::<game_cargo::Cargo>()?.0[&self.inventory].amount(good);
        let quote = (held, unit(tx, self.person, good)?);
        self.units.insert(good, quote);
        Ok(quote)
    }
    pub(super) fn accepts(&mut self, tx: &Transaction<'_>, r: &Trade) -> Result<bool> {
        let (cost, attachment) = self.cost(tx, r.requested, r.requested_quantity)?;
        let benefit = self.benefit(tx, r.offered, r.offered_quantity)?;
        Ok(benefit - cost - attachment > 0)
    }
    pub(super) fn cost(
        &mut self,
        tx: &Transaction<'_>,
        give: GoodId,
        q: u64,
    ) -> Result<(i64, i64)> {
        if let Some((cost, attachment)) = self.outgoing.get(&(give, q)) {
            return Ok((*cost, *attachment));
        }
        let (held, giving) = self.quote(tx, give)?;
        let cost =
            utility(tx, self.person, giving, held) - utility(tx, self.person, giving, held - q);
        let attachment = game_sentiment::relinquishing(tx, self.person, self.inventory, give, q);
        self.outgoing.insert((give, q), (cost, attachment));
        Ok((cost, attachment))
    }
    pub(super) fn benefit(&mut self, tx: &Transaction<'_>, get: GoodId, q: u64) -> Result<i64> {
        if let Some(benefit) = self.incoming.get(&(get, q)) {
            return Ok(*benefit);
        }
        let (held, receiving) = self.quote(tx, get)?;
        let after = held.checked_add(q).ok_or(Error::Overflow)?;
        let benefit =
            utility(tx, self.person, receiving, after) - utility(tx, self.person, receiving, held);
        self.incoming.insert((get, q), benefit);
        Ok(benefit)
    }
}

/// One person's current use, resale and sentimental value of an incoming or outgoing quantity.
pub fn marginal(
    tx: &Transaction<'_>,
    p: PersonId,
    inventory: InventoryId,
    good: GoodId,
    quantity: u64,
    incoming: bool,
) -> Result<i64> {
    let held = tx.get::<game_cargo::Cargo>()?.0[&inventory].amount(good);
    let price = unit(tx, p, good)?;
    let current = utility(tx, p, price, held);
    if incoming {
        Ok(utility(
            tx,
            p,
            price,
            held.checked_add(quantity).ok_or(Error::Overflow)?,
        ) - current)
    } else {
        require(quantity <= held, "insufficient offered goods")?;
        Ok(current - utility(tx, p, price, held - quantity)
            + game_sentiment::relinquishing(tx, p, inventory, good, quantity))
    }
}
pub fn credit_trust(tx: &Transaction<'_>, p: PersonId, q: PersonId) -> i64 {
    tx.get::<game_relationships::Social>()
        .ok()
        .and_then(|s| s.relationships.get(&p)?.get(&q))
        .map_or(0, |r| {
            i64::from(r.trust + r.respect / 2 - r.grievance - r.fear)
        })
}

pub fn accept_promise(
    tx: &Transaction<'_>,
    recipient: PersonId,
    inventory: InventoryId,
    term: &game_agreements::Term,
    good: GoodId,
    quantity: u64,
) -> Result<bool> {
    let confidence = (credit_trust(tx, recipient, term.debtor)
        + game_vitality::disposition(tx, recipient, "risk-tolerance") / 20)
        .clamp(0, 100);
    let delay = term.due.0.saturating_sub(tx.now().0) / DAY.0;
    let expected = i128::from(marginal(
        tx,
        recipient,
        inventory,
        term.good,
        term.quantity,
        true,
    )?) * i128::from(confidence)
        / i128::from(100 + delay.min(1000));
    Ok(confidence >= 30
        && expected >= i128::from(marginal(tx, recipient, inventory, good, quantity, false)?))
}
