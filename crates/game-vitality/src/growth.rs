use super::*;
const YEAR: u64 = 365 * DAY.0;
/// Accumulated childhood conditions are acquired, never inherited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upbringing {
    pub since: SimTime,
    pub deprivation: u64,
    pub distress: u64,
}
fn curve(age: u64, points: &[(u64, u64)]) -> u64 {
    for pair in points.windows(2) {
        let [(start, a), (end, b)] = [pair[0], pair[1]];
        if age < end * YEAR {
            let elapsed = age.saturating_sub(start * YEAR);
            return (i128::from(a)
                + (i128::from(b) - i128::from(a)) * i128::from(elapsed)
                    / i128::from((end - start) * YEAR)) as u64;
        }
    }
    points.last().unwrap().1
}
impl Upbringing {
    pub(super) fn observe(
        &mut self,
        born: SimTime,
        at: SimTime,
        physical: u32,
        stress: u32,
        updated: SimTime,
    ) {
        let end = at.0.min(born.0.saturating_add(18 * YEAR));
        let start = self.since.0.max(born.0).min(end);
        let duration = end - start;
        if duration == 0 {
            self.since = at;
            return;
        }
        self.deprivation = self
            .deprivation
            .saturating_add(duration.saturating_mul(u64::from(1000 - physical)));
        // Integrate the same daily recovery as condition(), independent of review frequency.
        let integral = |time: u64| {
            let elapsed = time.saturating_sub(updated.0);
            let n = (elapsed / DAY.0).min(u64::from(stress).div_ceil(3));
            let whole = u128::from(DAY.0)
                * (u128::from(n) * u128::from(stress)
                    - 3 * u128::from(n) * u128::from(n.saturating_sub(1)) / 2);
            whole
                + u128::from(elapsed % DAY.0) * u128::from(u64::from(stress).saturating_sub(3 * n))
        };
        self.distress = self
            .distress
            .saturating_add((integral(end) - integral(start)) as u64);
        self.since = at;
    }
    pub fn deprivation(&self) -> u64 {
        (self.deprivation / (18 * YEAR)).min(1000)
    }
    pub fn distress(&self) -> u64 {
        (self.distress / (18 * YEAR)).min(1000)
    }
}
/// Permille of adult expression, with independent physical and cognitive trajectories.
pub fn maturation(born: SimTime, at: SimTime, key: &str) -> u64 {
    let age = at.0.saturating_sub(born.0);
    match key {
        "size" | "constitution" => curve(age, &[(0, 220), (2, 400), (12, 700), (20, 1000)]),
        "strength" | "endurance" | "robustness" | "hardiness" => {
            curve(age, &[(0, 80), (2, 200), (12, 550), (20, 1000)])
        }
        "speed" | "agility" => curve(age, &[(0, 0), (1, 200), (6, 650), (16, 1000)]),
        "vocalization" | "gesturing" => {
            curve(age, &[(0, 200), (1, 320), (2, 520), (6, 840), (12, 1000)])
        }
        "interpretation" => curve(age, &[(0, 100), (1, 235), (2, 460), (6, 820), (12, 1000)]),
        "hearing" => curve(age, &[(0, 298), (2, 460), (12, 730), (20, 1000)]),
        "language" => curve(age, &[(0, 0), (1, 150), (2, 400), (6, 800), (12, 1000)]),
        "intelligence" => curve(age, &[(0, 80), (2, 220), (6, 450), (14, 800), (20, 1000)]),
        "foresight" | "resilience" | "attention" => {
            curve(age, &[(0, 50), (6, 250), (14, 650), (24, 1000)])
        }
        "learning" => curve(age, &[(0, 500), (3, 1100), (8, 1200), (20, 1000)]),
        _ => 1000,
    }
}
impl Life {
    pub fn upbringing_at(&self, at: SimTime) -> Upbringing {
        if self.span.is_none() {
            return self.upbringing;
        }
        let mut result = self.upbringing;
        result.observe(self.born, at, self.physical, self.stress, self.updated);
        result
    }
    pub fn maturity(&self, at: SimTime, key: &str) -> u64 {
        if self.span.is_none() {
            1000
        } else {
            maturation(self.born, at, key)
        }
    }
    pub fn disposition(&self, raw: i64, key: &str, at: SimTime) -> i64 {
        if !matches!(
            key,
            "patience" | "risk-tolerance" | "sociability" | "empathy" | "compassion" | "rigidity"
        ) {
            return raw;
        }
        let regulation = self.maturity(at, "resilience") as i64;
        let adversity = self.upbringing_at(at).distress() as i64;
        match key {
            "patience" => (raw * (500 + regulation / 2) / 1000 - adversity / 5).clamp(0, 1000),
            "risk-tolerance" => (raw + (1000 - regulation) / 5 - adversity / 4).clamp(0, 1000),
            "sociability" => (raw - adversity / 5).clamp(0, 1000),
            "rigidity" => (raw * (500 + regulation / 2) / 1000
                - i64::from(self.condition(at).0) / 4
                - adversity / 10)
                .clamp(0, 1000),
            "empathy" | "compassion" => {
                (raw * (500 + regulation / 2) / 1000 - adversity / 10).clamp(0, 1000)
            }
            _ => raw,
        }
    }
    pub(super) fn grow(&mut self, at: SimTime) {
        self.upbringing = self.upbringing_at(at);
    }
}
pub fn disposition(tx: &Transaction<'_>, person: PersonId, key: &str) -> i64 {
    let raw = tx
        .get::<game_traits::Traits>()
        .map_or(500, |t| t.disposition(person, key));
    tx.get::<Vitality>()
        .ok()
        .and_then(|v| v.people.get(&person))
        .map_or(raw, |life| life.disposition(raw, key, tx.now()))
}
/// Physical locomotion readiness; a dependent may still be carried by another person.
pub fn can_walk(tx: &Transaction<'_>, person: PersonId) -> bool {
    let maturity = tx
        .get::<Vitality>()
        .ok()
        .and_then(|v| v.people.get(&person))
        .map_or(1000, |life| life.maturity(tx.now(), "speed"));
    maturity.saturating_mul(factor(ability(tx, person, "speed"))) / 1000 >= 200
}

