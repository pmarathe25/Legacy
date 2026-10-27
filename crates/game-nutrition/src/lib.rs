//! Actual diets, experienced food effects, and substance dependence.
mod batch;
pub use batch::Meals;
mod motives;
use game_core::*;
pub use game_diet::{Diet, Effects, Evaluation as FoodEvaluation, Habit, Learned, Status};
use game_sim::*;
use game_storage::{Records, Table};
pub use motives::appetite_for;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nutrition {
    pub enabled: bool,
    pub needs: BTreeMap<String, u64>,
    pub people: Records<PersonId, Diet>,
}
impl Default for Nutrition {
    fn default() -> Self {
        Self {
            enabled: false,
            people: Records::new(),
            needs: [
                ("energy", 30),
                ("protein", 8),
                ("fat", 4),
                ("fibre", 4),
                ("vitamins", 4),
                ("minerals", 3),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v))
            .collect(),
        }
    }
}
impl Resource for Nutrition {
    const KEY: &'static str = "nutrition.diets";
    fn validate(&self) -> Result<()> {
        require(
            self.needs.values().all(|n| *n > 0 && *n <= 1000),
            "invalid dietary needs",
        )?;
        require(
            self.people.values().all(|d| {
                d.requirement_per_mille > 0
                    && d.habits
                        .values()
                        .all(|h| h.level <= 1000 && h.tolerance <= 1000)
            }),
            "invalid dependence",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        require(self.needs == old.needs, "dietary requirements are frozen")?;
        require(
            self.people.changed(&old.people).all(|(_, d)| {
                d.requirement_per_mille > 0
                    && d.habits
                        .values()
                        .all(|h| h.level <= 1000 && h.tolerance <= 1000)
            }),
            "invalid dependence",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "nutrition.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Register {
    pub person: PersonId,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SetDemand {
    pub person: PersonId,
    pub per_mille: u64,
}
impl Request for SetDemand {
    const KEY: &'static str = "nutrition.demand";
    type Response = ();
}
impl Request for Register {
    const KEY: &'static str = "nutrition.register";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Deactivate {
    pub person: PersonId,
}
impl Request for Deactivate {
    const KEY: &'static str = "nutrition.deactivate";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Meal {
    pub person: PersonId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Meal {
    const KEY: &'static str = "nutrition.meal";
    type Response = Effects;
}
#[derive(Clone, Serialize, Deserialize)]
struct Review;
impl Request for Review {
    const KEY: &'static str = "nutrition.review";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Withdrawal {
    pub person: PersonId,
    pub fatigue: u64,
    pub distress: u32,
}
impl Event for Withdrawal {
    const KEY: &'static str = "nutrition.withdrawal";
}
pub fn status(tx: &Transaction<'_>, person: PersonId) -> Status {
    tx.get::<Nutrition>()
        .ok()
        .and_then(|n| n.people.get(&person).map(|d| d.status(&n.needs, tx.now())))
        .unwrap_or_default()
}
pub fn appetite(tx: &Transaction<'_>, person: PersonId) -> u64 {
    let s = status(tx, person);
    u64::from(1000 - s.quality) / 12 + u64::from(s.craving) / 10
}
/// Perceived benefits use experienced effects; an unfamiliar food is assessed by satiation.
pub fn preference(
    tx: &Transaction<'_>,
    p: PersonId,
    good: GoodId,
    food: &game_objects::Food,
) -> i64 {
    let Some(n) = tx.get::<Nutrition>().ok().filter(|n| n.enabled) else {
        return food.relief as i64;
    };
    let Some(diet) = n.people.get(&p) else {
        return food.relief as i64;
    };
    let learned = diet.learned.get(&good);
    let mut score = food.relief.min(i64::MAX as u64) as i64;
    if let Some(learned) = learned {
        let hedonism = game_vitality::disposition(tx, p, "hedonism");
        let foresight = tx
            .get::<game_traits::Traits>()
            .map_or(1000, |_| game_vitality::ability(tx, p, "foresight"))
            .min(16000) as i64;

        for (key, need) in &n.needs {
            let balance = diet
                .reserves
                .get(key)
                .copied()
                .unwrap_or(0)
                .saturating_sub(diet.used(*need, tx.now()));
            let short = diet.target(*need, 14_000).saturating_sub(balance);
            score = score.saturating_add(
                (short / 1000)
                    .min(*learned.nutrients.get(key).unwrap_or(&0))
                    .min(100) as i64,
            );
        }
        score = score
            .saturating_add(i64::from(learned.effects.mood) * hedonism / 100)
            .saturating_add(i64::from(learned.effects.health) * (1000 + foresight) / 1000);
    }
    if let Some(habit) = learned
        .and_then(|l| l.substance.as_ref())
        .and_then(|s| diet.habits.get(s))
    {
        score = score.saturating_add(i64::from(habit.craving(tx.now())) / 5);
    }
    score
}
/// Prepare shared dietary balances once for a read-only batch of alternatives.
pub fn food_evaluation<'a>(tx: &'a Transaction<'_>, person: PersonId) -> FoodEvaluation<'a> {
    let personal = tx
        .get::<Nutrition>()
        .ok()
        .filter(|n| n.enabled)
        .and_then(|n| n.people.get(&person).map(|d| (d, &n.needs)));
    let hedonism = game_vitality::disposition(tx, person, "hedonism");
    let foresight = tx
        .get::<game_traits::Traits>()
        .map_or(1000, |_| game_vitality::ability(tx, person, "foresight"))
        .min(16000) as i64;
    FoodEvaluation::new(personal, tx.now(), hedonism, foresight)
}
pub fn choose_meal(
    tx: &Transaction<'_>,
    person: PersonId,
    goods: &Table<GoodId, u64>,
) -> Option<(GoodId, game_objects::Food)> {
    let objects = tx.get::<game_objects::Objects>().ok()?;
    if !tx.get::<Nutrition>().is_ok_and(|n| n.enabled) {
        return objects.meal(goods);
    }
    goods
        .iter()
        .filter_map(|(good, q)| {
            let food = objects.food_rules().get(good)?;
            (*q >= food.portion).then(|| {
                (
                    preference(tx, person, *good, food),
                    std::cmp::Reverse(*good),
                    food,
                )
            })
        })
        .max_by_key(|(score, id, _)| (*score, *id))
        .map(|(_, id, food)| (id.0, food.clone()))
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("nutrition", 1)?;
    b.resource::<Nutrition>()?;
    batch::register(b)?;
    b.request::<Enable>(|tx, _| {
        if !tx.get::<Nutrition>()?.enabled {
            tx.edit::<Nutrition>()?.enabled = true;
            tx.schedule(tx.now().after(DAY)?, 35, Review)?;
        }
        Ok(())
    })?;
    b.request::<Register>(|tx, r| {
        if !tx.get::<Nutrition>()?.enabled || tx.get::<Nutrition>()?.people.contains_key(&r.person)
        {
            return Ok(());
        }
        let mut d = Diet::new(&tx.get::<Nutrition>()?.needs, tx.now());
        if let Some((traits, life)) = tx.get::<game_traits::Traits>().ok().zip(
            tx.get::<game_vitality::Vitality>()
                .ok()
                .and_then(|v| v.people.get(&r.person)),
        ) {
            d.requirement_per_mille = game_metabolism::demand(traits, life, r.person, tx.now());
            for reserve in d.reserves.values_mut() {
                *reserve = (u128::from(*reserve) * u128::from(d.requirement_per_mille) / 1000)
                    .min(u128::from(u64::MAX)) as u64;
            }
        }
        tx.edit::<Nutrition>()?.people.insert(r.person, d);
        Ok(())
    })?;
    b.request::<SetDemand>(|tx, r| {
        require(r.per_mille > 0, "invalid dietary demand")?;
        let n = tx.get::<Nutrition>()?;
        let Some(d) = n
            .people
            .get(&r.person)
            .filter(|d| n.enabled && d.requirement_per_mille != r.per_mille)
        else {
            return Ok(());
        };
        let reserves = d.reserves_at(&n.needs, tx.now());
        let now = tx.now();
        let d = tx.edit::<Nutrition>()?.people.get_mut(&r.person).unwrap();
        d.reserves = reserves;
        d.at = now;
        d.requirement_per_mille = r.per_mille;
        Ok(())
    })?;
    b.request::<Deactivate>(|tx, r| {
        if let Some(d) = tx.edit::<Nutrition>()?.people.get_mut(&r.person) {
            d.alive = false;
        }
        Ok(())
    })?;
    b.request::<Meal>(|tx, r| {
        if !tx.get::<Nutrition>()?.enabled {
            return Ok(Effects::default());
        }
        let food = tx
            .get::<game_objects::Objects>()?
            .by_id(r.good)?
            .food
            .clone()
            .ok_or_else(|| Error::Rejected("not edible".into()))?;
        require(r.quantity > 0, "empty meal")?;
        tx.call(Register { person: r.person })?;
        let now = tx.now();
        let needs = tx.get::<Nutrition>()?.needs.clone();
        let d = tx.edit::<Nutrition>()?.people.get_mut(&r.person).unwrap();
        let effects = d.consume(&needs, r.good, &food, r.quantity, now);
        Ok(effects)
    })?;
    b.request::<Review>(|tx, _| {
        let people: Vec<_> = tx
            .get::<Nutrition>()?
            .people
            .iter()
            .filter(|(_, d)| d.alive)
            .map(|(p, _)| *p)
            .collect();
        for person in people {
            let s = status(tx, person);
            if s.craving > 0 {
                tx.call(game_vitality::Experience {
                    person,
                    stress: (s.craving / 20).max(1) as i32,
                    satisfaction: -((s.craving / 30) as i32),
                })?;
                tx.emit(
                    Withdrawal {
                        person,
                        fatigue: u64::from(s.craving) / 100,
                        distress: s.craving,
                    },
                    vec![person],
                )?;
            }
        }
        tx.schedule(tx.now().after(DAY)?, 35, Review)?;
        Ok(())
    })?;
    Ok(())
}
