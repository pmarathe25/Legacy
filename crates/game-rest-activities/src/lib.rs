//! Persistent recovery, registered under the physiology owner. Jobs are the epoch state.
use game_body::Rested;
use game_core::*;
use game_sim::*;
use game_soma::{Bodies, SetResting, SetRestingMany};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct RestActivity {
    pub person: PersonId,
    pub fatigue_above: u64,
    pub hunger_below: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RestActivities(pub Vec<RestActivity>);
impl Request for RestActivities {
    const KEY: &'static str = "physiology.rest-activities";
    type Response = ();
}
/// The character owner checks control, urgency and current personal commitments.
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckRestActivities {
    pub people: Vec<PersonId>,
    pub starting: bool,
}
impl Request for CheckRestActivities {
    const KEY: &'static str = "characters.check-rest-activities";
    type Response = bool;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct InterruptRest(pub PersonId);
impl Request for InterruptRest {
    const KEY: &'static str = "physiology.interrupt-rest-activity";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Cancel(PersonId);
impl Request for Cancel {
    const KEY: &'static str = "physiology.cancel-rest-activity";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Recovery {
    person: PersonId,
    action: u64,
    hunger_below: u64,
    health: u64,
    until: SimTime,
}
#[derive(Clone, Serialize, Deserialize)]
struct Epoch(Vec<Recovery>);
impl Request for Epoch {
    const KEY: &'static str = "physiology.rest-epoch";
    type Response = ();
}
pub fn resting_activity(tx: &Transaction<'_>, person: PersonId) -> bool {
    tx.get::<Actions>().is_ok_and(|s| {
        s.0.get(&person)
            .is_some_and(|a| a.cancel.key() == Cancel::KEY)
    })
}
fn live(tx: &Transaction<'_>, row: &Recovery) -> bool {
    tx.get::<Actions>().is_ok_and(|s| {
        s.0.get(&row.person)
            .is_some_and(|a| a.id == row.action && a.cancel.key() == Cancel::KEY)
    })
}
fn approved(tx: &mut Transaction<'_>, people: Vec<PersonId>, starting: bool) -> Result<bool> {
    if tx.registered::<CheckRestActivities>() {
        tx.call(CheckRestActivities { people, starting })
    } else {
        Ok(true)
    }
}
fn valid_start(tx: &Transaction<'_>, row: &RestActivity) -> bool {
    let bodies = tx.get::<Bodies>().unwrap();
    bodies.people.get(&row.person).is_some_and(|body| {
        body.alive
            && body.health > 0
            && body.health == body.maximum(&bodies.rules)
            && body.fatigue > row.fatigue_above
            && body.hunger < row.hunger_below
    }) && !tx.get::<Actions>().unwrap().0.contains_key(&row.person)
        && game_transport::area(tx.get().unwrap(), row.person).is_ok()
}
fn flags(tx: &mut Transaction<'_>, people: Vec<PersonId>, resting: bool) -> Result<()> {
    if tx.has_before::<SetResting>() {
        for person in people {
            tx.call(SetResting { person, resting })?;
        }
        Ok(())
    } else {
        tx.call(SetRestingMany { people, resting })
    }
}
fn schedule_epoch(tx: &mut Transaction<'_>, rows: Vec<Recovery>) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }
    let bodies = tx.get::<Bodies>()?;
    let mut at = tx.now().after(HOUR)?;
    for row in &rows {
        if let Some(due) = game_body::hunger_threshold(
            &bodies.people[&row.person],
            &bodies.rules,
            row.hunger_below,
        )
        .filter(|due| *due > tx.now())
        {
            at = at.min(due);
        }
        at = at.min(row.until);
    }
    tx.schedule(at, 10, Epoch(rows))?;
    Ok(())
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<Cancel>(|tx, r| {
        tx.call(SetResting {
            person: r.0,
            resting: false,
        })
    })?;
    b.request::<InterruptRest>(|tx, r| {
        if resting_activity(tx, r.0) {
            tx.cancel_action(r.0)?;
        }
        Ok(())
    })?;
    b.request::<RestActivities>(|tx, r| {
        require(
            r.0.len() <= 100_000 && r.0.windows(2).all(|p| p[0].person < p[1].person),
            "unordered rest activity starts",
        )?;
        let people: Vec<_> = r.0.iter().map(|row| row.person).collect();
        require(approved(tx, people.clone(), true)?, "rest priority changed")?;
        require(
            r.0.iter().all(|row| valid_start(tx, row)),
            "rest prerequisites changed",
        )?;
        flags(tx, people.clone(), true)?;
        require(
            approved(tx, people, true)?,
            "rest priority changed during body phase",
        )?;
        require(
            r.0.iter().all(|row| {
                valid_start(tx, row) && tx.get::<Bodies>().unwrap().people[&row.person].resting
            }),
            "rest prerequisites changed during body phase",
        )?;
        let until = tx.now().after(SimDuration(8 * HOUR.0))?;
        let mut rows = Vec::new();
        for row in r.0 {
            tx.start_action(row.person, "rest", None, Cancel(row.person))?;
            rows.push(Recovery {
                person: row.person,
                action: tx.get::<Actions>()?.0[&row.person].id,
                hunger_below: row.hunger_below,
                health: tx.get::<Bodies>()?.people[&row.person].health,
                until,
            });
        }
        game_diagnostics::interaction_profile!(count "activity.rest.started", rows.len() as u64);
        schedule_epoch(tx, rows)
    })?;
    b.request::<Epoch>(|tx, r| advance(tx, r.0))?;
    b.scheduled_wave::<Epoch>(128, |tx, requests| {
        advance(tx, requests.iter().flat_map(|r| r.0.clone()).collect())
    })
}
fn advance(tx: &mut Transaction<'_>, mut rows: Vec<Recovery>) -> Result<()> {
    rows.retain(|row| live(tx, row));
    rows.sort_by_key(|row| row.person);
    require(
        rows.windows(2).all(|p| p[0].person < p[1].person),
        "duplicate recovery row",
    )?;
    loop {
        rows.retain(|row| live(tx, row));
        if rows.is_empty() {
            return Ok(());
        }
        // One owner barrier checks the complete cohort after its hooks. Read the
        // physical snapshot only afterward; no callback follows a retained result.
        let allowed = approved(tx, rows.iter().map(|row| row.person).collect(), false)?;
        rows.retain(|row| live(tx, row));
        let bodies = tx.get::<Bodies>()?;
        let now = tx.now();
        let continuing: Vec<_> = tx.compute().install(|| {
            rows.par_iter()
                .map(|row| {
                    allowed
                        && bodies.people.get(&row.person).is_some_and(|body| {
                            body.alive
                                && body.health >= row.health
                                && body.resting
                                && body.fatigue > 0
                                && body.hunger < row.hunger_below
                                && now < row.until
                        })
                })
                .collect()
        });
        let mut finished = Vec::new();
        let mut retained = Vec::new();
        for (row, continuing) in rows.into_iter().zip(continuing) {
            if continuing {
                retained.push(row);
            } else {
                finished.push(row);
            }
        }
        if finished.is_empty() {
            game_diagnostics::interaction_profile!(count "activity.rest.continued", retained.len() as u64);
            return schedule_epoch(tx, retained);
        }
        // Every pass removes at least one row. Completion observers can invalidate
        // other rows, which are rechecked before any future epoch is armed.
        flags(tx, finished.iter().map(|row| row.person).collect(), false)?;
        require(
            finished.iter().all(|row| live(tx, row)),
            "rest action changed during completion body phase",
        )?;
        for row in finished {
            if live(tx, &row) {
                require(
                    !tx.get::<Bodies>()?.people[&row.person].resting,
                    "rest completion flag changed",
                )?;
                tx.finish_action(row.person, "rest")?;
                tx.emit(Rested { person: row.person }, vec![row.person])?;
            }
        }
        rows = retained;
    }
}
