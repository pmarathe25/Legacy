use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    pub level: u32,
    pub tolerance: u32,
    pub last: SimTime,
    pub duration: SimDuration,
}
impl Habit {
    pub fn craving(&self, at: SimTime) -> u32 {
        if at.0.saturating_sub(self.last.0) <= self.duration.0 {
            return 0;
        }
        self.level
            .saturating_sub((at.0.saturating_sub(self.last.0) / DAY.0).min(100) as u32 * 10)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Learned {
    pub nutrients: BTreeMap<String, u64>,
    pub effects: Effects,
    pub substance: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diet {
    pub at: SimTime,
    pub reserves: BTreeMap<String, u64>,
    pub intake: BTreeMap<String, u64>,
    pub habits: BTreeMap<String, Habit>,
    pub familiar: BTreeMap<GoodId, u64>,
    #[serde(default)]
    pub learned: BTreeMap<GoodId, Learned>,
    pub meals: u64,
    pub alive: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub quality: u32,
    pub craving: u32,
}
impl Default for Status {
    fn default() -> Self {
        Self {
            quality: 1000,
            craving: 0,
        }
    }
}
impl Diet {
    pub fn new(needs: &BTreeMap<String, u64>, at: SimTime) -> Self {
        Self {
            at,
            reserves: needs.iter().map(|(k, n)| (k.clone(), n * 30_000)).collect(),
            intake: BTreeMap::new(),
            habits: BTreeMap::new(),
            familiar: BTreeMap::new(),
            learned: BTreeMap::new(),
            meals: 0,
            alive: true,
        }
    }
    pub fn reserves_at(&self, needs: &BTreeMap<String, u64>, at: SimTime) -> BTreeMap<String, u64> {
        needs
            .iter()
            .map(|(k, n)| {
                let used = (u128::from(at.0.saturating_sub(self.at.0)) * u128::from(*n) * 1000
                    / u128::from(DAY.0))
                .min(u128::from(u64::MAX)) as u64;
                (
                    k.clone(),
                    self.reserves
                        .get(k)
                        .copied()
                        .unwrap_or(0)
                        .saturating_sub(used),
                )
            })
            .collect()
    }
    pub fn status(&self, needs: &BTreeMap<String, u64>, at: SimTime) -> Status {
        let quality = needs
            .iter()
            .map(|(k, n)| {
                let used = (u128::from(at.0.saturating_sub(self.at.0)) * u128::from(*n) * 1000
                    / u128::from(DAY.0))
                .min(u128::from(u64::MAX)) as u64;
                (self
                    .reserves
                    .get(k)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(used)
                    / (n * 14))
                    .min(1000) as u32
            })
            .min()
            .unwrap_or(1000);
        Status {
            quality,
            craving: self
                .habits
                .values()
                .map(|h| h.craving(at))
                .max()
                .unwrap_or(0),
        }
    }
    pub fn consume(
        &mut self,
        needs: &BTreeMap<String, u64>,
        good: GoodId,
        food: &game_objects::Food,
        q: u64,
        at: SimTime,
    ) -> Effects {
        self.reserves = self.reserves_at(needs, at);
        self.at = at;
        for (key, amount) in &food.nutrients {
            let dose = (u128::from(*amount) * u128::from(q) * 1000 / u128::from(food.portion))
                .min(u128::from(u64::MAX)) as u64;
            *self.intake.entry(key.clone()).or_default() = self
                .intake
                .get(key)
                .copied()
                .unwrap_or(0)
                .saturating_add(dose);
            let reserve = self.reserves.entry(key.clone()).or_default();
            *reserve = reserve.saturating_add(dose).min(
                needs
                    .get(key)
                    .copied()
                    .unwrap_or(1000)
                    .saturating_mul(60_000),
            );
        }
        let scale = |n: i32| {
            (i128::from(n) * i128::from(q) / i128::from(food.portion))
                .clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
        };
        let mut effects = Effects {
            health: scale(food.effects.health),
            mood: scale(food.effects.mood),
            alertness: scale(food.effects.alertness),
        };
        if let Some(substance) = &food.effects.substance {
            let h = self.habits.entry(substance.clone()).or_insert(Habit {
                level: 0,
                tolerance: 0,
                last: at,
                duration: food.effects.duration,
            });
            let days = (at.0.saturating_sub(h.last.0) / DAY.0).min(1000) as u32;
            h.level = h.level.saturating_sub(days * 10);
            h.tolerance = h.tolerance.saturating_sub(days * 5);
            effects.mood = (i64::from(effects.mood) * 1000 / (1000 + i64::from(h.tolerance)))
                .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            effects.alertness = (i64::from(effects.alertness) * 1000
                / (1000 + i64::from(h.tolerance)))
            .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
            let dose = (u128::from(food.effects.dependence) * u128::from(q)
                / u128::from(food.portion))
            .min(1000) as u32;
            h.level = h.level.saturating_add(dose).min(1000);
            h.tolerance = h.tolerance.saturating_add(dose / 2).min(1000);
            h.last = at;
            h.duration = food.effects.duration;
        }
        // A personal estimate from this meal; small differences are not perfectly discerned.
        self.learned.insert(
            good,
            Learned {
                nutrients: food
                    .nutrients
                    .iter()
                    .map(|(k, n)| (k.clone(), n / 5 * 5))
                    .collect(),
                effects: Effects {
                    health: (i128::from(effects.health) * i128::from(food.portion)
                        / i128::from(q.max(1))) as i32,
                    mood: (i128::from(effects.mood) * i128::from(food.portion)
                        / i128::from(q.max(1))) as i32,
                    alertness: (i128::from(effects.alertness) * i128::from(food.portion)
                        / i128::from(q.max(1))) as i32,
                },
                substance: food.effects.substance.clone(),
            },
        );
        self.meals = self.meals.saturating_add(1);
        *self.familiar.entry(good).or_default() += 1;
        effects
    }
}
