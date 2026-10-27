//! Cohort preparation with explicitly delayed proposal visibility.
use game_core::*;
pub use game_decision_clock::{Demand as ConfigureDemand, demand, wake};
use game_offers::*;
use game_sim::*;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Serialize, Deserialize)]
pub struct Configure {
    pub interval: Option<SimDuration>,
}
impl Request for Configure {
    const KEY: &'static str = "choice.configure-frames";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Tune(pub FrameTuning);
impl Request for Tune {
    const KEY: &'static str = "choice.tune-frames";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Recheck {
    person: PersonId,
    proposal: Proposal,
    #[serde(default)]
    provider: Option<String>,
}
impl Request for Recheck {
    const KEY: &'static str = "choice.recheck-frame";
    type Response = bool;
}
#[derive(Serialize, Deserialize)]
pub struct AuxiliaryReview {
    pub person: PersonId,
}
impl Event for AuxiliaryReview {
    const KEY: &'static str = "choice.frame-auxiliary";
}
#[derive(Serialize, Deserialize)]
pub struct Gather {
    pub until: SimTime,
}
impl Event for Gather {
    const KEY: &'static str = "choice.gather-frame";
}
#[derive(Serialize, Deserialize)]
pub struct PlanCohort {
    pub people: Vec<PersonId>,
}
impl Event for PlanCohort {
    const KEY: &'static str = "choice.plan-cohort";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct StoreDecisions(pub Vec<(PersonId, Option<Pending>)>);
impl Request for StoreDecisions {
    const KEY: &'static str = "choice.store-decisions";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Batch {
    pub people: Vec<PersonId>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct InvalidatePerson(pub PersonId);
impl Request for InvalidatePerson {
    const KEY: &'static str = "choice.invalidate-frame-person";
    type Response = ();
}
impl Request for Batch {
    const KEY: &'static str = "choice.prepare-frame";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Prepare;
impl Event for Prepare {
    const KEY: &'static str = "choice.frame-inputs";
}
#[derive(Clone, Serialize, Deserialize)]
struct RegisterProvider(String, bool);
impl Request for RegisterProvider {
    const KEY: &'static str = "choice.frame-provider";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct SwitchProvider(String);
impl Request for SwitchProvider {
    const KEY: &'static str = "choice.switch-draft-provider";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Store {
    provider: String,
    rows: Vec<(PersonId, Option<Vec<Proposal>>)>,
}
impl Request for Store {
    const KEY: &'static str = "choice.store-frame";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
struct Fetch {
    provider: String,
    person: PersonId,
}
impl Request for Fetch {
    const KEY: &'static str = "choice.fetch-frame";
    type Response = Option<Vec<Proposal>>;
}
pub fn register(b: &mut Builder) -> Result<()> {
    b.request::<InvalidatePerson>(|tx, r| {
        if let Some(frame) = tx.edit::<Offers>()?.frame.as_mut() {
            let frame = Arc::make_mut(frame);
            frame.people.remove(&r.0);
            frame.actions.remove(&r.0);
            frame.proposals.remove(&r.0);
            frame.decisions.remove(&r.0);
        }
        Ok(())
    })?;
    b.request::<StoreDecisions>(|tx, r| {
        let f = tx
            .edit::<Offers>()?
            .frame
            .as_mut()
            .ok_or_else(|| Error::Rejected("frames disabled".into()))?;
        let f = Arc::make_mut(f);
        for (person, decision) in r.0 {
            if let Some(decision) = decision {
                f.decisions.insert(person, decision);
                f.prepared += 1;
            } else {
                f.effectful += 1;
            }
        }
        Ok(())
    })?;
    b.request::<Recheck>(|tx, r| {
        let stale = stale(tx, r.person, &r.proposal, r.provider.as_ref());
        if stale {
            tx.edit::<Offers>()?.frame_conflicts += 1;
        }
        Ok(stale)
    })?;
    b.request::<Tune>(|tx, r| {
        require(r.0.minimum_cohort <= 100_000, "invalid minimum cohort")?;
        require(r.0.commit_batch <= 128, "commit batch must be 0..=128")?;
        require(
            r.0.commit_batch == 0 || r.0.batch_decisions,
            "grouped commits require whole-actor frames",
        )?;
        require(
            !r.0.meal_arbitration || r.0.commit_batch > 0,
            "meal arbitration requires grouped commits",
        )?;
        require(r.0.valid_experiments(), "invalid frame experiments")?;
        let f = tx
            .edit::<Offers>()?
            .frame
            .as_mut()
            .ok_or_else(|| Error::Rejected("frames disabled".into()))?;
        let f = Arc::make_mut(f);
        f.tuning = r.0;
        f.providers.clear();
        f.auxiliary.clear();
        f.decisions.clear();
        Ok(())
    })?;
    b.request::<Configure>(|tx, r| {
        require(
            r.interval.is_none_or(|i| i.0 > 0 && i <= DAY),
            "invalid decision frame",
        )?;
        if tx.get::<Offers>()?.frame.as_ref().map(|f| f.interval) == r.interval {
            return Ok(());
        }
        game_decision_clock::cancel(tx)?;
        let clock = demand(tx).then(FrameClock::default);
        let until = if clock.is_some() {
            tx.now()
        } else {
            SimTime(0)
        };
        tx.edit::<Offers>()?.frame = r.interval.map(|interval| {
            Arc::new(FrameState {
                meal_round: 0,
                clock,
                interval,
                until,
                people: Default::default(),
                actions: Default::default(),
                proposals: BTreeMap::new(),
                decisions: BTreeMap::new(),
                disabled: Default::default(),
                providers: Default::default(),
                auxiliary: Default::default(),
                tuning: Default::default(),
                prepared: 0,
                effectful: 0,
            })
        });
        if r.interval.is_some() {
            game_decision_clock::schedule(tx, tx.now())?;
        }
        Ok(())
    })?;
    game_decision_clock::register(b, |tx| {
        let Some(interval) = tx.get::<Offers>()?.frame.as_ref().map(|f| f.interval) else {
            return Ok(());
        };
        let until = tx.now().after(interval)?;
        let old = tx.get::<Offers>()?.frame.as_ref().unwrap();
        let next = FrameState {
            meal_round: old.meal_round,
            clock: old.clock.clone(),
            interval,
            until,
            people: Default::default(),
            actions: Default::default(),
            proposals: BTreeMap::new(),
            decisions: BTreeMap::new(),
            disabled: old.disabled.clone(),
            providers: old.providers.clone(),
            auxiliary: old.auxiliary.clone(),
            tuning: old.tuning,
            prepared: old.prepared,
            effectful: old.effectful,
        };
        tx.edit::<Offers>()?.frame = Some(Arc::new(next));
        tx.notify(Gather { until })?;
        Ok(())
    })?;
    b.request::<Batch>(|tx, mut r| {
        require(r.people.len() <= 100_000, "frame cohort too large")?;
        r.people.sort_unstable();
        r.people.dedup();
        let actions = tx.get::<Actions>()?;
        let actors: Vec<_> = r
            .people
            .iter()
            .map(|&p| (p, actions.0.get(&p).map(|a| a.id)))
            .collect();
        let f = tx
            .edit::<Offers>()?
            .frame
            .as_mut()
            .ok_or_else(|| Error::Rejected("frames disabled".into()))?;
        let f = Arc::make_mut(f);
        f.people.extend(&r.people);
        f.actions.extend(actors.iter().copied());
        if r.people.is_empty() {
            return Ok(());
        }
        if f.tuning.batch_decisions {
            return tx.notify(PlanCohort { people: r.people });
        }
        if tx
            .get::<Offers>()?
            .frame
            .as_ref()
            .unwrap()
            .providers
            .is_empty()
        {
            tx.notify(Prepare)?;
        }
        let f = tx.get::<Offers>()?.frame.as_ref().unwrap();
        let (auxiliary, providers): (Vec<_>, Vec<_>) = f
            .providers
            .iter()
            .filter(|name| !f.disabled.contains(*name))
            .cloned()
            .partition(|name| f.auxiliary.contains(name));
        if providers.is_empty() && auxiliary.is_empty() {
            return Ok(());
        }
        let grain = if tx.compute().current_num_threads() == 1 {
            providers.len().max(1)
        } else {
            (actors.len() * providers.len())
                .div_ceil(tx.compute().current_num_threads() * 2)
                .clamp(1, providers.len().max(1))
        };
        let tasks: Vec<_> = actors
            .iter()
            .flat_map(|&(person, ongoing)| {
                providers
                    .chunks(grain)
                    .chain(auxiliary.iter().map(std::slice::from_ref))
                    .map(move |names| (person, ongoing, names))
            })
            .collect();
        let results: Vec<_> = tx.compute().install(|| {
            tasks
                .par_iter()
                .map(|&(person, ongoing, names)| {
                    (
                        person,
                        prepare_chunk(tx, person, ongoing, names, &f.auxiliary),
                    )
                })
                .collect()
        });
        let mut grouped = BTreeMap::<String, Vec<_>>::new();
        for (person, rows) in results {
            for (name, proposals) in rows {
                grouped.entry(name).or_default().push((person, proposals));
            }
        }
        for (provider, rows) in grouped {
            tx.call(Store { provider, rows })?;
        }
        Ok(())
    })?;
    b.request::<SwitchProvider>(|tx, r| tx.edit::<Offers>()?.switch_draft(r.0))?;
    b.request::<RegisterProvider>(|tx, r| {
        let f = tx
            .edit::<Offers>()?
            .frame
            .as_mut()
            .ok_or_else(|| Error::Rejected("frames disabled".into()))?;
        let f = Arc::make_mut(f);
        if r.1 {
            f.auxiliary.insert(r.0.clone());
        }
        f.providers.insert(r.0);
        Ok(())
    })?;
    b.request::<Store>(|tx, r| {
        let f = tx
            .edit::<Offers>()?
            .frame
            .as_mut()
            .ok_or_else(|| Error::Rejected("frames disabled".into()))?;
        let f = Arc::make_mut(f);
        if r.rows.iter().any(|(_, p)| p.is_none()) {
            f.disabled.insert(r.provider);
            f.effectful += r.rows.len() as u64;
            return Ok(());
        }
        for (person, proposals) in r.rows {
            if let Some(proposals) = proposals {
                f.prepared += 1;
                f.proposals
                    .entry(person)
                    .or_default()
                    .insert(r.provider.clone(), proposals);
            } else {
                f.effectful += 1;
            }
        }
        Ok(())
    })?;
    b.request::<Fetch>(|tx, r| {
        let now = tx.now();
        let offers = tx.edit::<Offers>()?;
        let Some(f) = offers.frame.as_ref().filter(|f| now < f.until) else {
            return Ok(None);
        };
        let proposals = f
            .proposals
            .get(&r.person)
            .and_then(|p| p.get(&r.provider))
            .cloned();
        if proposals.is_some() {
            offers.frame_reused += 1;
        } else {
            offers.frame_missed += 1;
        }
        Ok(proposals)
    })
}
pub fn provider(
    b: &mut Builder,
    name: &'static str,
    listener: impl Fn(&mut Transaction<'_>, &Review) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    b.subscribe_isolated::<Review, Offers>(name, move |tx, e| {
        if !fast_review(tx)
            && tx
                .get::<Offers>()
                .is_ok_and(|offers| offers.frame.is_some())
            && let Some(proposals) = tx.call(Fetch {
                provider: name.into(),
                person: e.person,
            })?
        {
            for proposal in proposals {
                submit(tx, e.person, proposal)?;
            }
            return Ok(());
        }
        with_provider(tx, Some(name), |tx| listener(tx, e))
    })?;
    b.subscribe::<Prepare>(name, move |tx, _| {
        tx.call(RegisterProvider(name.into(), false))
    })
}

/// Read-only auxiliary inputs share the same actor/provider pool submission.
pub fn auxiliary(
    b: &mut Builder,
    name: &'static str,
    enabled: fn(&Transaction<'_>) -> bool,
    listener: impl Fn(&mut Transaction<'_>, &AuxiliaryReview) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    b.subscribe_isolated::<AuxiliaryReview, Offers>(name, listener)?;
    b.subscribe::<Prepare>(name, move |tx, _| {
        if enabled(tx) {
            tx.call(RegisterProvider(name.into(), true))
        } else {
            Ok(())
        }
    })
}
pub fn tuning(tx: &Transaction<'_>) -> FrameTuning {
    tx.get::<Offers>()
        .ok()
        .and_then(|o| o.frame.as_ref())
        .map_or(FrameTuning::default(), |f| f.tuning)
}
pub fn payload<R: Request>(tx: &Transaction<'_>, person: PersonId, provider: &str) -> Option<R> {
    let f = tx.get::<Offers>().ok()?.frame.as_ref()?;
    let row = f.proposals.get(&person)?.get(provider)?.first()?;
    if tx.now() >= f.until {
        return None;
    }
    decode_pending(&row.request)
}
/// Inspect a request without executing it, including unbound payloads restored
/// inside saved frame decisions. A mismatched key or invalid payload returns None.
pub fn decode_pending<R: Request>(pending: &Pending) -> Option<R> {
    if pending.key() != R::KEY {
        return None;
    }
    pending.request::<R>().cloned().or_else(|| {
        let envelope: Envelope =
            serde_json::from_value(serde_json::to_value(pending).ok()?).ok()?;
        serde_json::from_value(envelope.payload).ok()
    })
}
pub fn decision<R: Request>(tx: &Transaction<'_>, person: PersonId) -> Option<R> {
    let f = tx.get::<Offers>().ok()?.frame.as_ref()?;
    if !f.tuning.batch_decisions || tx.now() >= f.until {
        return None;
    }
    decode_pending(f.decisions.get(&person)?)
}
/// Rejected stale opportunities are arbitration conflicts, not personal failures.
fn stale(
    tx: &Transaction<'_>,
    person: PersonId,
    proposal: &Proposal,
    provider: Option<&String>,
) -> bool {
    let Some(f) = tx.get::<Offers>().ok().and_then(|o| o.frame.as_ref()) else {
        return false;
    };
    if !f.tuning.recheck_conflicts || tx.now() >= f.until {
        return false;
    }
    let name = provider.or_else(|| {
        f.proposals.get(&person).and_then(|groups| {
            groups
                .iter()
                .find(|(name, rows)| {
                    !f.auxiliary.contains(*name)
                        && rows
                            .iter()
                            .any(|p| p.key == proposal.key && p.request == proposal.request)
                })
                .map(|(name, _)| name)
        })
    });
    let Some(name) = name else {
        return false;
    };
    let ongoing = tx
        .get::<Actions>()
        .ok()
        .and_then(|a| a.0.get(&person))
        .map(|a| a.id);
    let rows = prepare_chunk(
        tx,
        person,
        ongoing,
        std::slice::from_ref(name),
        &f.auxiliary,
    );
    rows.get(name).and_then(|r| r.as_ref()).is_some_and(|rows| {
        !rows
            .iter()
            .any(|p| p.key == proposal.key && p.request == proposal.request)
    })
}
pub fn recheck(tx: &mut Transaction<'_>, person: PersonId, proposal: &Proposal) -> Result<bool> {
    recheck_provider(tx, person, proposal, None)
}
pub fn recheck_provider(
    tx: &mut Transaction<'_>,
    person: PersonId,
    proposal: &Proposal,
    provider: Option<String>,
) -> Result<bool> {
    if !tuning(tx).recheck_conflicts {
        return Ok(false);
    }
    if provider.is_none()
        && tx
            .get::<Offers>()?
            .frame
            .as_ref()
            .is_none_or(|f| !f.proposals.contains_key(&person))
    {
        return Ok(false);
    }
    tx.call(Recheck {
        person,
        proposal: proposal.clone(),
        provider,
    })
}

pub fn deadline(tx: &Transaction<'_>) -> Option<SimTime> {
    game_decision_clock::deadline(tx, tx.now()).ok().flatten()
}
pub fn start(tx: &Transaction<'_>) -> Option<SimTime> {
    tx.get::<Offers>()
        .ok()?
        .frame
        .as_ref()
        .map(|f| SimTime(f.until.0.saturating_sub(f.interval.0)))
}

/// A resolution involved this actor after its proposals were prepared.
/// Deferring its obsolete turn preserves the newly started joint activity.
pub fn action_changed(tx: &Transaction<'_>, person: PersonId) -> bool {
    let Some(frame) = tx.get::<Offers>().ok().and_then(|o| o.frame.as_ref()) else {
        return false;
    };
    let Some(&expected) = frame.actions.get(&person) else {
        return false;
    };
    tx.get::<Actions>()
        .ok()
        .and_then(|a| a.0.get(&person))
        .is_some_and(|a| Some(a.id) != expected)
}

fn prepare_chunk(
    tx: &Transaction<'_>,
    person: PersonId,
    ongoing: Option<u64>,
    mut names: &[String],
    auxiliary: &std::collections::BTreeSet<String>,
) -> BTreeMap<String, Option<Vec<Proposal>>> {
    let mut rows = BTreeMap::new();
    while !names.is_empty() {
        let draft = tx.draft(Offers::scratch(person, ongoing), |child| {
            for name in names {
                child.call(SwitchProvider(name.clone()))?;
                if auxiliary.contains(name) {
                    child.listener(&AuxiliaryReview { person }, name)?;
                } else {
                    child.listener(&Review { person }, name)?;
                }
            }
            Ok(())
        });
        let (complete, failed) = draft.state.draft_groups(draft.accepted);
        rows.extend(complete.into_iter().map(|(name, p)| (name, Some(p))));
        if draft.accepted {
            break;
        }
        if let Some(index) = failed
            .as_ref()
            .and_then(|name| names.iter().position(|n| n == name))
        {
            rows.insert(names[index].clone(), None);
            names = &names[index + 1..];
        } else {
            rows.extend(
                names
                    .iter()
                    .filter(|name| !rows.contains_key(*name))
                    .cloned()
                    .map(|name| (name, None))
                    .collect::<Vec<_>>(),
            );
            break;
        }
    }
    rows
}
