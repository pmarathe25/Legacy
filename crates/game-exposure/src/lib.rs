//! Finite thermal inertia, weather, clothing, shelter, and actual bodily harm.
use game_core::*;
use game_sim::*;
use game_storage::Table;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thermal {
    pub at: SimTime,
    pub core: i32,
    pub equivalent: i32,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exposure {
    pub enabled: bool,
    pub people: Table<PersonId, Thermal>,
}
impl Resource for Exposure {
    const KEY: &'static str = "exposure.thermal";
    fn validate(&self) -> Result<()> {
        require(
            self.people
                .values()
                .all(|p| (25000..=50000).contains(&p.core)),
            "invalid core temperature",
        )
    }
    fn validate_changes(&self, old: &Self) -> Result<()> {
        require(
            self.people
                .changed(&old.people)
                .all(|(_, p)| (25000..=50000).contains(&p.core)),
            "invalid core temperature",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "exposure.enable";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Tick;
impl Request for Tick {
    const KEY: &'static str = "exposure.tick";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Changed {
    pub person: PersonId,
    pub core: i32,
    pub equivalent: i32,
    pub damage: u64,
}
impl Event for Changed {
    const KEY: &'static str = "exposure.changed";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("exposure", 1)?;
    b.resource::<Exposure>()?;
    b.request::<game_physiology::MetabolicEnvironment>(|tx, r| {
        let Some(thermal) = tx.get::<Exposure>()?.people.get(&r.person) else {
            return Ok(Default::default());
        };
        let cold = (12 - thermal.equivalent).clamp(0, 100) as u64;
        let heat = (thermal.equivalent - 30).clamp(0, 100) as u64;
        let mut rates = game_physiology::Rates::default();
        rates.energy += cold * 20;
        rates.fatigue += (cold + heat) * 15;
        rates.recovery = 1000u64.saturating_sub((cold + heat) * 15).max(250);
        Ok(rates)
    })?;
    b.request::<Enable>(|tx, _| {
        if !tx.get::<Exposure>()?.enabled {
            tx.edit::<Exposure>()?.enabled = true;
            tx.schedule(tx.now().after(SimDuration(6 * HOUR.0))?, 2, Tick)?;
        }
        Ok(())
    })?;
    b.request::<Tick>(|tx, _| {
        let now = tx.now();
        let people: Vec<_> = tx
            .get::<game_physiology::Bodies>()?
            .people
            .iter()
            .filter(|(_, p)| p.alive && p.health > 0)
            .map(|(id, _)| *id)
            .collect();
        for person in people {
            if game_physiology::capable(tx, person).is_err() {
                continue;
            }
            let Some(area) = game_transport::surrounding(tx.get()?, person, now) else {
                continue;
            };
            let warmth = game_environment::at(tx, area)?.warmth;
            let protection =
                game_equipment::protection(tx, person) + game_shelter::protection(tx, area);
            let resistance = (game_vitality::ability(tx, person, "hardiness") / 200
                + game_vitality::ability(tx, person, "size") / 1000)
                .min(100) as i32;
            let dressed = warmth + protection;
            let equivalent = if dressed < 5 {
                (dressed + resistance).min(5)
            } else if dressed > 30 {
                (dressed - resistance).max(30)
            } else {
                dressed
            };
            let target = if equivalent < 5 {
                37000 - (5 - equivalent).min(100) * 120
            } else if equivalent > 30 {
                37000 + (equivalent - 30).min(100) * 120
            } else {
                37000
            };
            let previous = tx
                .get::<Exposure>()?
                .people
                .get(&person)
                .copied()
                .unwrap_or(Thermal {
                    at: SimTime(now.0.saturating_sub(6 * HOUR.0)),
                    core: 37000,
                    equivalent,
                });
            let elapsed = now.0.saturating_sub(previous.at.0);
            let change = i128::from(target - previous.core) * i128::from(elapsed)
                / i128::from(elapsed.saturating_add(6 * HOUR.0).max(1));
            let core =
                previous.core + (change.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32);
            tx.edit::<Exposure>()?.people.insert(
                person,
                Thermal {
                    at: now,
                    core,
                    equivalent,
                },
            );
            let cold = (35000 - core).max(0) as u64;
            let hot = (core - 39000).max(0) as u64;
            let damage = (cold + hot).div_ceil(200).min(30);
            let discomfort = ((36500 - core).max(0) + (core - 37500).max(0)) as u64;
            let strain = damage.max(discomfort.div_ceil(300).min(20));
            if strain > 0 {
                tx.call(game_physiology::Strain {
                    person,
                    amount: strain,
                })?;
                tx.call(game_vitality::Experience {
                    person,
                    stress: strain.min(50) as i32,
                    satisfaction: -(strain.min(50) as i32),
                })?;
                if damage > 0 {
                    tx.call(game_physiology::Damage {
                        person,
                        amount: damage,
                    })?;
                }
            }
            if damage > 0 || previous.core / 500 != core / 500 {
                tx.emit(
                    Changed {
                        person,
                        core,
                        equivalent,
                        damage,
                    },
                    vec![person],
                )?;
            }
        }
        tx.schedule(now.after(SimDuration(6 * HOUR.0))?, 2, Tick)?;
        Ok(())
    })?;
    Ok(())
}
