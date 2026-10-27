//! Persistent local trials; registered under the mechanics owner.
use game_core::*;
use game_mechanics_state::*;
use game_physiology::Bodies;
use game_sim::*;
use serde::{Deserialize, Serialize};
mod wave;

type Finish = fn(&mut Transaction<'_>, PersonId) -> Result<bool>;
#[derive(Clone, Serialize, Deserialize)]
struct Cancel(PersonId);
impl Request for Cancel {
    const KEY: &'static str = "mechanics.cancel-session";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Row {
    person: PersonId,
    action: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Epoch(Vec<Row>);
impl Request for Epoch {
    const KEY: &'static str = "mechanics.session-epoch";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct TrialWave(Vec<Row>);
impl Request for TrialWave {
    const KEY: &'static str = "mechanics.trial-wave";
    type Response = ();
}
fn limits(tx: &mut Transaction<'_>, people: Vec<PersonId>) -> Result<Option<SessionLimits>> {
    if !game_choice::frame_tuning(tx).mechanics_sessions {
        return Ok(None);
    }
    if tx.registered::<CheckSessions>() {
        tx.call(CheckSessions(people))
    } else {
        Ok(Some(SessionLimits {
            hunger_below: 100,
            fatigue_below: 100,
        }))
    }
}
fn live(tx: &Transaction<'_>, row: &Row) -> bool {
    tx.get::<Actions>()
        .unwrap()
        .0
        .get(&row.person)
        .is_some_and(|a| a.id == row.action && a.cancel.key() == Cancel::KEY)
        && tx
            .get::<Machines>()
            .unwrap()
            .work
            .get(&row.person)
            .is_some_and(|w| w.session.as_ref().is_some_and(|s| s.action == row.action))
}
fn physical(tx: &Transaction<'_>, row: &Row) -> bool {
    if !live(tx, row) {
        return false;
    }
    let work = &tx.get::<Machines>().unwrap().work[&row.person];
    let session = work.session.as_ref().unwrap();
    let Some(body) = tx.get::<Bodies>().unwrap().people.get(&row.person) else {
        return false;
    };
    let Ok(area) = ready(tx, row.person) else {
        return false;
    };
    body.health >= session.health
        && !body.resting
        && body.hunger < session.limits.hunger_below
        && body.fatigue < session.limits.fatigue_below
        && tx
            .get::<Machines>()
            .unwrap()
            .devices
            .get(&work.device.unwrap())
            .is_some_and(|d| d.area == area)
        && game_cargo::locality(tx, work.weights.unwrap()).ok() == Some(area)
        && tx
            .get::<game_cargo::Cargo>()
            .unwrap()
            .0
            .get(&work.weights.unwrap())
            .is_some_and(|i| i.mass >= work.task.force)
}
/// Called inside the normal Experiment dispatch, after its registered hooks.
pub fn start(tx: &mut Transaction<'_>, request: Experiment) -> Result<SimTime> {
    let person = request.person;
    let apparatus = tx
        .get::<Machines>()?
        .devices
        .get(&request.device)
        .ok_or_else(|| Error::Missing("apparatus".into()))?
        .inventory;
    tx.call(game_property::Use {
        actor: person,
        inventory: apparatus,
        purpose: "using a mechanism".into(),
    })?;
    tx.call(game_property::Use {
        actor: person,
        inventory: request.weights,
        purpose: "testing a mechanism".into(),
    })?;
    let limits = limits(tx, vec![person])?
        .ok_or_else(|| Error::Rejected("mechanics session priority changed".into()))?;
    require(
        limits.hunger_below > 0 && limits.fatigue_below > 0,
        "invalid session thresholds",
    )?;
    let area = ready(tx, person)?;
    let device = tx
        .get::<Machines>()?
        .devices
        .get(&request.device)
        .ok_or_else(|| Error::Missing("apparatus".into()))?;
    require(
        device.inventory == apparatus
            && device.area == area
            && !busy(tx, request.device)
            && request.outlet < device.parts.len()
            && device.parts[request.outlet].linear()
            && !device.parts.iter().any(|p| p.input == Some(request.outlet)),
        "apparatus inaccessible or occupied",
    )?;
    require(
        request.load > 0
            && game_cargo::locality(tx, request.weights)? == area
            && tx.get::<game_cargo::Cargo>()?.0[&request.weights].mass >= request.load,
        "test needs a physical load",
    )?;
    let body = tx.get::<Bodies>()?.people[&person];
    require(
        !body.resting && body.hunger < limits.hunger_below && body.fatigue < limits.fatigue_below,
        "session physical priority changed",
    )?;
    let since = tx.now();
    let until = since.after(SimDuration(4 * HOUR.0))?;
    let effort = source(tx, person);
    tx.start_action(person, "experiment", None, Cancel(person))?;
    let action = tx.get::<Actions>()?.0[&person].id;
    tx.edit::<Machines>()?.work.insert(
        person,
        Work {
            device: Some(request.device),
            task: Task {
                force: request.load,
                travel: 1,
            },
            recipe: None,
            source: effort,
            since,
            unit: HOUR,
            weights: Some(request.weights),
            outlet: request.outlet,
            session: Some(Session {
                action,
                until,
                health: body.health,
                trial: 0,
                limits,
            }),
        },
    );
    game_diagnostics::interaction_profile!(count "activity.mechanics.started", 1);
    schedule(tx, vec![Row { person, action }])
}
fn schedule(tx: &mut Transaction<'_>, rows: Vec<Row>) -> Result<SimTime> {
    let mut at = tx.now().after(HOUR)?;
    let bodies = tx.get::<Bodies>()?;
    for row in &rows {
        let session = tx.get::<Machines>()?.work[&row.person]
            .session
            .as_ref()
            .unwrap();
        let body = &bodies.people[&row.person];
        at = at.min(session.until);
        if let Some(due) =
            game_physiology::hunger_threshold(body, &bodies.rules, session.limits.hunger_below)
                .filter(|due| *due > tx.now())
        {
            at = at.min(due);
        }
        let rate =
            u128::from(bodies.rules.fatigue_per_tick) * u128::from(body.metabolic_rates.fatigue);
        if rate > 0 && body.fatigue < session.limits.fatigue_below {
            let need = u128::from(session.limits.fatigue_below - body.fatigue) * 1000
                - u128::from(body.fatigue_fraction);
            let duration = need
                .div_ceil(rate)
                .saturating_mul(u128::from(bodies.rules.tick.0));
            let due = SimTime(
                body.last
                    .0
                    .saturating_add(duration.min(u128::from(u64::MAX)) as u64),
            );
            if due > tx.now() {
                at = at.min(due);
            }
        }
    }
    if !rows.is_empty() {
        tx.schedule(at, 21, Epoch(rows))?;
    }
    Ok(at)
}
fn finish_rows(tx: &mut Transaction<'_>, rows: &[Row], finish: Finish) -> Result<()> {
    for row in rows {
        if live(tx, row) {
            finish(tx, row.person)?;
            if tx
                .get::<Actions>()?
                .0
                .get(&row.person)
                .is_some_and(|a| a.id == row.action)
            {
                tx.finish_action(row.person, "experiment")?;
            }
        }
    }
    Ok(())
}
fn advance(tx: &mut Transaction<'_>, mut rows: Vec<Row>, finish: Finish) -> Result<()> {
    rows.retain(|r| live(tx, r));
    rows.sort_by_key(|r| r.person);
    require(
        rows.windows(2).all(|p| p[0].person < p[1].person),
        "duplicate mechanics epoch row",
    )?;
    if rows.is_empty() {
        return Ok(());
    }
    if tx.attempt(TrialWave(rows.clone())).is_err() {
        game_diagnostics::interaction_profile!(count "activity.mechanics.fallback", rows.len() as u64);
        finish_rows(tx, &rows, finish)?;
        return Ok(());
    }
    // Completion observers can invalidate retained peers; each pass removes at
    // least one actor and rereads physical conditions after all owner callbacks.
    loop {
        rows.retain(|r| live(tx, r));
        if rows.is_empty() {
            return Ok(());
        }
        let allowed = limits(tx, rows.iter().map(|r| r.person).collect())?.is_some();
        let (retained, stopped): (Vec<_>, Vec<_>) = rows.into_iter().partition(|r| {
            allowed
                && physical(tx, r)
                && tx.now()
                    < tx.get::<Machines>().unwrap().work[&r.person]
                        .session
                        .as_ref()
                        .unwrap()
                        .until
        });
        if stopped.is_empty() {
            schedule(tx, retained)?;
            return Ok(());
        }
        finish_rows(tx, &stopped, finish)?;
        rows = retained;
    }
}
pub fn register(b: &mut Builder, finish: Finish) -> Result<()> {
    b.request::<Cancel>(move |tx, r| {
        finish(tx, r.0)?;
        Ok(())
    })?;
    b.request::<InterruptSession>(|tx, r| {
        if session_active(tx, r.0) {
            tx.cancel_action(r.0)?;
        }
        Ok(())
    })?;
    b.request::<TrialWave>(|tx, r| wave::apply(tx, r.0))?;
    b.request::<Epoch>(move |tx, r| advance(tx, r.0, finish))?;
    b.scheduled_wave::<Epoch>(128, move |tx, requests| {
        advance(
            tx,
            requests.iter().flat_map(|r| r.0.clone()).collect(),
            finish,
        )
    })
}
