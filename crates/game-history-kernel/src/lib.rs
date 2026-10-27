//! Experimental multirate history. Ordinary commitments, not per-action callback dispatch.
mod planning;
pub use game_history_economy::{Economy, Settlement};
pub use game_history_population::{Activity, DAY, FATIGUE, HUNGER, Person, Population};
pub use game_history_society::{SocialPerson, Society};
use rayon::ThreadPool;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    pub sequence: u64,
    pub at: u64,
    pub kind: String,
    pub people: Vec<u64>,
    pub area: u64,
    pub amount: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Pregnancy {
    mother: u64,
    father: u64,
    due: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    version: u32,
    pub now: u64,
    pub seed: u64,
    population: Population,
    economy: Economy,
    society: Society,
    pregnancies: Vec<Pregnancy>,
    next_person: u64,
    sequence: u64,
    pub totals: BTreeMap<String, u64>,
}
pub fn random(seed: u64, id: u64, time: u64) -> u64 {
    let mut z = seed ^ id.wrapping_mul(0x9e3779b97f4a7c15) ^ time.wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
impl World {
    pub fn population(&self) -> &Population {
        &self.population
    }
    pub fn economy(&self) -> &Economy {
        &self.economy
    }
    pub fn society(&self) -> &Society {
        &self.society
    }
    pub fn new(
        seed: u64,
        now: u64,
        population: Population,
        economy: Economy,
        society: Society,
    ) -> Result<Self, String> {
        let next_person = population
            .people()
            .last()
            .map_or(1, |p| p.id.saturating_add(1));
        let w = Self {
            version: 1,
            now,
            seed,
            population,
            economy,
            society,
            pregnancies: vec![],
            next_person,
            sequence: 0,
            totals: BTreeMap::new(),
        };
        w.validate()?;
        Ok(w)
    }
    pub fn fixture(seed: u64, people: usize, settlements: usize) -> Result<Self, String> {
        if people == 0 || settlements == 0 || settlements > people || people > 1_000_000 {
            return Err("invalid settlement population".into());
        }
        let now = 30 * 365 * DAY;
        let rows = (0..people)
            .map(|i| {
                let id = i as u64 + 1;
                let n = random(seed, id, 0);
                Person {
                    id,
                    name: format!("Person {id}"),
                    born: now - (18 + n % 12) * 365 * DAY,
                    dies_at: now + (45 + n % 25) * 365 * DAY,
                    parents: vec![],
                    genome: [
                        (n % 2) as u16,
                        (n % 1000) as u16,
                        ((n >> 16) % 1000) as u16,
                        ((n >> 32) % 1000) as u16,
                    ],
                    area: (i % settlements) as u64 + 1,
                    alive: true,
                    controlled: false,
                    dependent: false,
                    hunger: 120 + n % 240,
                    fatigue: n % 180,
                    health: 6000,
                    activity: Activity::Idle,
                    work_minutes: 0,
                    accounted_work: 0,
                    social_minutes: 0,
                }
            })
            .collect::<Vec<_>>();
        let social = rows
            .iter()
            .map(|p| SocialPerson {
                id: p.id,
                ideas: [p.area].into_iter().collect(),
                ties: BTreeMap::new(),
            })
            .collect();
        let areas = (0..settlements)
            .map(|i| Settlement {
                id: i as u64 + 1,
                name: format!("Settlement {}", i + 1),
                food: (people / settlements * 10 + 10) as u64,
                capacity: (people * 100 + 100) as u64,
                produced: 0,
                consumed: 0,
                spoiled: 0,
            })
            .collect();
        Self::new(
            seed,
            now,
            Population::new(rows)?,
            Economy::new(areas)?,
            Society::new(social)?,
        )
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self
                .population
                .people()
                .last()
                .is_some_and(|p| p.id >= self.next_person)
        {
            return Err("invalid kernel version or identity counter".into());
        }
        self.population.validate()?;
        if self.population.people().iter().any(|p| {
            self.economy.get(p.area).is_none()
                || p.born > self.now
                || p.alive && p.dies_at <= self.now
                || p.activity.until().is_some_and(|at| at <= self.now)
        }) {
            return Err("invalid individual area or deadline".into());
        }
        if self.population.people().iter().map(|p| p.id).ne(self
            .society
            .people()
            .iter()
            .map(|p| p.id))
        {
            return Err("social identity set differs".into());
        }
        if self
            .economy
            .settlements()
            .iter()
            .any(|s| s.food > s.capacity)
        {
            return Err("inventory exceeds capacity".into());
        }
        if self.pregnancies.iter().any(|p| {
            p.due <= self.now
                || self.population.get(p.mother).is_none()
                || self.population.get(p.father).is_none()
        }) {
            return Err("invalid pregnancy".into());
        }
        Ok(())
    }
    /// Boundary selection is independent of caller's output/checkpoint chunk sizes.
    pub fn next_boundary(&self) -> Result<u64, String> {
        let ordinary = (self.now / 720 + 1)
            .checked_mul(720)
            .ok_or("time overflow")?;
        Ok(ordinary
            .min(self.population.deadline(self.now))
            .max(self.now.checked_add(1).ok_or("time overflow")?))
    }
    /// Advance one canonical boundary. Every fallible phase precedes the final owner installation.
    pub fn step(&mut self, pool: &ThreadPool) -> Result<Vec<Receipt>, String> {
        let at = self.next_boundary()?;
        let (mut population, completed) = self.population.advance(pool, self.now, at);
        let mut economy = self.economy.clone();
        let mut society = None;
        let mut pregnancies = self.pregnancies.clone();
        let mut next_person = self.next_person;
        let mut events = Vec::<(String, Vec<u64>, u64, u64)>::new();
        let work = population.production();
        for (area, amount) in economy.harvest(&work, pool)? {
            if amount > 0 {
                events.push(("harvest".into(), vec![], area, amount));
            }
        }
        let mut pairs = vec![];
        let completed_by_person: BTreeMap<_, _> =
            completed.iter().map(|c| (c.person, &c.activity)).collect();
        for c in &completed {
            match c.activity {
                Activity::Travel { destination, .. } => {
                    events.push(("arrival".into(), vec![c.person], destination, 0))
                }
                Activity::Social { partner, .. }
                    if c.person < partner
                        && completed_by_person.get(&partner).is_some_and(
                            |a| matches!(a,Activity::Social{partner:p,..} if *p==c.person),
                        ) =>
                {
                    pairs.push((c.person, partner))
                }
                Activity::Rest { .. } => events.push(("rested".into(), vec![c.person], c.area, 0)),
                _ => (),
            }
        }
        if !pairs.is_empty() {
            let mut next = self.society.clone();
            next.meet(&pairs, pool)?;
            society = Some(next);
            for &(a, b) in &pairs {
                events.push((
                    "encounter".into(),
                    vec![a, b],
                    population.get(a).unwrap().area,
                    0,
                ));
            }
        }
        for p in population.people() {
            if !p.alive && self.population.get(p.id).is_some_and(|old| old.alive) {
                events.push(("death".into(), vec![p.id], p.area, 0));
            }
        }
        if at % DAY == 0 {
            let due: Vec<_> = pregnancies
                .iter()
                .filter(|p| p.due <= at)
                .cloned()
                .collect();
            pregnancies.retain(|p| p.due > at);
            for birth in due {
                let mother = population.get(birth.mother).unwrap().clone();
                if !mother.alive {
                    continue;
                }
                let father = population.get(birth.father).unwrap();
                let id = next_person;
                next_person = next_person.checked_add(1).ok_or("identity overflow")?;
                let n = random(self.seed, id, at);
                let child = Person {
                    id,
                    name: format!("Person {id}"),
                    born: at,
                    dies_at: at
                        .checked_add((65 + n % 25) * 365 * DAY)
                        .ok_or("lifespan overflow")?,
                    parents: vec![mother.id, father.id],
                    genome: [
                        (n % 2) as u16,
                        mother.genome[1],
                        father.genome[2],
                        ((u32::from(mother.genome[3]) + u32::from(father.genome[3])) / 2) as u16,
                    ],
                    area: mother.area,
                    alive: true,
                    controlled: false,
                    dependent: true,
                    hunger: 0,
                    fatigue: 0,
                    health: 6000,
                    activity: Activity::Idle,
                    work_minutes: 0,
                    accounted_work: 0,
                    social_minutes: 0,
                };
                society
                    .get_or_insert_with(|| self.society.clone())
                    .insert(id, &child.parents)?;
                population.insert(child)?;
                events.push((
                    "birth".into(),
                    vec![id, mother.id, birth.father],
                    mother.area,
                    0,
                ));
            }
            let adults: Vec<_> = population
                .people()
                .iter()
                .filter(|p| {
                    p.alive
                        && !p.controlled
                        && p.hunger < HUNGER
                        && p.fatigue < FATIGUE
                        && at - p.born >= 18 * 365 * DAY
                        && at - p.born < 40 * 365 * DAY
                })
                .collect();
            for mother in adults.iter().filter(|p| p.genome[0] % 2 == 0) {
                if random(self.seed, mother.id, at / DAY) % 900 != 0
                    || pregnancies.iter().any(|p| p.mother == mother.id)
                {
                    continue;
                }
                if let Some(father) = adults.iter().find(|p| {
                    p.area == mother.area
                        && p.genome[0] % 2 == 1
                        && !mother.parents.contains(&p.id)
                        && !p.parents.contains(&mother.id)
                        && !mother.parents.iter().any(|a| p.parents.contains(a))
                }) {
                    pregnancies.push(Pregnancy {
                        mother: mother.id,
                        father: father.id,
                        due: at.checked_add(280 * DAY).ok_or("pregnancy overflow")?,
                    });
                    events.push((
                        "conception".into(),
                        vec![mother.id, father.id],
                        mother.area,
                        0,
                    ));
                }
            }
            for p in population.people().iter().filter(|p| {
                p.alive
                    && !p.dependent
                    && !p.controlled
                    && random(self.seed ^ 17, p.id, at / DAY) % 3650 == 0
            }) {
                let idea = random(self.seed ^ 37, p.id, at);
                society
                    .get_or_insert_with(|| self.society.clone())
                    .innovate(p.id, idea)?;
                events.push(("innovation".into(), vec![p.id], p.area, idea));
            }
        }
        planning::settle(
            &mut population,
            &mut economy,
            pool,
            self.seed,
            at,
            &mut events,
        )?;
        let mut totals = self.totals.clone();
        let mut sequence = self.sequence;
        let mut receipts = Vec::with_capacity(events.len());
        for (kind, people, area, amount) in events {
            sequence = sequence.checked_add(1).ok_or("receipt overflow")?;
            let count = totals.entry(kind.clone()).or_default();
            *count = count.checked_add(1).ok_or("counter overflow")?;
            receipts.push(Receipt {
                sequence,
                at,
                kind,
                people,
                area,
                amount,
            });
        }
        // Validate staged owner state before installation; no gameplay callbacks run here.
        population.validate()?;
        self.population = population;
        self.economy = economy;
        if let Some(society) = society {
            self.society = society;
        }
        self.pregnancies = pregnancies;
        self.next_person = next_person;
        self.sequence = sequence;
        self.totals = totals;
        self.now = at;
        Ok(receipts)
    }
    pub fn control(&mut self, person: u64, controlled: bool) -> Result<(), String> {
        self.population.control(person, controlled)
    }
    pub fn injure(&mut self, person: u64, amount: u64) -> Result<(), String> {
        self.population.injure(person, amount)
    }
    pub fn save(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|e| e.to_string())
    }
    pub fn load(text: &str) -> Result<Self, String> {
        let w: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        w.validate()?;
        Ok(w)
    }
}
