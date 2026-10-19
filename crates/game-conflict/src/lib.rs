//! Headless tactical fixture: continuous coordinates and fixed shared rounds.
use game_storage::Records;
mod geometry;
mod resolution;
mod view;
pub use geometry::{Map, Point};
pub use resolution::{Effect, OrderKind, resolve};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use view::{BattleView, view};
use {game_core::*, game_sim::*};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fighter {
    pub team: u32,
    pub point: Point,
    pub health: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battle {
    pub area: AreaId,
    pub map: Map,
    pub fighters: BTreeMap<PersonId, Fighter>,
    pub controlled: BTreeSet<PersonId>,
    pub orders: BTreeMap<PersonId, OrderKind>,
    pub round: u64,
    pub interval: SimDuration,
    pub movement: i64,
    pub reach: i64,
    pub damage: u64,
    pub scheduled: Option<ScheduleId>,
    pub finished: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Battles(pub Records<u64, Battle>);
impl Resource for Battles {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().copied().collect()
    }
    const KEY: &'static str = "conflict.battles";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.0.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.0.changed(&previous.0).map(|(_, row)| row))
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Start(pub Battle);
impl Request for Start {
    const KEY: &'static str = "conflict.start";
    type Response = u64;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Order {
    pub person: PersonId,
    pub battle: u64,
    pub action: OrderKind,
}
impl Request for Order {
    const KEY: &'static str = "conflict.order";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Round {
    battle: u64,
}
impl Request for Round {
    const KEY: &'static str = "conflict.round";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel {
    battle: u64,
    person: PersonId,
}
impl Request for Cancel {
    const KEY: &'static str = "conflict.cancel";
    type Response = ();
}
/// The optional invocation owner serves this fact, avoiding a reverse dependency.
#[derive(Clone, Serialize, Deserialize)]
pub struct HasPlayerControl {
    pub person: PersonId,
}
impl Request for HasPlayerControl {
    const KEY: &'static str = "conflict.player_control";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RefreshTurns;
/// Direct evidence of a resolved hit, emitted after the round's simultaneous damage.
#[derive(Clone, Serialize, Deserialize)]
pub struct Struck {
    pub battle: u64,
    pub actor: PersonId,
    pub target: PersonId,
    pub amount: u64,
}
impl Event for Struck {
    const KEY: &'static str = "conflict.struck";
}
impl Request for RefreshTurns {
    const KEY: &'static str = "conflict.refresh_turns";
    type Response = ();
}
fn queue(tx: &mut Transaction<'_>, id: u64) -> Result<()> {
    let battle = tx.get::<Battles>()?.0[&id].clone();
    if battle.finished || battle.scheduled.is_some() {
        return Ok(());
    }
    for person in &battle.controlled {
        if battle.fighters[person].health == 0 || battle.orders.contains_key(person) {
            continue;
        }
        if !tx.registered::<HasPlayerControl>() || tx.call(HasPlayerControl { person: *person })? {
            return Ok(());
        }
    }
    let at = tx.now().after(battle.interval)?;
    let job = tx.schedule(at, 15, Round { battle: id })?;
    tx.edit::<Battles>()?.0.get_mut(&id).unwrap().scheduled = Some(job);
    Ok(())
}
type WaitingVersion = game_storage::Version<u64, std::sync::Arc<Battle>>;
thread_local! {
    static WAITING:std::cell::RefCell<(Option<WaitingVersion>,Vec<u64>)>=Default::default();
}
fn awaiting(host: &Host) -> Result<Option<String>> {
    let battles = &host.get::<Battles>()?.0;
    let waiting = WAITING.with(|slot| {
        let mut cache = slot.borrow_mut();
        if cache
            .0
            .as_ref()
            .is_none_or(|version| !battles.matches(version))
        {
            cache.1 = battles
                .iter()
                .filter(|(_, b)| !b.finished && b.scheduled.is_none() && !b.controlled.is_empty())
                .map(|(id, _)| *id)
                .collect();
            cache.0 = Some(battles.version());
        }
        cache.1.clone()
    });
    for id in waiting {
        let b = &battles[&id];
        for person in &b.controlled {
            if b.fighters[person].health > 0
                && !b.orders.contains_key(person)
                && (!host.registered::<HasPlayerControl>()
                    || host.query(HasPlayerControl { person: *person })?)
            {
                return Ok(Some(format!("combat:{id}")));
            }
        }
    }
    Ok(None)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("conflict", 1)?;
    b.resource::<Battles>()?;
    b.validate_save(|h| {
        for b in h.get::<Battles>()?.0.values() {
            for person in b.fighters.keys() {
                require(
                    h.get::<game_physiology::Bodies>()?
                        .people
                        .contains_key(person),
                    "unknown fighter",
                )?;
            }
        }
        Ok(())
    });
    b.stop(awaiting);
    b.request::<RefreshTurns>(|tx, _| {
        let ids: Vec<_> = tx.get::<Battles>()?.0.keys().copied().collect();
        for id in ids {
            queue(tx, id)?;
        }
        Ok(())
    })?;
    b.request::<Start>(|tx, r| {
        let mut battle = r.0;
        require(
            battle.round == 0
                && battle.scheduled.is_none()
                && !battle.finished
                && battle.orders.is_empty()
                && battle.fighters.len() >= 2,
            "invalid new encounter",
        )?;
        let id = tx.allocate()?;
        for (person, f) in &mut battle.fighters {
            game_physiology::capable(tx, *person)?;
            require(
                game_transport::area(tx.get()?, *person)? == battle.area,
                "fighter is elsewhere",
            )?;
            f.health = tx.get::<game_physiology::Bodies>()?.people[person].health;
            tx.start_action(
                *person,
                "combat",
                None,
                Cancel {
                    battle: id,
                    person: *person,
                },
            )?;
        }
        tx.edit::<Battles>()?.0.insert(id, battle);
        queue(tx, id)?;
        Ok(id)
    })?;
    b.request::<Order>(|tx, r| {
        require(
            match r.action {
                OrderKind::Attack(_) => game_vitality::can_manipulate(tx, r.person),
                OrderKind::Move(_) | OrderKind::Retreat => game_vitality::can_walk(tx, r.person),
                OrderKind::Wait => true,
            },
            "combat action exceeds motor capacity",
        )?;
        let battle = tx
            .get::<Battles>()?
            .0
            .get(&r.battle)
            .ok_or_else(|| Error::Missing("battle".into()))?;
        require(
            !battle.finished
                && battle.scheduled.is_none()
                && battle.controlled.contains(&r.person)
                && battle.fighters[&r.person].health > 0,
            "turn unavailable",
        )?;
        match r.action {
            OrderKind::Move(point) => require(
                battle.map.contains(point)
                    && battle.map.clear(battle.fighters[&r.person].point, point)
                    && geometry::distance2(battle.fighters[&r.person].point, point)
                        <= i128::from(battle.movement).pow(2),
                "invalid tactical move",
            )?,
            OrderKind::Attack(target) => require(
                battle
                    .fighters
                    .get(&target)
                    .is_some_and(|f| f.health > 0 && f.team != battle.fighters[&r.person].team),
                "invalid target",
            )?,
            _ => {}
        }
        tx.edit::<Battles>()?
            .0
            .get_mut(&r.battle)
            .unwrap()
            .orders
            .insert(r.person, r.action);
        queue(tx, r.battle)
    })?;
    b.request::<Round>(|tx, r| {
        let mut snapshot = tx.get::<Battles>()?.0[&r.battle].clone();
        for person in snapshot.fighters.keys() {
            let order = snapshot.orders.get(person);
            let possible = match order {
                Some(OrderKind::Move(_) | OrderKind::Retreat) => {
                    game_vitality::can_walk(tx, *person)
                }
                Some(OrderKind::Wait) => true,
                _ => game_vitality::can_manipulate(tx, *person),
            };
            if !possible {
                snapshot.orders.insert(*person, OrderKind::Wait);
            }
        }
        let strength = snapshot
            .fighters
            .keys()
            .map(|p| (*p, game_vitality::ability(tx, *p, "strength")))
            .collect();
        let agility = snapshot
            .fighters
            .keys()
            .map(|p| (*p, game_vitality::ability(tx, *p, "agility")))
            .collect();
        let effects = resolution::resolve_with_agility(&snapshot, &strength, &agility);
        for effect in effects {
            match effect {
                Effect::Moved(person, point) => {
                    tx.edit::<Battles>()?
                        .0
                        .get_mut(&r.battle)
                        .unwrap()
                        .fighters
                        .get_mut(&person)
                        .unwrap()
                        .point = point;
                }
                Effect::Damage(person, amount) => {
                    tx.call(game_physiology::Damage { person, amount })?;
                }
                Effect::Retreated(person) => {
                    tx.cancel_action(person)?;
                }
                Effect::Hit {
                    actor,
                    target,
                    amount,
                } => {
                    tx.emit(
                        Struck {
                            battle: r.battle,
                            actor,
                            target,
                            amount,
                        },
                        vec![actor, target],
                    )?;
                }
            }
        }
        let health: BTreeMap<_, _> = snapshot
            .fighters
            .keys()
            .map(|p| {
                (
                    *p,
                    tx.get::<game_physiology::Bodies>().unwrap().people[p].health,
                )
            })
            .collect();
        let battle = tx.edit::<Battles>()?.0.get_mut(&r.battle).unwrap();
        for (person, f) in &mut battle.fighters {
            if f.health > 0 {
                f.health = health[person];
            }
        }
        battle.round += 1;
        battle.orders.clear();
        battle.scheduled = None;
        let teams: BTreeSet<_> = battle
            .fighters
            .values()
            .filter(|f| f.health > 0)
            .map(|f| f.team)
            .collect();
        let finished = teams.len() <= 1;
        battle.finished = finished;
        if finished {
            for person in snapshot.fighters.keys() {
                if tx
                    .get::<Actions>()?
                    .0
                    .get(person)
                    .is_some_and(|a| a.kind == "combat")
                {
                    tx.finish_action(*person, "combat")?;
                }
            }
        } else {
            queue(tx, r.battle)?;
        }
        Ok(())
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Some(b) = tx.edit::<Battles>()?.0.get_mut(&r.battle) {
            b.fighters.get_mut(&r.person).unwrap().health = 0;
        }
        queue(tx, r.battle)
    })
}
impl Battles {
    fn validate_rows<'a>(&'a self, rows: impl Iterator<Item = &'a Battle>) -> Result<()> {
        for b in rows {
            require(
                b.interval.0 > 0
                    && b.movement > 0
                    && b.reach > 0
                    && b.map.width > 0
                    && b.map.width <= 1_000_000
                    && b.map.valid(),
                "invalid tactical rules",
            )?;
            require(
                b.controlled.iter().all(|p| b.fighters.contains_key(p))
                    && b.fighters.values().all(|f| b.map.contains(f.point)),
                "invalid tactical positions",
            )?;
        }
        Ok(())
    }
}