pub fn can_converse(tx: &Transaction<'_>, person: PersonId) -> bool {
    tx.get::<Vitality>()
        .ok()
        .and_then(|v| v.people.get(&person))
        .is_none_or(|life| {
            life.maturity(tx.now(), "language") * factor(ability(tx, person, "intelligence")) / 1000
                >= 400
        })
}

/// Coordinated deliberate manipulation, rather than reflexive infant movement.
/// Uses expressed motor development, so inherited potential and impairment matter.
pub fn can_manipulate(tx: &Transaction<'_>, person: PersonId) -> bool {
    let maturity = tx
        .get::<Vitality>()
        .ok()
        .and_then(|v| v.people.get(&person))
        .map_or(1000, |life| life.maturity(tx.now(), "agility"));
    maturity.saturating_mul(factor(ability(tx, person, "agility"))) / 1000 >= 200
}

/// Attention supports a current commitment; important needs can still interrupt it.
pub fn switching_margin(tx: &Transaction<'_>, person: PersonId, base: i64) -> i64 {
    base.saturating_mul(250 + ability(tx, person, "attention").min(32000) as i64) / 1250
}

/// A personal horizon for reconsideration, shortened by the stakes of current needs.
/// External evidence remains free to interrupt; this does not exclude any action.
pub fn attention_horizon(tx: &Transaction<'_>, p: PersonId, pressure: u64) -> u64 {
    let attention = tx
        .get::<Vitality>()
        .ok()
        .filter(|v| v.enabled)
        .map_or(0, |_| ability(tx, p, "attention").min(16000));
    1 + attention / 250 / pressure.max(1)
}
