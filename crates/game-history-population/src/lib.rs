//! Individual physical state. Time is integer minutes in this experimental model.
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub const DAY: u64 = 1440;
pub const HUNGER: u64 = 480;
pub const FATIGUE: u64 = 360;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Activity {
    Idle,
    Work { until: u64 },
    Rest { until: u64 },
    Travel { destination: u64, until: u64 },
    Eat { until: u64 },
    Care { patient: u64, until: u64 },
    Social { partner: u64, until: u64 },
}
impl Activity {
    pub fn until(&self) -> Option<u64> {
        match *self {
            Self::Idle => None,
            Self::Work { until }
            | Self::Rest { until }
            | Self::Travel { until, .. }
            | Self::Eat { until }
            | Self::Care { until, .. }
            | Self::Social { until, .. } => Some(until),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: u64,
    pub name: String,
    pub born: u64,
    pub dies_at: u64,
    pub parents: Vec<u64>,
    pub genome: [u16; 4],
    pub area: u64,
    pub alive: bool,
    pub controlled: bool,
    pub dependent: bool,
    pub hunger: u64,
    pub fatigue: u64,
    pub health: u64,
    pub activity: Activity,
    pub work_minutes: u64,
    pub accounted_work: u64,
    pub social_minutes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Population {
    people: Vec<Person>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub person: u64,
    pub area: u64,
    pub activity: Activity,
}
impl Population {
    pub fn new(mut people: Vec<Person>) -> Result<Self, String> {
        people.sort_by_key(|p| p.id);
        let result = Self { people };
        result.validate()?;
        Ok(result)
    }
    pub fn people(&self) -> &[Person] {
        &self.people
    }
    pub fn get(&self, id: u64) -> Option<&Person> {
        self.people
            .binary_search_by_key(&id, |p| p.id)
            .ok()
            .map(|i| &self.people[i])
    }
    pub fn validate(&self) -> Result<(), String> {
        if !self.people.windows(2).all(|p| p[0].id < p[1].id)
            || self.people.iter().any(|p| {
                p.id == 0
                    || p.name.is_empty()
                    || p.born > p.dies_at
                    || p.alive && p.health == 0
                    || p.accounted_work > p.work_minutes
                    || p.parents.iter().any(|id| *id == p.id)
            })
        {
            return Err("invalid individual population".into());
        }
        Ok(())
    }
    /// Double-buffer hot personal rows. No callback or shared write runs on workers.
    pub fn advance(&self, pool: &rayon::ThreadPool, from: u64, to: u64) -> (Self, Vec<Completion>) {
        let elapsed = to - from;
        let rows: Vec<_> = pool.install(|| {
            self.people
                .par_iter()
                .map(|old| {
                    let mut p = old.clone();
                    let mut completion = None;
                    if p.alive {
                        p.hunger = p.hunger.saturating_add(elapsed);
                        let starving = to.saturating_sub(
                            from.max(from.saturating_add(1440u64.saturating_sub(old.hunger))),
                        );
                        p.health = p.health.saturating_sub(starving);
                        if matches!(p.activity, Activity::Rest { .. }) {
                            p.fatigue = p.fatigue.saturating_sub(elapsed.saturating_mul(3));
                        } else {
                            p.fatigue = p.fatigue.saturating_add(elapsed);
                        }
                        if matches!(p.activity, Activity::Work { .. }) {
                            p.work_minutes = p.work_minutes.saturating_add(elapsed);
                        }
                        if matches!(p.activity, Activity::Social { .. }) {
                            p.social_minutes = p.social_minutes.saturating_add(elapsed);
                        }
                        p.dependent = to.saturating_sub(p.born) < 12 * 365 * DAY;
                        if to >= p.dies_at || p.health == 0 {
                            p.alive = false;
                            p.activity = Activity::Idle;
                        } else if p.activity.until().is_some_and(|at| at <= to) {
                            completion = Some(Completion {
                                person: p.id,
                                area: p.area,
                                activity: p.activity.clone(),
                            });
                            if let Activity::Travel { destination, .. } = p.activity {
                                p.area = destination;
                            }
                            p.activity = Activity::Idle;
                        }
                    }
                    (p, completion)
                })
                .collect()
        });
        let (people, completions): (Vec<_>, Vec<_>) = rows.into_iter().unzip();
        (Self { people }, completions.into_iter().flatten().collect())
    }
    pub fn deadline(&self, now: u64) -> u64 {
        self.people
            .iter()
            .filter(|p| p.alive)
            .fold(u64::MAX, |due, p| {
                let mut at = p.activity.until().unwrap_or(u64::MAX).min(p.dies_at);
                if !p.controlled && p.hunger < HUNGER {
                    at = at.min(now.saturating_add(HUNGER - p.hunger));
                }
                if !p.controlled
                    && p.fatigue < FATIGUE
                    && !matches!(p.activity, Activity::Rest { .. })
                {
                    at = at.min(now.saturating_add(FATIGUE - p.fatigue));
                }
                if p.hunger >= 1440 {
                    at = at.min(now.saturating_add(p.health));
                } else {
                    at = at.min(now.saturating_add(1440 - p.hunger));
                }
                due.min(at)
            })
    }
    /// Receiver-owned command, used only after deterministic admission.
    pub fn assign(&mut self, id: u64, activity: Activity) -> Result<(), String> {
        let i = self
            .people
            .binary_search_by_key(&id, |p| p.id)
            .map_err(|_| "unknown person")?;
        let p = &mut self.people[i];
        if !p.alive || p.controlled {
            return Err("person unavailable".into());
        }
        p.activity = activity;
        Ok(())
    }
    pub fn feed(&mut self, id: u64) -> Result<(), String> {
        let i = self
            .people
            .binary_search_by_key(&id, |p| p.id)
            .map_err(|_| "unknown eater")?;
        if !self.people[i].alive {
            return Err("dead eater".into());
        }
        self.people[i].hunger = self.people[i].hunger.saturating_sub(600);
        Ok(())
    }
    pub fn control(&mut self, id: u64, controlled: bool) -> Result<(), String> {
        let i = self
            .people
            .binary_search_by_key(&id, |p| p.id)
            .map_err(|_| "unknown controlled person")?;
        self.people[i].controlled = controlled;
        self.people[i].activity = Activity::Idle;
        Ok(())
    }
    pub fn injure(&mut self, id: u64, amount: u64) -> Result<(), String> {
        let i = self
            .people
            .binary_search_by_key(&id, |p| p.id)
            .map_err(|_| "unknown injured person")?;
        let p = &mut self.people[i];
        if !p.alive {
            return Err("person already dead".into());
        }
        p.health = p.health.saturating_sub(amount);
        p.alive = p.health > 0;
        p.activity = Activity::Idle;
        Ok(())
    }
    pub fn insert(&mut self, person: Person) -> Result<(), String> {
        let i = self
            .people
            .binary_search_by_key(&person.id, |p| p.id)
            .err()
            .ok_or("duplicate birth")?;
        self.people.insert(i, person);
        Ok(())
    }
    /// Work credit survives interruption; completion alone never awards output.
    pub fn production(&mut self) -> Vec<(u64, u64)> {
        self.people
            .iter_mut()
            .filter_map(|p| {
                let units = (p.work_minutes - p.accounted_work) / 30;
                if units == 0 {
                    return None;
                }
                p.accounted_work += units * 30;
                Some((p.area, units))
            })
            .collect()
    }
}
