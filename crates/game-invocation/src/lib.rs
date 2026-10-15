//! Active past-life access, scoped consent, and binding control-loss intervals.
mod costs;
pub use costs::{Cost, CostAt, apply};
use game_core::*;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Active {
    pub person: PersonId,
    pub lives: BTreeSet<LifeId>,
    pub began: SimTime,
    pub step: usize,
    pub job: ScheduleId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub explanation: String,
    pub cost: Cost,
    pub amount: u64,
    pub duration: SimDuration,
    pub step: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invocation {
    pub costs: Vec<CostAt>,
    pub repeat: SimDuration,
    pub active: Option<Active>,
    pub warning: Option<Warning>,
    pub control_until: Option<SimTime>,
    pub identity: u64,
    pub instability: u64,
    pub lifespan_lost: SimDuration,
}
impl Resource for Invocation {
    const KEY: &'static str = "invocation.session";
    fn validate(&self) -> Result<()> {
        require(
            self.costs.windows(2).all(|s| s[0].after < s[1].after)
                && self.costs.iter().all(|s| s.after.0 > 0 && s.amount > 0),
            "invalid invocation costs",
        )?;
        require(
            self.active.as_ref().is_none_or(|a| !a.lives.is_empty()),
            "no selected lives",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configure {
    pub costs: Vec<CostAt>,
    pub repeat: SimDuration,
}
impl Request for Configure {
    const KEY: &'static str = "invocation.configure";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Invoke {
    pub person: PersonId,
    pub lives: BTreeSet<LifeId>,
}
impl Request for Invoke {
    const KEY: &'static str = "invocation.invoke";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Expose;
impl Request for Expose {
    const KEY: &'static str = "invocation.expose";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Consent {
    pub step: usize,
    pub accept: bool,
}
impl Request for Consent {
    const KEY: &'static str = "invocation.consent";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Dismiss;
impl Request for Dismiss {
    const KEY: &'static str = "invocation.dismiss";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ControlEnd {
    pub until: SimTime,
}
impl Request for ControlEnd {
    const KEY: &'static str = "invocation.control_end";
    type Response = ();
}
pub fn controlled(state: &Invocation, now: SimTime) -> bool {
    state.control_until.is_none_or(|until| until <= now)
}
fn schedule(tx: &mut Transaction<'_>) -> Result<()> {
    let s = tx.get::<Invocation>()?;
    let a = s.active.as_ref().unwrap();
    let index = a.step.min(s.costs.len() - 1);
    let cycles = a.step.saturating_sub(s.costs.len() - 1) as u64;
    let elapsed = s.costs[index]
        .after
        .0
        .checked_add(s.repeat.0.checked_mul(cycles).ok_or(Error::Overflow)?)
        .ok_or(Error::Overflow)?
        / (a.lives.len() as u64);
    let at = a.began.after(SimDuration(elapsed))?.max(tx.now());
    let job = tx.schedule(at, 45, Expose)?;
    tx.edit::<Invocation>()?.active.as_mut().unwrap().job = job;
    Ok(())
}
fn pay_and_continue(tx: &mut Transaction<'_>, w: Warning) -> Result<()> {
    apply(tx, &w)?;
    let current = tx.get::<game_lineage::Lineage>()?.current;
    if tx
        .get::<Invocation>()?
        .active
        .as_ref()
        .is_some_and(|a| Some(a.person) == current)
    {
        let s = tx.edit::<Invocation>()?;
        s.warning = None;
        s.active.as_mut().unwrap().step += 1;
        schedule(tx)?;
    } else {
        tx.edit::<Invocation>()?.active = None;
        tx.edit::<Invocation>()?.warning = None;
    }
    Ok(())
}
pub fn memories(host: &Host) -> Result<BTreeMap<String, game_lineage::Life>> {
    let s = host.get::<Invocation>()?;
    let history = &host.get::<game_lineage::Lineage>()?.history;
    Ok(s.active
        .as_ref()
        .map(|a| {
            a.lives
                .iter()
                .map(|id| (id.0.to_string(), history[id].clone()))
                .collect()
        })
        .unwrap_or_default())
}
pub fn load(store: &mut game_content::Store, key: &str) -> Result<Configure> {
    let doc: game_content::Document<Configure> = store.load(&format!("invocation/{key}.toml"))?;
    let check = Invocation {
        costs: doc.data.costs.clone(),
        repeat: doc.data.repeat,
        ..Default::default()
    };
    check.validate()?;
    require(
        check.repeat.0 > 0
            && !check.costs.is_empty()
            && check
                .costs
                .iter()
                .all(|c| c.cost != Cost::ControlLoss || c.duration.0 > 0),
        "invalid invocation profile",
    )?;
    Ok(doc.data)
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("invocation", 1)?;
    b.resource::<Invocation>()?;
    b.validate_save(|h| {
        if let Some(active) = &h.get::<Invocation>()?.active {
            require(
                active.lives.iter().all(|id| {
                    h.get::<game_lineage::Lineage>()
                        .unwrap()
                        .history
                        .contains_key(id)
                }),
                "invocation references unknown life",
            )?;
            require(
                h.get::<game_lineage::Lineage>()?.current == Some(active.person),
                "invocation belongs to wrong ruler",
            )?;
        }
        Ok(())
    });
    b.request::<game_conflict::HasPlayerControl>(|tx, r| {
        let s = tx.get::<Invocation>()?;
        Ok(s.active.as_ref().is_none_or(|a| a.person != r.person) || controlled(s, tx.now()))
    })?;
    b.guard(|h, _| require(controlled(h.get()?, h.now()), "player control is suspended"));
    b.stop(|h| {
        Ok(h.get::<Invocation>()?
            .warning
            .as_ref()
            .map(|w| format!("invocation:{}", w.step)))
    });
    b.request::<Configure>(|tx, r| {
        require(
            tx.get::<Invocation>()?.active.is_none(),
            "invocation is active",
        )?;
        require(
            r.repeat.0 > 0 && !r.costs.is_empty(),
            "cost rules are required",
        )?;
        let s = tx.edit::<Invocation>()?;
        s.costs = r.costs;
        s.repeat = r.repeat;
        Ok(())
    })?;
    b.request::<Invoke>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let lineage = tx.get::<game_lineage::Lineage>()?;
        require(
            lineage.current == Some(r.person),
            "only the current ruler invokes",
        )?;
        require(
            !r.lives.is_empty() && r.lives.iter().all(|id| lineage.history.contains_key(id)),
            "unknown past life",
        )?;
        let now = tx.now();
        let s = tx.edit::<Invocation>()?;
        require(
            s.active.is_none() && !s.costs.is_empty(),
            "invocation unavailable",
        )?;
        s.active = Some(Active {
            person: r.person,
            lives: r.lives,
            began: now,
            step: 0,
            job: ScheduleId(0),
        });
        schedule(tx)
    })?;
    b.request::<Expose>(|tx, _| {
        let s = tx.get::<Invocation>()?;
        let a = s
            .active
            .as_ref()
            .ok_or_else(|| Error::Rejected("no invocation".into()))?;
        let cost = &s.costs[a.step.min(s.costs.len() - 1)];
        let amount = cost
            .amount
            .checked_mul(a.lives.len() as u64)
            .and_then(|q| q.checked_mul(1 + a.step.saturating_sub(s.costs.len() - 1) as u64))
            .ok_or(Error::Overflow)?;
        let explanation=if cost.cost==Cost::ControlLoss {format!("Accepting suspends player control for {} ms. Autonomous actions and subsequent costs proceed during that interval. Dismissal is unavailable until control returns.",cost.duration.0)}else{format!("Accept {:?}, or dismiss to prevent this new cost.",cost.cost)};
        let w = Warning {
            explanation,
            cost: cost.cost.clone(),
            amount,
            duration: cost.duration,
            step: a.step,
        };
        if controlled(s, tx.now()) {
            tx.edit::<Invocation>()?.warning = Some(w);
            Ok(())
        } else {
            pay_and_continue(tx, w)
        }
    })?;
    b.request::<Consent>(|tx, r| {
        let warning = tx
            .get::<Invocation>()?
            .warning
            .clone()
            .ok_or_else(|| Error::Rejected("no pending warning".into()))?;
        require(warning.step == r.step, "stale warning")?;
        if r.accept {
            pay_and_continue(tx, warning)
        } else {
            tx.call(Dismiss)
        }
    })?;
    b.request::<Dismiss>(|tx, _| {
        require(
            controlled(tx.get()?, tx.now()),
            "cannot dismiss during control loss",
        )?;
        if let Some(a) = tx.edit::<Invocation>()?.active.take() {
            tx.unschedule(a.job);
        }
        tx.edit::<Invocation>()?.warning = None;
        tx.edit::<Invocation>()?.control_until = None;
        Ok(())
    })?;
    b.request::<ControlEnd>(|tx, r| {
        if tx.get::<Invocation>()?.control_until == Some(r.until) {
            tx.edit::<Invocation>()?.control_until = None;
        }
        Ok(())
    })?;
    b.subscribe::<game_physiology::Died>("invocation.end_life", |tx, e| {
        if tx
            .get::<Invocation>()?
            .active
            .as_ref()
            .is_some_and(|a| a.person == e.person)
        {
            let a = tx.edit::<Invocation>()?.active.take().unwrap();
            tx.unschedule(a.job);
            tx.edit::<Invocation>()?.warning = None;
            tx.edit::<Invocation>()?.control_until = None;
        }
        Ok(())
    })
}
