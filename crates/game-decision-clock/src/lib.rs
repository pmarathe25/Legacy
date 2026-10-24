//! Choice-owned frame scheduling; demand mode sleeps between useful deadlines.
use game_core::*;
use game_offers::{FrameClock, Offers};
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize)]
struct Tick;
impl Request for Tick {
    const KEY: &'static str = "choice.frame";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Demand(pub bool);
impl Request for Demand {
    const KEY: &'static str = "choice.frame-demand";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Wake(SimTime);
impl Request for Wake {
    const KEY: &'static str = "choice.wake-frame";
    type Response = ();
}
pub fn demand(tx: &Transaction<'_>) -> bool {
    tx.get::<Offers>()
        .ok()
        .and_then(|o| o.frame.as_ref())
        .is_some_and(|f| f.clock.is_some())
}
/// The next boundary retains the phase of the preceding frame, even after sleep.
pub fn deadline(tx: &Transaction<'_>, at: SimTime) -> Result<Option<SimTime>> {
    let Some(f) = tx.get::<Offers>()?.frame.as_ref() else {
        return Ok(None);
    };
    if f.clock.is_none() {
        return Ok(Some(f.until.max(at)));
    }
    let steps = at.0.saturating_sub(f.until.0).div_ceil(f.interval.0);
    let offset = steps.checked_mul(f.interval.0).ok_or(Error::Overflow)?;
    Ok(Some(f.until.after(SimDuration(offset))?))
}
/// Receiver-owned request, safe for another feature's wake or deadline forecast.
pub fn wake(tx: &mut Transaction<'_>, at: SimTime) -> Result<()> {
    if demand(tx) {
        tx.call(Wake(at))?;
    }
    Ok(())
}
pub fn cancel(tx: &mut Transaction<'_>) -> Result<()> {
    tx.cancel_scheduled::<Tick>()
}
/// Only choice-owned configuration and clock handlers call this directly.
pub fn schedule(tx: &mut Transaction<'_>, at: SimTime) -> Result<()> {
    let id = tx.schedule(at, 49, Tick)?;
    if let Some(clock) = Arc::make_mut(tx.edit::<Offers>()?.frame.as_mut().unwrap())
        .clock
        .as_mut()
    {
        clock.pending = Some((at, id));
    }
    Ok(())
}
pub fn register(
    b: &mut Builder,
    tick: impl Fn(&mut Transaction<'_>) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    b.validate_save(|host| {
        let Some(f) = host.get::<Offers>()?.frame.as_ref() else {
            return Ok(());
        };
        if let Some(clock) = &f.clock {
            require(
                f.interval.0 > 0 && f.interval <= DAY,
                "invalid frame interval",
            )?;
            let mut ticks = host.scheduled().filter(|j| j.request.key() == Tick::KEY);
            let valid = match (clock.pending, ticks.next()) {
                (None, None) => true,
                (Some((at, id)), Some(job)) => {
                    at >= host.now() && (job.at, job.id, job.priority) == (at, id, 49)
                }
                _ => false,
            };
            require(valid && ticks.next().is_none(), "invalid pending frame")?;
        }
        Ok(())
    });
    b.request::<Demand>(|tx, r| {
        require(tx.get::<Offers>()?.frame.is_some(), "frames disabled")?;
        if demand(tx) == r.0 {
            return Ok(());
        }
        cancel(tx)?;
        let now = tx.now();
        let f = Arc::make_mut(tx.edit::<Offers>()?.frame.as_mut().unwrap());
        f.clock = r.0.then(FrameClock::default);
        if r.0 {
            f.until = now;
        }
        // Start at this instant, preserving a well-defined phase on reconfiguration.
        schedule(tx, tx.now())
    })?;
    b.request::<Wake>(|tx, r| {
        if !demand(tx) {
            return Ok(());
        }
        let at = deadline(tx, r.0.max(tx.now()))?.unwrap();
        let pending = tx
            .get::<Offers>()?
            .frame
            .as_ref()
            .unwrap()
            .clock
            .as_ref()
            .unwrap()
            .pending;
        if pending.is_some_and(|(old, _)| old <= at) {
            return Ok(());
        }
        if let Some((_, id)) = pending {
            tx.unschedule(id);
        }
        schedule(tx, at)
    })?;
    b.request::<Tick>(move |tx, _| {
        if tx.get::<Offers>()?.frame.is_none() {
            return Ok(());
        }
        if demand(tx) {
            let clock = Arc::make_mut(tx.edit::<Offers>()?.frame.as_mut().unwrap())
                .clock
                .as_mut()
                .unwrap();
            clock.pending = None;
            clock.ticks += 1;
        }
        tick(tx)?;
        if !demand(tx) {
            let at = tx.get::<Offers>()?.frame.as_ref().unwrap().until;
            schedule(tx, at)?;
        }
        Ok(())
    })
}
