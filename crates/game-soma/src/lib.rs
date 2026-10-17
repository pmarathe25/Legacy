//! Live bodily state and owner-validated physical changes.
use game_body::*;
use game_core::*;
use game_packed::Packed;
use game_storage::Table;
mod storage;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod development;
pub use development::{Express, maximum_health};
pub use game_metabolism::Environment as MetabolicEnvironment;
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(from = "storage::Saved")]
pub struct Bodies {
    pub people: Packed<PersonId, Body>,
    pub rules: Rules,
    pub ticking: bool,
    active: Table<PersonId, ()>,
    clock: SimTime,
}
type BodyMark = (game_packed::Mark, Rules, bool, Table<PersonId, ()>, SimTime);
impl Resource for Bodies {
    fn allocated_ids(&self) -> Vec<u64> {
        self.people.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "physiology.bodies";
    fn validate(&self) -> Result<()> {
        require(
            self.active.len() == self.people.values().filter(|b| b.alive).count()
                && self
                    .active
                    .keys()
                    .all(|p| self.people.get(p).is_some_and(|b| b.alive)),
            "invalid living index",
        )?;
        self.validate_rows(self.people.rows().map(|(_, b)| b))
    }
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new((
            self.people.begin(),
            self.rules.clone(),
            self.ticking,
            self.active.clone(),
            self.clock,
        ))
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        let (mark, rules, ticking, active, clock) =
            *checkpoint.downcast::<BodyMark>().expect("body checkpoint");
        self.people.rollback(mark);
        self.rules = rules;
        self.ticking = ticking;
        self.active = active;
        self.clock = clock;
    }
    fn commit(&mut self, checkpoint: Checkpoint) {
        self.people.commit(
            checkpoint
                .downcast::<BodyMark>()
                .expect("body checkpoint")
                .0,
        );
    }
    fn validate_checkpoint(&self, checkpoint: &dyn std::any::Any) -> Result<()> {
        let previous = checkpoint
            .downcast_ref::<BodyMark>()
            .expect("body checkpoint");
        if self.rules != previous.1 {
            return self.validate();
        }
        self.validate_rows(self.people.changed_since(&previous.0).map(|(_, row)| row))
    }
}
pub fn view(host: &Host, person: PersonId) -> Result<Option<BodyView>> {
    Ok(host
        .get::<Bodies>()?
        .people
        .get(&person)
        .map(BodyView::from))
}
pub fn capable(tx: &Transaction<'_>, person: PersonId) -> Result<()> {
    let p = tx
        .get::<Bodies>()?
        .people
        .get(&person)
        .ok_or_else(|| Error::Missing("body".into()))?;
    require(p.alive && p.health > 0, "actor cannot act")
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Create {
    pub kind: Kind,
    pub born: SimTime,
}
impl Request for Create {
    const KEY: &'static str = "physiology.create";
    type Response = PersonId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Damage {
    pub person: PersonId,
    pub amount: u64,
}
impl Request for Damage {
    const KEY: &'static str = "physiology.damage";
    type Response = ();
}
fn body_mut<'a>(tx: &'a mut Transaction<'_>, person: PersonId) -> Result<&'a mut Body> {
    tx.edit::<Bodies>()?
        .people
        .get_mut(&person)
        .ok_or_else(|| Error::Missing("body".into()))
}
fn death(tx: &mut Transaction<'_>, person: PersonId, injury: bool) -> Result<()> {
    let body = &tx.get::<Bodies>()?.people[&person];
    if body.kind == Kind::Mortal && body.alive && body.health == 0 {
        tx.edit::<Bodies>()?.people.get_mut(&person).unwrap().alive = false;
        tx.edit::<Bodies>()?.active.remove(&person);
        tx.cancel_action(person)?;
        if tx.registered::<game_nutrition::Deactivate>() {
            tx.call(game_nutrition::Deactivate { person })?;
        }
        tx.emit(
            Died {
                person,
                at: tx.now(),
                body: Some(tx.get::<Bodies>()?.people[&person]),
                cause: Some(tx.get::<Bodies>()?.people[&person].cause(
                    injury,
                    &tx.get::<Bodies>()?.rules,
                    tx.now(),
                )),
            },
            vec![person],
        )?;
    } else if body.kind == Kind::Guardian && body.health == 0 {
        tx.cancel_action(person)?;
    }
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Heal {
    pub person: PersonId,
    pub amount: u64,
}
impl Request for Heal {
    const KEY: &'static str = "physiology.heal";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Strain {
    pub person: PersonId,
    pub amount: u64,
}
impl Request for Strain {
    const KEY: &'static str = "physiology.strain";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ShortenLife {
    pub person: PersonId,
    pub duration: SimDuration,
}
impl Request for ShortenLife {
    const KEY: &'static str = "physiology.shorten_life";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Configure(pub Rules);
impl Request for Configure {
    const KEY: &'static str = "physiology.configure";
    type Response = ();
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Advance;
impl Request for Advance {
    const KEY: &'static str = "soma.advance";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Begin;
impl Request for Begin {
    const KEY: &'static str = "soma.begin";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct SetResting {
    pub person: PersonId,
    pub resting: bool,
}
impl Request for SetResting {
    const KEY: &'static str = "soma.resting";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Nourish {
    pub person: PersonId,
    pub relief: u64,
    pub health: i32,
    pub alertness: i32,
}
impl Request for Nourish {
    const KEY: &'static str = "soma.nourish";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("soma", 1)?;
    b.resource::<Bodies>()?;
    b.request::<Configure>(|tx, r| {
        require(
            tx.get::<Bodies>()?.people.is_empty(),
            "physiology is in use",
        )?;
        tx.edit::<Bodies>()?.rules = r.0;
        Ok(())
    })?;
    b.request::<Create>(|tx, r| {
        require(r.born <= tx.now(), "birth is in the future")?;
        let person = PersonId(tx.allocate()?);
        let max_health = tx.get::<Bodies>()?.rules.max_health;
        let now = tx.now();
        let life_limit = if r.kind == Kind::Mortal {
            Some(r.born.after(tx.get::<Bodies>()?.rules.lifespan)?)
        } else {
            None
        };
        tx.edit::<Bodies>()?.active.insert(person, ());
        tx.edit::<Bodies>()?.people.insert(
            person,
            Body {
                kind: r.kind,
                born: r.born,
                alive: true,
                hunger: 0,
                fatigue: 0,
                health: max_health,
                maximum_health: None,
                fatigue_fraction: 0,
                hunger_fraction: 0,
                recovery_fraction: 0,
                metabolic_rates: Default::default(),
                resting: false,
                last: now,
                life_limit,
            },
        );
        Ok(person)
    })?;
    b.request::<Heal>(|tx, r| {
        let max = maximum_health(tx.get()?, r.person);
        let p = body_mut(tx, r.person)?;
        require(p.alive, "cannot revive the dead")?;
        p.health = p.health.saturating_add(r.amount).min(max);
        Ok(())
    })?;
    b.request::<Strain>(|tx, r| {
        let amount = r.amount.saturating_mul(1000)
            / game_vitality::factor(game_vitality::ability(tx, r.person, "endurance"));
        let p = body_mut(tx, r.person)?;
        p.fatigue = p.fatigue.saturating_add(amount);
        Ok(())
    })?;
    b.request::<ShortenLife>(|tx, r| {
        let now = tx.now();
        let p = body_mut(tx, r.person)?;
        if let Some(limit) = p.life_limit {
            p.life_limit = Some(SimTime(limit.0.saturating_sub(r.duration.0)));
            if p.life_limit.is_some_and(|limit| limit <= now) {
                p.health = 0;
            }
        }
        death(tx, r.person, false)
    })?;
    b.request::<Damage>(|tx, r| {
        let amount = (r.amount.saturating_mul(1000)
            / game_vitality::factor(game_vitality::ability(tx, r.person, "robustness")))
        .max(u64::from(r.amount > 0));
        let p = body_mut(tx, r.person)?;
        p.health = p.health.saturating_sub(amount);
        tx.emit(
            Damaged {
                person: r.person,
                amount,
            },
            vec![r.person],
        )?;
        death(tx, r.person, true)
    })?;
    b.request::<Advance>(|tx, _| {
        let _measurement = game_diagnostics::span("physiology advance");
        let rules = tx.get::<Bodies>()?.rules.clone();
        let at = tx.now();
        let traits = tx.get::<game_traits::Traits>().ok().cloned();
        let vitality = tx.get::<game_vitality::Vitality>().ok().cloned();
        let mut environment = std::collections::BTreeMap::new();
        if tx.registered::<MetabolicEnvironment>() {
            let people: Vec<_> = tx.get::<Bodies>()?.living().map(|(p, _)| *p).collect();
            for person in people {
                environment.insert(person, tx.call(MetabolicEnvironment { person })?);
            }
        }
        tx.edit_with_compute::<Bodies, _>(|state, pool| {
            let update = |p: &PersonId, body: &mut Body| {
                *body = development::advance(
                    body,
                    &rules,
                    at,
                    *p,
                    traits.as_ref(),
                    vitality.as_ref(),
                    environment.get(p).copied().unwrap_or_default(),
                )
            };
            if state.active.len() < state.people.len() / 2 {
                for id in state.active.keys() {
                    update(id, state.people.get_mut(id).unwrap());
                }
            } else {
                state.people.update(pool, update);
            }
            state.clock = at;
        })?;
        development::condition(tx)?;
        // Only deaths and incapacitated guardians require cross-feature effects.
        let mut affected: Vec<_> = tx
            .get::<Bodies>()?
            .living()
            .filter(|(_, p)| p.health == 0)
            .map(|(id, _)| *id)
            .collect();
        affected.sort_unstable();
        for id in affected {
            death(tx, id, false)?;
        }
        Ok(())
    })?;
    b.request::<Begin>(|tx, _| {
        require(!tx.get::<Bodies>()?.ticking, "already ticking")?;
        tx.edit::<Bodies>()?.ticking = true;
        Ok(())
    })?;
    b.request::<SetResting>(|tx, r| {
        let p = body_mut(tx, r.person)?;
        p.resting = r.resting;
        Ok(())
    })?;
    b.request::<Nourish>(|tx, r| {
        let max = maximum_health(tx.get()?, r.person);
        let p = body_mut(tx, r.person)?;
        require(p.alive, "cannot feed a dead body")?;
        if r.relief > p.hunger {
            p.hunger_fraction = 0;
        }
        p.hunger = p.hunger.saturating_sub(r.relief);
        p.health = if r.health >= 0 {
            p.health.saturating_add(r.health as u64).min(max)
        } else {
            p.health.saturating_sub(u64::from(r.health.unsigned_abs()))
        };
        p.fatigue = if r.alertness >= 0 {
            p.fatigue.saturating_sub(r.alertness as u64)
        } else {
            p.fatigue
                .saturating_add(u64::from(r.alertness.unsigned_abs()))
        };
        death(tx, r.person, r.health < 0)
    })?;
    development::register(b)
}

impl Bodies {
    fn validate_rows<'a>(&'a self, rows: impl Iterator<Item = &'a Body>) -> Result<()> {
        require(
            self.rules.tick.0 > 0 && self.rules.max_health > 0,
            "invalid physiology rates",
        )?;
        for p in rows {
            require(
                p.health <= p.maximum(&self.rules)
                    && p.fatigue_fraction < 1000
                    && p.hunger_fraction < 1000
                    && p.recovery_fraction < 1000
                    && (p.kind == Kind::Mortal || p.alive),
                "invalid body",
            )?;
        }
        Ok(())
    }
}
