//! Lazy common subexpressions for one exact life value and clock.
use super::*;

pub struct Projection {
    life: Life,
    at: SimTime,
    upbringing: Option<Upbringing>,
    condition: Option<(u32, i32)>,
    aging: [Option<u64>; 2],
    maturity: [Option<u64>; 11],
}
impl Projection {
    pub fn new(life: Life, at: SimTime) -> Self {
        Self {
            life,
            at,
            upbringing: None,
            condition: None,
            aging: [None; 2],
            maturity: [None; 11],
        }
    }
    fn upbringing(&mut self) -> Upbringing {
        *self
            .upbringing
            .get_or_insert_with(|| self.life.upbringing_at(self.at))
    }
    fn condition(&mut self) -> (u32, i32) {
        *self
            .condition
            .get_or_insert_with(|| self.life.condition(self.at))
    }
    fn aging(&mut self, mental: bool) -> u64 {
        *self.aging[usize::from(mental)].get_or_insert_with(|| self.life.aging(self.at, mental))
    }
    pub fn maturity(&mut self, key: &str) -> u64 {
        let slot = match key {
            "size" | "constitution" => 0,
            "strength" | "endurance" | "robustness" | "hardiness" => 1,
            "speed" | "agility" => 2,
            "vocalization" | "gesturing" => 3,
            "interpretation" => 4,
            "hearing" => 5,
            "language" => 6,
            "intelligence" => 7,
            "foresight" | "resilience" | "attention" => 8,
            "learning" => 9,
            _ => 10,
        };
        *self.maturity[slot].get_or_insert_with(|| self.life.maturity(self.at, key))
    }
    pub fn ability(&mut self, raw: u64, key: &str) -> u64 {
        let Some(slot) = ability_slot(key) else {
            return raw;
        };
        let mental = slot < 7;
        let condition = if mental {
            1000 - u64::from(self.condition().0) / 3
        } else {
            500 + u64::from(self.life.physical) / 2
        };
        let development = self.maturity(key);
        let fulfillment = 1000 - self.upbringing().deprivation() / 4;
        scaled(
            scaled(
                scaled(scaled(raw, development, 1000), fulfillment, 1000),
                self.aging(mental),
                1000,
            ),
            condition,
            1000,
        )
    }
    pub fn disposition(&mut self, raw: i64, key: &str) -> i64 {
        if disposition_slot(key).is_none() {
            return raw;
        }
        let regulation = self.maturity("resilience") as i64;
        let adversity = self.upbringing().distress() as i64;
        match key {
            "patience" => (raw * (500 + regulation / 2) / 1000 - adversity / 5).clamp(0, 1000),
            "risk-tolerance" => (raw + (1000 - regulation) / 5 - adversity / 4).clamp(0, 1000),
            "sociability" => (raw - adversity / 5).clamp(0, 1000),
            "rigidity" => (raw * (500 + regulation / 2) / 1000
                - i64::from(self.condition().0) / 4
                - adversity / 10)
                .clamp(0, 1000),
            "empathy" | "compassion" => {
                (raw * (500 + regulation / 2) / 1000 - adversity / 10).clamp(0, 1000)
            }
            _ => raw,
        }
    }
}
