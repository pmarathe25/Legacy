//! Inherited potential, age-related expression, and experienced condition.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
mod growth;
mod projection;
pub use game_expression::{Life, Upbringing, maturation};
pub use growth::{
    attention_horizon, can_converse, can_manipulate, can_walk, disposition, switching_margin,
};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vitality {
    pub enabled: bool,
    pub people: Table<PersonId, Life>,
}
impl Resource for Vitality {
    const KEY: &'static str = "vitality.lives";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.people.values())
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        self.validate_rows(self.people.changed(&old.people).map(|(_, life)| life))
    }
}
impl Vitality {
    fn validate_rows<'a>(&self, mut rows: impl Iterator<Item = &'a Life>) -> Result<()> {
        require(
            rows.all(|p| {
                p.base_health > 0
                    && p.physical <= 1000
                    && p.stress <= 1000
                    && (-1000..=1000).contains(&p.satisfaction)
                    && p.span.is_none_or(|s| s.0 > 0)
            }),
            "invalid constitution or condition",
        )
    }
}
/// Ordinary potential is 1000; zero ability still permits impaired function.
pub fn factor(ability: u64) -> u64 {
    500u64.saturating_add(ability / 2)
}
fn scaled(value: u64, numerator: u64, denominator: u64) -> u64 {
    (u128::from(value) * u128::from(numerator) / u128::from(denominator)).min(u128::from(u64::MAX))
        as u64
}
pub fn expressed(
    traits: Option<&game_traits::Traits>,
    life: Option<&Life>,
    person: PersonId,
    key: &str,
    at: SimTime,
) -> u64 {
    let raw = traits.map_or(1000, |t| t.ability(person, key));
    life.map_or(raw, |life| {
        game_expression::ability(life, person, key, at, raw)
    })
}
pub fn ability(tx: &Transaction<'_>, person: PersonId, key: &str) -> u64 {
    projection::ability(tx, person, key)
}
pub fn condition(tx: &Transaction<'_>, person: PersonId) -> Option<(u32, i32)> {
    Some(
        tx.get::<Vitality>()
            .ok()?
            .people
            .get(&person)?
            .condition(tx.now()),
    )
}
pub fn maximum_health(
    traits: &game_traits::Traits,
    person: PersonId,
    life: &Life,
    at: SimTime,
) -> u64 {
    scaled(
        scaled(
            life.base_health,
            factor(traits.ability(person, "constitution")),
            1000,
        ),
        life.aging(at, false) * life.maturity(at, "constitution") / 1000,
        1000,
    )
    .max(1)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "vitality.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Register {
    pub person: PersonId,
    pub born: SimTime,
    pub span: Option<SimDuration>,
    pub base_health: u64,
}
impl Request for Register {
    const KEY: &'static str = "vitality.register";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Develop {
    pub person: PersonId,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Capacity {
    pub health: u64,
    pub life_limit: Option<SimTime>,
}
impl Request for Develop {
    const KEY: &'static str = "vitality.develop";
    type Response = Option<Capacity>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Experience {
    pub person: PersonId,
    pub stress: i32,
    pub satisfaction: i32,
}
impl Request for Experience {
    const KEY: &'static str = "vitality.experience";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Conditions(pub Vec<(PersonId, u32)>);
impl Request for Conditions {
    const KEY: &'static str = "vitality.conditions";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Assess {
    pub person: PersonId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assessment {
    pub physical: u32,
    pub stress: u32,
    pub satisfaction: i32,
    pub physical_aging: u64,
    pub cognitive_aging: u64,
    pub current_traits: std::collections::BTreeMap<String, u64>,
    pub development: std::collections::BTreeMap<String, u64>,
    pub upbringing: Upbringing,
}
impl Request for Assess {
    const KEY: &'static str = "vitality.assess";
    type Response = Option<Assessment>;
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("vitality", 2)?;
    b.resource::<Vitality>()?;
    b.request::<Assess>(|tx, r| {
        let Some(life) = tx.get::<Vitality>()?.people.get(&r.person) else {
            return Ok(None);
        };
        let (stress, satisfaction) = life.condition(tx.now());
        let current_traits = tx
            .get::<game_traits::Traits>()?
            .definitions
            .keys()
            .map(|key| (key.clone(), ability(tx, r.person, key)))
            .collect();
        Ok(Some(Assessment {
            physical: life.physical,
            stress,
            satisfaction,
            physical_aging: life.aging(tx.now(), false),
            cognitive_aging: life.aging(tx.now(), true),
            current_traits,
            development: [
                "size",
                "strength",
                "speed",
                "intelligence",
                "foresight",
                "learning",
                "language",
                "resilience",
            ]
            .into_iter()
            .map(|key| (key.into(), life.maturity(tx.now(), key)))
            .collect(),
            upbringing: life.upbringing_at(tx.now()),
        }))
    })?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Vitality>()?.enabled = true;
        Ok(())
    })?;
    b.request::<Register>(|tx, r| {
        require(
            r.born <= tx.now() && r.base_health > 0 && r.span.is_none_or(|s| s.0 > 0),
            "invalid development baseline",
        )?;
        require(
            !tx.get::<Vitality>()?.people.contains_key(&r.person),
            "constitution already registered",
        )?;
        let at = tx.now();
        tx.edit::<Vitality>()?.people.insert(
            r.person,
            Life {
                born: r.born,
                span: r.span,
                base_health: r.base_health,
                expressed: false,
                physical: 1000,
                stress: 0,
                satisfaction: 0,
                updated: at,
                upbringing: Upbringing {
                    since: at,
                    ..Default::default()
                },
            },
        );
        Ok(())
    })?;
    b.request::<Develop>(|tx, r| {
        let Some(mut life) = tx
            .get::<Vitality>()?
            .people
            .get(&r.person)
            .copied()
            .filter(|p| !p.expressed)
        else {
            return Ok(None);
        };
        let traits = tx.get::<game_traits::Traits>()?;
        life.span = life.span.map(|s| {
            SimDuration(scaled(s.0, factor(traits.ability(r.person, "longevity")), 1000).max(1))
        });
        life.expressed = true;
        let capacity = Capacity {
            health: maximum_health(traits, r.person, &life, tx.now()),
            life_limit: life.span.map(|s| life.born.after(s)).transpose()?,
        };
        tx.edit::<Vitality>()?.people.insert(r.person, life);
        Ok(Some(capacity))
    })?;
    b.request::<Experience>(|tx, r| {
        let now = tx.now();
        let resilience = ability(tx, r.person, "resilience");
        let Some(life) = tx.edit::<Vitality>()?.people.get_mut(&r.person) else {
            return Ok(());
        };
        life.grow(now);
        let (stress, satisfaction) = life.condition(now);
        let change = if r.stress > 0 {
            scaled(r.stress as u64, 1000, factor(resilience)).min(1000) as i32
        } else {
            r.stress
        };
        life.stress = (i64::from(stress) + i64::from(change)).clamp(0, 1000) as u32;
        life.satisfaction = satisfaction
            .saturating_add(r.satisfaction)
            .clamp(-1000, 1000);
        life.updated = now;
        Ok(())
    })?;
    b.request::<Conditions>(|tx, r| {
        let now = tx.now();
        for (person, physical) in r.0 {
            require(physical <= 1000, "invalid physical well-being")?;
            if let Some(life) = tx.edit::<Vitality>()?.people.get_mut(&person) {
                life.grow(now);
                life.physical = physical;
            }
        }
        Ok(())
    })?;
    Ok(())
}
