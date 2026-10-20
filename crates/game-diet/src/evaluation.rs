use super::*;
use std::cell::OnceCell;
struct Need<'a> {
    key: &'a str,
    need: u64,
    shortage: u64,
    benefit: u64,
}
struct Prepared<'a> {
    diet: &'a Diet,
    needs: &'a BTreeMap<String, u64>,
    rows: Vec<Need<'a>>,
    now: SimTime,
    hedonism: i64,
    foresight: i64,
    appetite: OnceCell<u64>,
}
/// Read-only evaluation of one person's current learned dietary consequences.
pub struct Evaluation<'a>(Option<Prepared<'a>>);
impl<'a> Evaluation<'a> {
    pub fn new(
        personal: Option<(&'a Diet, &'a BTreeMap<String, u64>)>,
        now: SimTime,
        hedonism: i64,
        foresight: i64,
    ) -> Self {
        Self(personal.map(|(diet, needs)| {
            Prepared {
                diet,
                needs,
                now,
                hedonism,
                foresight,
                appetite: OnceCell::new(),
                rows: needs
                    .iter()
                    .map(|(key, need)| {
                        let balance = diet
                            .reserves
                            .get(key)
                            .copied()
                            .unwrap_or(0)
                            .saturating_sub(diet.used(*need, now));
                        Need {
                            key,
                            need: *need,
                            shortage: 1000 - (balance / diet.target(*need, 14).max(1)).min(1000),
                            benefit: diet.target(*need, 14_000).saturating_sub(balance) / 1000,
                        }
                    })
                    .collect(),
            }
        }))
    }
    pub fn appetite_for(&self, good: GoodId) -> u64 {
        let Some(p) = &self.0 else { return 0 };
        let Some(learned) = p.diet.learned.get(&good) else {
            return 0;
        };
        let deficiency = p
            .rows
            .iter()
            .map(|n| {
                n.shortage
                    * learned
                        .nutrients
                        .get(n.key)
                        .copied()
                        .unwrap_or(0)
                        .min(n.need)
                    / n.need
            })
            .max()
            .unwrap_or(0);
        let craving = learned
            .substance
            .as_ref()
            .and_then(|s| p.diet.habits.get(s))
            .map_or(0, |h| u64::from(h.craving(p.now)) / 10);
        deficiency / 12 + craving
    }
    pub fn preference(&self, good: GoodId, food: &game_objects::Food) -> i64 {
        let Some(p) = &self.0 else {
            return food.relief as i64;
        };
        let learned = p.diet.learned.get(&good);
        let mut score = food.relief.min(i64::MAX as u64) as i64;
        if let Some(learned) = learned {
            for n in &p.rows {
                score = score.saturating_add(
                    n.benefit
                        .min(*learned.nutrients.get(n.key).unwrap_or(&0))
                        .min(100) as i64,
                );
            }
            score = score
                .saturating_add(i64::from(learned.effects.mood) * p.hedonism / 100)
                .saturating_add(i64::from(learned.effects.health) * (1000 + p.foresight) / 1000);
        }
        if let Some(habit) = learned
            .and_then(|l| l.substance.as_ref())
            .and_then(|s| p.diet.habits.get(s))
        {
            score = score.saturating_add(i64::from(habit.craving(p.now)) / 5);
        }
        score
    }
    pub fn novelty(&self, good: GoodId) -> i64 {
        let Some(p) = &self.0 else { return 0 };
        if p.diet.learned.contains_key(&good) {
            return 0;
        }
        (*p.appetite.get_or_init(|| {
            let s = p.diet.status(p.needs, p.now);
            u64::from(1000 - s.quality) / 12 + u64::from(s.craving) / 10
        }))
        .min(80) as i64
    }
}
