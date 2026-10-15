use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderKind {
    Move(Point),
    Attack(PersonId),
    Wait,
    Retreat,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Moved(PersonId, Point),
    Damage(PersonId, u64),
    Retreated(PersonId),
}
pub fn resolve(b: &Battle) -> Vec<Effect> {
    let mut positions: BTreeMap<_, _> = b.fighters.iter().map(|(p, f)| (*p, f.point)).collect();
    let mut orders = b.orders.clone();
    for (person, f) in &b.fighters {
        if f.health == 0 || orders.contains_key(person) {
            continue;
        }
        let target = b
            .fighters
            .iter()
            .filter(|(_, t)| t.health > 0 && t.team != f.team)
            .min_by_key(|(p, t)| (geometry::distance2(f.point, t.point), **p));
        orders.insert(
            *person,
            target.map_or(OrderKind::Wait, |(p, _)| OrderKind::Attack(*p)),
        );
    }
    let mut effects = vec![];
    for (person, order) in &orders {
        if b.fighters[person].health == 0 {
            continue;
        }
        match order {
            OrderKind::Move(p) => {
                positions.insert(*person, *p);
                effects.push(Effect::Moved(*person, *p));
            }
            OrderKind::Retreat => effects.push(Effect::Retreated(*person)),
            _ => {}
        }
    }
    let mut damage = BTreeMap::<PersonId, u64>::new();
    for (person, order) in orders {
        if b.fighters[&person].health == 0 {
            continue;
        }
        if let OrderKind::Attack(target) = order
            && geometry::distance2(positions[&person], positions[&target])
                <= i128::from(b.reach).pow(2)
            && b.map.clear(positions[&person], positions[&target])
        {
            let amount = damage.entry(target).or_default();
            *amount = amount.saturating_add(b.damage);
        }
    }
    effects.extend(damage.into_iter().map(|(p, q)| Effect::Damage(p, q)));
    effects
}
