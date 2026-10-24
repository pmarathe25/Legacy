mod perception;
use game_core::*;
use game_sim::*;
use game_storage::Table;
use game_vitality::ability;
pub use perception::{ApplyAssessment, Assessment, assess};
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
    require(
        tx.get::<game_physiology::Bodies>()?
            .people
            .get(&b)
            .is_some_and(|body| body.alive),
        "signal subject is dead or unknown",
    )?;
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
/// Transient signal context follows the memory owner's inherited retention policy.
pub fn perception<'a>(
    tx: &'a Transaction<'_>,
    observer: PersonId,
    person: PersonId,
) -> Option<&'a Perception> {
    let p = remembered(tx, observer, person)?;
    (tx.now().0.saturating_sub(p.at.0) < game_memory::short_term_duration(tx, observer).0)
        .then_some(p)
}
fn remembered<'a>(
    tx: &'a Transaction<'_>,
    observer: PersonId,
    person: PersonId,
) -> Option<&'a Perception> {
    tx.get::<Signals>()
        .ok()?
        .perceptions
        .get(&observer)?
        .get(&person)
}
/// A subjective forecast, learned from observed hunger and feeding intervals.
pub fn estimated_hunger(tx: &Transaction<'_>, observer: PersonId, person: PersonId) -> Option<u64> {
    let memory = perception(tx, observer, person)?;
    perception::forecast(tx, observer, memory)
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
            let prior = perception(tx, e.caregiver, e.patient);
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
    perception::register(b)
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
