use game_core::*;
use game_sim::*;
use game_storage::Table;
use game_vitality::ability;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signals {
    pub enabled: bool,
    pub perceptions: Table<PersonId, Table<PersonId, Perception>>,
    #[serde(default)]
    pub expressed: Table<PersonId, (SimTime, u64)>,
}
impl Resource for Signals {
    const KEY: &'static str = "signals.experience";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Enable;
impl Request for Enable {
    const KEY: &'static str = "signals.enable";
    type Response = ();
}
fn active(tx: &Transaction<'_>) -> bool {
    tx.get::<Signals>().is_ok_and(|s| s.enabled)
}
fn local(tx: &Transaction<'_>, a: PersonId, b: PersonId) -> Result<()> {
    game_physiology::capable(tx, a)?;
    game_physiology::capable(tx, b)?;
    require(
        game_transport::together(tx.get()?, a, b),
        "signal is not local",
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Perception {
    pub cue: String,
    pub guessed_need: Option<String>,
    pub estimated_need: u64,
    pub at: SimTime,
    #[serde(default)]
    pub last_fed: Option<SimTime>,
    /// Last food-specific evidence survives an ambiguous or competing cue.
    #[serde(default)]
    pub food: Option<(SimTime, u64)>,
    #[serde(default = "ordinary_appetite")]
    pub appetite: u64,
    #[serde(default)]
    pub area: Option<AreaId>,
}
fn ordinary_appetite() -> u64 {
    1000
}
fn care_skill(tx: &Transaction<'_>, p: PersonId) -> u64 {
    tx.get::<game_skills::Skills>()
        .ok()
        .and_then(|s| s.levels.get(&p)?.get("child-care"))
        .copied()
        .unwrap_or(0)
        .min(4000)
}
/// A subjective forecast, learned from observed hunger and feeding intervals.
pub fn estimated_hunger(tx: &Transaction<'_>, observer: PersonId, person: PersonId) -> Option<u64> {
    let memory = tx
        .get::<Signals>()
        .ok()?
        .perceptions
        .get(&observer)?
        .get(&person)?;
    let (at, need) = memory.food?;
    let rules = &tx.get::<game_physiology::Bodies>().ok()?.rules;
    let foresight = ability(tx, observer, "foresight").saturating_add(care_skill(tx, observer));
    let growth = u128::from(tx.now().0.saturating_sub(at.0))
        * u128::from(rules.hunger_per_tick)
        * u128::from(memory.appetite)
        / u128::from(rules.tick.0)
        / 1000;
    Some(need.saturating_add(
        (growth * u128::from(foresight) / (1000 + u128::from(foresight))).min(1000) as u64,
    ))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Notice {
    pub observer: PersonId,
    pub person: PersonId,
}
impl Request for Notice {
    const KEY: &'static str = "communication.notice";
    type Response = Perception;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Signalled {
    pub person: PersonId,
    pub cue: String,
    pub intensity: u64,
}
impl Event for Signalled {
    const KEY: &'static str = "communication.signalled";
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("signals", 1)?;
    b.resource::<Signals>()?;
    expression(b)?;
    b.request::<Enable>(|tx, _| {
        tx.edit::<Signals>()?.enabled = true;
        Ok(())
    })?;
    b.subscribe::<game_physiology::Fed>("communication.witnessed-feeding", |tx, e| {
        if active(tx) && e.caregiver != e.patient {
            let at = tx.now();
            let prior = tx
                .get::<Signals>()?
                .perceptions
                .get(&e.caregiver)
                .and_then(|r| r.get(&e.patient));
            let appetite = prior.map_or(1000, |p| p.appetite);
            let need = estimated_hunger(tx, e.caregiver, e.patient).unwrap_or(0);
            let food = tx
                .get::<game_objects::Objects>()?
                .by_id(e.good)?
                .food
                .as_ref()
                .unwrap();
            let relief = (u128::from(food.relief) * u128::from(e.quantity)
                / u128::from(food.portion)) as u64;
            let area = game_transport::area(tx.get()?, e.caregiver).ok();
            tx.edit::<Signals>()?
                .perceptions
                .entry(e.caregiver)
                .or_default()
                .insert(
                    e.patient,
                    Perception {
                        cue: "fed".into(),
                        guessed_need: None,
                        estimated_need: 0,
                        at,
                        last_fed: Some(at),
                        food: Some((at, need.saturating_sub(relief))),
                        appetite,
                        area,
                    },
                );
        }
        Ok(())
    })?;
    b.request::<Notice>(|tx, r| {
        local(tx, r.observer, r.person)?;
        require(active(tx), "communication is inactive")?;
        let body = &tx.get::<game_physiology::Bodies>()?.people[&r.person];
        let food = body.hunger.min(1000);
        let rest = body.fatigue.min(1000);
        let pain = body
            .maximum_health
            .unwrap_or(100)
            .saturating_sub(body.health)
            .min(1000);
        let (need, intensity) = [("food", food), ("rest", rest), ("pain", pain)]
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .unwrap();
        let voice = ability(tx, r.person, "vocalization");
        let gesture = ability(tx, r.person, "gesturing");
        let can_hear = ability(tx, r.observer, "hearing") > 0;
        let cue = if intensity == 0 {
            "quiet"
        } else if gesture >= 400 {
            match need {
                "food" => "reaching-mouth",
                "rest" => "drooping",
                _ => "wincing",
            }
        } else if voice > 0 && can_hear {
            "crying"
        } else if gesture > 0 {
            "restless"
        } else {
            "quiet"
        };
        let understanding = ability(tx, r.observer, "interpretation")
            .saturating_add(game_vitality::ability(tx, r.observer, "intelligence") / 2)
            .saturating_add(care_skill(tx, r.observer));
        // A cry is ambiguous. A thoughtful observer can infer food, but may be wrong.
        let last_fed = tx
            .get::<Signals>()?
            .perceptions
            .get(&r.observer)
            .and_then(|row| row.get(&r.person))
            .and_then(|p| p.last_fed);
        let elapsed = last_fed.map_or(0, |at| tx.now().0.saturating_sub(at.0));
        let anticipated = cue == "quiet"
            && understanding >= 800
            && game_vitality::ability(tx, r.observer, "foresight") > 500
            && elapsed >= tx.get::<game_physiology::Bodies>()?.rules.tick.0;
        let guessed_need = if anticipated {
            Some("food".into())
        } else if cue == "quiet" || understanding == 0 {
            None
        } else if understanding >= 800 {
            Some(
                match cue {
                    "drooping" => "rest",
                    "wincing" => "pain",
                    _ => "food",
                }
                .to_string(),
            )
        } else {
            None
        };
        let estimated_need = if anticipated {
            let rules = &tx.get::<game_physiology::Bodies>()?.rules;
            (elapsed / rules.tick.0)
                .saturating_mul(rules.hunger_per_tick)
                .min(1000)
        } else if guessed_need.is_some() {
            let precision = 10 - (care_skill(tx, r.observer) / 100).min(9);
            (intensity / precision + 1) * precision
        } else {
            0
        };
        let previous = tx
            .get::<Signals>()?
            .perceptions
            .get(&r.observer)
            .and_then(|row| row.get(&r.person));
        let mut appetite = previous.map_or(1000, |p| p.appetite);
        let mut food = previous.and_then(|p| p.food);
        if guessed_need.as_deref() == Some("food") {
            if let Some((at, before)) = food {
                let elapsed = tx.now().0.saturating_sub(at.0);
                let rules = &tx.get::<game_physiology::Bodies>()?.rules;
                if elapsed >= rules.tick.0 && estimated_need > before && rules.hunger_per_tick > 0 {
                    let learned =
                        u128::from(estimated_need - before) * u128::from(rules.tick.0) * 1000
                            / u128::from(elapsed)
                            / u128::from(rules.hunger_per_tick);
                    appetite = ((u128::from(appetite) + learned.min(8000)) / 2) as u64;
                }
            }
            food = Some((tx.now(), estimated_need));
        }
        let perception = Perception {
            cue: cue.into(),
            guessed_need,
            estimated_need,
            at: tx.now(),
            last_fed,
            food,
            appetite,
            area: game_transport::area(tx.get()?, r.observer).ok(),
        };
        tx.edit::<Signals>()?
            .perceptions
            .entry(r.observer)
            .or_default()
            .insert(r.person, perception.clone());
        Ok(perception)
    })
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Express {
    pub person: PersonId,
}
impl Request for Express {
    const KEY: &'static str = "signals.express";
    type Response = ();
}
fn expression(b: &mut Builder) -> Result<()> {
    b.request::<Express>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        require(
            ability(tx, r.person, "vocalization") > 0 || ability(tx, r.person, "gesturing") > 0,
            "no available signal",
        )?;
        let body = &tx.get::<game_physiology::Bodies>()?.people[&r.person];
        let intensity = body.hunger.max(body.fatigue);
        let at = tx.now();
        tx.edit::<Signals>()?
            .expressed
            .insert(r.person, (at, intensity));
        tx.emit(
            Signalled {
                person: r.person,
                cue: "expressed-need".into(),
                intensity: intensity / 10 * 10,
            },
            vec![r.person],
        )
    })?;
    game_choice::provider(b, "signals.options", |tx, e| {
        if !active(tx) || !game_choice::collecting(tx, e.person) {
            return Ok(());
        }
        let body = &tx.get::<game_physiology::Bodies>()?.people[&e.person];
        let intensity = body.hunger.max(body.fatigue).min(1000);
        let recent = tx
            .get::<Signals>()?
            .expressed
            .get(&e.person)
            .map_or(0, |(at, n)| {
                n.saturating_sub(tx.now().0.saturating_sub(at.0) / MINUTE.0)
            });
        let difficulty = if game_vitality::can_walk(tx, e.person) {
            30
        } else {
            0
        };
        let independence = game_vitality::disposition(tx, e.person, "patience") / 100;
        game_choice::offer(
            tx,
            e.person,
            "signal:need",
            game_choice::motive(
                intensity.saturating_sub(recent) as i64 * 2,
                0,
                0,
                difficulty + independence,
                0,
            ),
            game_choice::Undertake {
                person: e.person,
                kind: "express-need".into(),
                duration: MINUTE,
                request: Pending::immediate(Express { person: e.person }),
            },
        )?;
        Ok(())
    })
}
