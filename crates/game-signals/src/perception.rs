//! Pure signal assessment and ordered application through the signal owner.
use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assessment {
    observer: PersonId,
    person: PersonId,
    previous: Option<Perception>,
    perception: Perception,
}
impl Assessment {
    pub fn perception(&self) -> &Perception {
        &self.perception
    }
    pub fn food_need(&self, tx: &Transaction<'_>) -> u64 {
        if self.perception.guessed_need.as_deref() == Some("food") {
            self.perception.estimated_need
        } else {
            forecast(tx, self.observer, &self.perception).unwrap_or(0)
        }
    }
}
pub(super) fn forecast(
    tx: &Transaction<'_>,
    observer: PersonId,
    memory: &Perception,
) -> Option<u64> {
    let (at, need) = memory.food?;
    if tx.now().0.saturating_sub(at.0) >= game_memory::short_term_duration(tx, observer).0 {
        return None;
    }
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
pub fn assess(tx: &Transaction<'_>, r: Notice) -> Result<Assessment> {
    let perception = calculate(tx, &r)?;
    Ok(Assessment {
        observer: r.observer,
        person: r.person,
        previous: super::perception(tx, r.observer, r.person).cloned(),
        perception,
    })
}
fn calculate(tx: &Transaction<'_>, r: &Notice) -> Result<Perception> {
    local(tx, r.observer, r.person)?;
    require(active(tx), "communication is inactive")?;
    let memory = remembered(tx, r.observer, r.person);
    let retention = memory.map_or(0, |_| game_memory::short_term_duration(tx, r.observer).0);
    let previous = memory.filter(|p| tx.now().0.saturating_sub(p.at.0) < retention);
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
    let last_fed = previous
        .and_then(|p| p.last_fed)
        .filter(|at| tx.now().0.saturating_sub(at.0) < retention);
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
    let mut appetite = previous.map_or(1000, |p| p.appetite);
    let mut food = previous
        .and_then(|p| p.food)
        .filter(|(at, _)| tx.now().0.saturating_sub(at.0) < retention);
    if guessed_need.as_deref() == Some("food") {
        if let Some((at, before)) = food {
            let elapsed = tx.now().0.saturating_sub(at.0);
            let rules = &tx.get::<game_physiology::Bodies>()?.rules;
            if elapsed >= rules.tick.0 && estimated_need > before && rules.hunger_per_tick > 0 {
                let learned = u128::from(estimated_need - before) * u128::from(rules.tick.0) * 1000
                    / u128::from(elapsed)
                    / u128::from(rules.hunger_per_tick);
                appetite = ((u128::from(appetite) + learned.min(8000)) / 2) as u64;
            }
        }
        food = Some((tx.now(), estimated_need));
    }
    Ok(Perception {
        cue: cue.into(),
        guessed_need,
        estimated_need,
        at: tx.now(),
        last_fed,
        food,
        appetite,
        area: game_transport::area(tx.get()?, r.observer).ok(),
    })
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ApplyAssessment(pub Assessment);
impl Request for ApplyAssessment {
    const KEY: &'static str = "signals.apply-assessment";
    type Response = ();
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Notice>(|tx, r| {
        let perception = calculate(tx, &r)?;
        tx.edit::<Signals>()?
            .perceptions
            .entry(r.observer)
            .or_default()
            .insert(r.person, perception.clone());
        Ok(perception)
    })?;
    b.request::<ApplyAssessment>(|tx, r| {
        let a = r.0;
        if a.perception.at != tx.now() || !active(tx) || local(tx, a.observer, a.person).is_err() {
            return Ok(());
        }
        let current = tx
            .get::<Signals>()?
            .perceptions
            .get(&a.observer)
            .and_then(|r| r.get(&a.person));
        if current != a.previous.as_ref() || tx.has_before::<Notice>() {
            tx.call(Notice {
                observer: a.observer,
                person: a.person,
            })?;
        } else {
            tx.edit::<Signals>()?
                .perceptions
                .entry(a.observer)
                .or_default()
                .insert(a.person, a.perception);
        }
        Ok(())
    })
}
