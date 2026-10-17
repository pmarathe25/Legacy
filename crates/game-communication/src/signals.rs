use super::*;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Perception {
    pub cue: String,
    pub guessed_need: Option<String>,
    pub estimated_need: u64,
    pub at: SimTime,
    #[serde(default)]
    pub last_fed: Option<SimTime>,
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
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.subscribe::<game_physiology::Fed>("communication.witnessed-feeding", |tx, e| {
        if active(tx) && e.caregiver != e.patient {
            let at = tx.now();
            tx.edit::<Communication>()?
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
            .saturating_add(game_vitality::ability(tx, r.observer, "intelligence") / 2);
        // A cry is ambiguous. A thoughtful observer can infer food, but may be wrong.
        let last_fed = tx
            .get::<Communication>()?
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
            (intensity / 10 + 1) * 10
        } else {
            0
        };
        let perception = Perception {
            cue: cue.into(),
            guessed_need,
            estimated_need,
            at: tx.now(),
            last_fed,
        };
        let previous = tx
            .get::<Communication>()?
            .perceptions
            .get(&r.observer)
            .and_then(|row| row.get(&r.person));
        let changed = previous
            .is_none_or(|p| p.cue != perception.cue || p.guessed_need != perception.guessed_need);
        tx.edit::<Communication>()?
            .perceptions
            .entry(r.observer)
            .or_default()
            .insert(r.person, perception.clone());
        if changed && cue != "quiet" {
            tx.emit(
                Signalled {
                    person: r.person,
                    cue: cue.into(),
                    intensity: estimated_need,
                },
                vec![r.person, r.observer],
            )?;
        }
        Ok(perception)
    })
}
