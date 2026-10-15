//! Needs, bodily harm, rest, and guardian recovery.
mod compute;
pub use compute::{Batch, advance_body};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Mortal,
    Guardian,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    pub kind: Kind,
    pub born: SimTime,
    pub alive: bool,
    pub hunger: u64,
    pub fatigue: u64,
    pub health: u64,
    pub resting: bool,
    pub last: SimTime,
    pub life_limit: Option<SimTime>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub lifespan: SimDuration,
    pub tick: SimDuration,
    pub hunger_per_tick: u64,
    pub fatigue_per_tick: u64,
    pub rest_per_tick: u64,
    pub deprivation: u64,
    pub harm_per_tick: u64,
    pub guardian_recovery: u64,

    pub max_health: u64,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            lifespan: SimDuration(DAY.0 * 365 * 80),
            tick: MINUTE,
            hunger_per_tick: 1,
            fatigue_per_tick: 1,
            rest_per_tick: 4,
            deprivation: 120,
            harm_per_tick: 1,
            guardian_recovery: 2,

            max_health: 100,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bodies {
    pub people: BTreeMap<PersonId, Body>,
    pub rules: Rules,
    pub ticking: bool,
}
impl Resource for Bodies {
    fn allocated_ids(&self) -> Vec<u64> {
        self.people.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "physiology.bodies";
    fn validate(&self) -> Result<()> {
        require(
            self.rules.tick.0 > 0 && self.rules.max_health > 0,
            "invalid physiology rates",
        )?;
        for p in self.people.values() {
            require(
                p.health <= self.rules.max_health && (p.kind == Kind::Mortal || p.alive),
                "invalid body",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BodyView {
    pub kind: Kind,
    pub born: SimTime,
    pub alive: bool,
    pub hunger: u64,
    pub fatigue: u64,
    pub health: u64,
}
pub fn view(host: &Host, person: PersonId) -> Result<Option<BodyView>> {
    Ok(host.get::<Bodies>()?.people.get(&person).map(|b| BodyView {
        kind: b.kind,
        born: b.born,
        alive: b.alive,
        hunger: b.hunger,
        fatigue: b.fatigue,
        health: b.health,
    }))
}
pub fn capable(tx: &Transaction<'_>, person: PersonId) -> Result<()> {
    let p = tx
        .get::<Bodies>()?
        .people
        .get(&person)
        .ok_or_else(|| Error::Missing("body".into()))?;
    require(p.alive && p.health > 0, "actor cannot act")
}
#[derive(Serialize, Deserialize)]
pub struct Create {
    pub kind: Kind,
    pub born: SimTime,
}
impl Request for Create {
    const KEY: &'static str = "physiology.create";
    type Response = PersonId;
}
#[derive(Serialize, Deserialize)]
pub struct Start;
impl Request for Start {
    const KEY: &'static str = "physiology.start";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Tick;
impl Request for Tick {
    const KEY: &'static str = "physiology.tick";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Eat {
    pub person: PersonId,
    pub inventory: InventoryId,
}
impl Request for Eat {
    const KEY: &'static str = "physiology.eat";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Rest {
    pub person: PersonId,
}
impl Request for Rest {
    const KEY: &'static str = "physiology.rest";
    type Response = SimTime;
}
#[derive(Serialize, Deserialize)]
struct Wake {
    person: PersonId,
}
impl Request for Wake {
    const KEY: &'static str = "physiology.wake";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct CancelRest {
    person: PersonId,
    job: ScheduleId,
}
impl Request for CancelRest {
    const KEY: &'static str = "physiology.cancel_rest";
    type Response = ();
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
#[derive(Clone, Serialize, Deserialize)]
pub struct Died {
    pub person: PersonId,
    pub at: SimTime,
}
impl Event for Died {
    const KEY: &'static str = "physiology.died";
}
fn death(tx: &mut Transaction<'_>, person: PersonId) -> Result<()> {
    let body = &tx.get::<Bodies>()?.people[&person];
    if body.kind == Kind::Mortal && body.alive && body.health == 0 {
        tx.edit::<Bodies>()?.people.get_mut(&person).unwrap().alive = false;
        tx.cancel_action(person)?;
        tx.emit(
            Died {
                person,
                at: tx.now(),
            },
            vec![person],
        )?;
    } else if body.kind == Kind::Guardian && body.health == 0 {
        tx.cancel_action(person)?;
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
pub struct Heal {
    pub person: PersonId,
    pub amount: u64,
}
impl Request for Heal {
    const KEY: &'static str = "physiology.heal";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Strain {
    pub person: PersonId,
    pub amount: u64,
}
impl Request for Strain {
    const KEY: &'static str = "physiology.strain";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct ShortenLife {
    pub person: PersonId,
    pub duration: SimDuration,
}
impl Request for ShortenLife {
    const KEY: &'static str = "physiology.shorten_life";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
pub struct Configure(pub Rules);
impl Request for Configure {
    const KEY: &'static str = "physiology.configure";
    type Response = ();
}
pub fn load(store: &mut game_content::Store, key: &str) -> Result<Rules> {
    let doc: game_content::Document<Rules> = store.load(&format!("physiology/{key}.toml"))?;
    let check = Bodies {
        rules: doc.data.clone(),
        ..Default::default()
    };
    check.validate()?;
    Ok(doc.data)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("physiology", 1)?;
    b.resource::<Bodies>()?;
    b.request::<Configure>(|tx, r| {
        require(
            tx.get::<Bodies>()?.people.is_empty(),
            "physiology is in use",
        )?;
        tx.edit::<Bodies>()?.rules = r.0;
        Ok(())
    })?;
    b.validate_save(|h| {
        for person in h.get::<game_transport::Positions>()?.0.keys() {
            require(
                h.get::<Bodies>()?.people.contains_key(person),
                "position references unknown body",
            )?;
        }
        Ok(())
    });
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
        tx.edit::<Bodies>()?.people.insert(
            person,
            Body {
                kind: r.kind,
                born: r.born,
                alive: true,
                hunger: 0,
                fatigue: 0,
                health: max_health,
                resting: false,
                last: now,
                life_limit,
            },
        );
        Ok(person)
    })?;
    b.request::<Start>(|tx, _| {
        require(!tx.get::<Bodies>()?.ticking, "already ticking")?;
        tx.edit::<Bodies>()?.ticking = true;
        tx.schedule(tx.now().after(tx.get::<Bodies>()?.rules.tick)?, 0, Tick)?;
        Ok(())
    })?;
    b.request::<Tick>(|tx, _| {
        let state = tx.get::<Bodies>()?;
        let input: Vec<_> = state
            .people
            .iter()
            .map(|(id, p)| (*id, p.clone()))
            .collect();
        let rules = state.rules.clone();
        let output = compute::Cpu.evaluate(&input, &rules, tx.now(), tx.compute());
        let people = &mut tx.edit::<Bodies>()?.people;
        for (id, body) in &output {
            people.insert(*id, body.clone());
        }
        for (id, _) in output {
            death(tx, id)?;
        }
        tx.schedule(tx.now().after(rules.tick)?, 0, Tick)?;
        Ok(())
    })?;
    b.request::<Eat>(|tx, r| {
        capable(tx, r.person)?;
        require(
            !tx.get::<Actions>()?.0.contains_key(&r.person),
            "actor is busy",
        )?;
        require(
            tx.get::<game_property::Property>()?
                .allowed(r.person, r.inventory),
            "access denied",
        )?;
        require(
            game_transport::area(tx.get()?, r.person)? == game_cargo::locality(tx, r.inventory)?,
            "food is elsewhere",
        )?;
        let (good, food) = tx
            .get::<game_objects::Objects>()?
            .meal(&tx.get::<game_cargo::Cargo>()?.0[&r.inventory].goods)
            .ok_or_else(|| Error::Rejected("no edible portion available".into()))?;
        tx.call(game_cargo::Consume {
            inventory: r.inventory,
            good,
            quantity: food.portion,
        })?;
        let body = tx.edit::<Bodies>()?.people.get_mut(&r.person).unwrap();
        body.hunger = body.hunger.saturating_sub(food.relief);
        Ok(())
    })?;
    b.request::<Rest>(|tx, r| {
        capable(tx, r.person)?;
        game_transport::area(tx.get()?, r.person)?;
        let until = tx.now().after(HOUR)?;
        let job = tx.schedule(until, 10, Wake { person: r.person })?;
        tx.start_action(
            r.person,
            "rest",
            Some(until),
            CancelRest {
                person: r.person,
                job,
            },
        )?;
        tx.edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .unwrap()
            .resting = true;
        Ok(until)
    })?;
    b.request::<Wake>(|tx, r| {
        tx.finish_action(r.person, "rest")?;
        tx.edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .unwrap()
            .resting = false;
        Ok(())
    })?;
    b.request::<CancelRest>(|tx, r| {
        tx.unschedule(r.job);
        tx.edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .unwrap()
            .resting = false;
        Ok(())
    })?;
    b.before::<game_property::Take>("physiology.taking", |tx, r| capable(tx, r.actor))?;
    b.before::<game_transport::Move>("physiology.capability", |tx, r| capable(tx, r.person))?;
    b.request::<Heal>(|tx, r| {
        let max = tx.get::<Bodies>()?.rules.max_health;
        let p = tx
            .edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .ok_or_else(|| Error::Missing("body".into()))?;
        require(p.alive, "cannot revive the dead")?;
        p.health = p.health.saturating_add(r.amount).min(max);
        Ok(())
    })?;
    b.request::<Strain>(|tx, r| {
        let p = tx
            .edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .ok_or_else(|| Error::Missing("body".into()))?;
        p.fatigue = p.fatigue.saturating_add(r.amount);
        Ok(())
    })?;
    b.request::<ShortenLife>(|tx, r| {
        let now = tx.now();
        let p = tx
            .edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .ok_or_else(|| Error::Missing("body".into()))?;
        if let Some(limit) = p.life_limit {
            p.life_limit = Some(SimTime(limit.0.saturating_sub(r.duration.0)));
            if p.life_limit.is_some_and(|limit| limit <= now) {
                p.health = 0;
            }
        }
        death(tx, r.person)
    })?;
    b.request::<Damage>(|tx, r| {
        let p = tx
            .edit::<Bodies>()?
            .people
            .get_mut(&r.person)
            .ok_or_else(|| Error::Missing("body".into()))?;
        p.health = p.health.saturating_sub(r.amount);
        death(tx, r.person)
    })
}
