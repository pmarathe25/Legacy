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
    Hit {
        actor: PersonId,
        target: PersonId,
        amount: u64,
    },
}
pub fn resolve(b: &Battle, strength: &BTreeMap<PersonId, u64>) -> Vec<Effect> {
    resolve_with_agility(b, strength, &BTreeMap::new())
}
pub fn resolve_with_agility(
    b: &Battle,
    strength: &BTreeMap<PersonId, u64>,
    agility: &BTreeMap<PersonId, u64>,
) -> Vec<Effect> {
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
    let mut hits = vec![];
    for (person, order) in orders {
        if b.fighters[&person].health == 0 {
            continue;
        }
        if let OrderKind::Attack(target) = order
            && geometry::distance2(positions[&person], positions[&target])
                <= i128::from(b.reach).pow(2)
            && b.map.clear(positions[&person], positions[&target])
        {
            let base = b.damage.saturating_mul(
                strength
                    .get(&person)
                    .copied()
                    .unwrap_or(1000)
                    .saturating_add(100),
            ) / 1100;
            let power = base.max(u64::from(b.damage > 0));
            let power = if power == 0 {
                0
            } else {
                (power.saturating_mul(game_vitality::factor(
                    agility.get(&person).copied().unwrap_or(1000),
                )) / game_vitality::factor(agility.get(&target).copied().unwrap_or(1000)))
                .max(1)
            };
            let amount = damage.entry(target).or_default();
            *amount = amount.saturating_add(power);
            if power > 0 {
                hits.push(Effect::Hit {
                    actor: person,
                    target,
                    amount: power,
                });
            }
        }
    }
    effects.extend(damage.into_iter().map(|(p, q)| Effect::Damage(p, q)));
    effects.extend(hits);
    effects
}
