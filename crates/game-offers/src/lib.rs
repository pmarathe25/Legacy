//! Proposal state and ordered collection; callers keep policy and execution.
mod planning;
use game_core::*;
use game_sim::*;
pub use game_utility::Candidate;
pub use planning::{defer, planning, with_provider};
use serde::{Deserialize, Serialize};
mod collection;
use collection::Collection;
pub const LIMIT: usize = 256;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub key: String,
    pub motive: Candidate,
    pub request: Pending,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<u64>,
}
#[inline]
pub fn identity(key: &str) -> u64 {
    game_core::fingerprint(key)
}
impl Proposal {
    pub fn new<R: Request>(
        key: impl Into<String>,
        mut motive: Candidate,
        request: R,
    ) -> Result<Self> {
        let key = key.into();
        require(!key.is_empty() && key.len() <= 160, "invalid proposal key")?;
        motive.id = identity(&key);
        Ok(Self {
            key,
            motive,
            request: Pending::immediate(request),
            continuation: None,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offers {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub goal_execution: bool,
    #[serde(skip)]
    planning: bool,
    #[serde(skip)]
    fanout: bool,
    #[serde(skip)]
    effects: Vec<Pending>,
    #[serde(skip)]
    source: Option<String>,
    #[serde(skip)]
    sources: std::collections::BTreeMap<String, String>,
    #[serde(skip)]
    active: Option<Collection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<std::sync::Arc<FrameState>>,
    #[serde(skip)]
    pub frame_reused: u64,
    #[serde(skip)]
    pub frame_missed: u64,
    #[serde(skip)]
    pub frame_conflicts: u64,
    #[serde(skip)]
    draft_provider: Option<String>,
    #[serde(skip)]
    draft_groups: std::collections::BTreeMap<String, Vec<Proposal>>,
}
impl Resource for Offers {
    const KEY: &'static str = "choice.offers";
    fn validate(&self) -> Result<()> {
        require(self.active.is_none(), "unfinished choice collection")?;
        require(
            self.frame.as_ref().is_none_or(|f| {
                f.interval.0 > 0
                    && f.interval <= DAY
                    && f.tuning.minimum_cohort <= 100_000
                    && f.tuning.commit_batch <= 128
                    && (f.tuning.commit_batch == 0 || f.tuning.batch_decisions)
            }),
            "invalid decision frame",
        )
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Review {
    pub person: PersonId,
}
impl Event for Review {
    const KEY: &'static str = "characters.idle-review";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Changed {
    pub person: PersonId,
}
impl Event for Changed {
    const KEY: &'static str = "choice.changed";
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Collect {
    pub person: PersonId,
}
impl Request for Collect {
    const KEY: &'static str = "choice.collect";
    type Response = Vec<Proposal>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Offer {
    pub person: PersonId,
    pub proposal: Proposal,
}
impl Request for Offer {
    const KEY: &'static str = "choice.offer";
    type Response = ();
}
/// Owners can retain observed failure evidence while asking the chooser to try another offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "choice")]
pub enum Outcome {
    Performed,
    Reconsider,
}
impl From<bool> for Outcome {
    fn from(performed: bool) -> Self {
        if performed {
            Self::Performed
        } else {
            Self::Reconsider
        }
    }
}
pub fn collecting(tx: &Transaction<'_>, person: PersonId) -> bool {
    tx.get::<Offers>()
        .is_ok_and(|o| o.active.as_ref().is_some_and(|c| c.person == person))
}
pub fn submit(tx: &mut Transaction<'_>, person: PersonId, proposal: Proposal) -> Result<()> {
    tx.call(Offer { person, proposal })
}
pub fn offer<R: Request>(
    tx: &mut Transaction<'_>,
    person: PersonId,
    key: impl Into<String>,
    motive: Candidate,
    request: R,
) -> Result<bool> {
    let Ok(state) = tx.get::<Offers>() else {
        return Ok(false);
    };
    let Some(collection) = state.active.as_ref().filter(|c| c.person == person) else {
        return Ok(false);
    };
    let key = key.into();
    if !collection.heap.is_empty() && !state.planning && !tx.has_before::<Offer>() {
        let weights = tx
            .get::<game_traits::Traits>()
            .ok()
            .map(|t| t.weights(person));
        if collection.rejects(&key, motive, weights) {
            require(!key.is_empty() && key.len() <= 160, "invalid proposal key")?;
            return Ok(true);
        }
    }
    tx.call(Offer {
        person,
        proposal: Proposal::new(key, motive, request)?,
    })?;
    Ok(true)
}
/// Keep query errors deferred until the caller has checked the resulting actor state.
pub fn collect_with<T: Send>(
    tx: &mut Transaction<'_>,
    person: PersonId,
    query: impl Fn(&Transaction<'_>) -> Result<T> + Sync,
) -> Result<(Vec<Proposal>, Result<T>)> {
    // Providers may observe or update the world. Finish their ordered effects
    // before preparing food (or another caller-owned query) on that world.
    Ok((tx.call(Collect { person })?, query(tx)))
}
pub fn motive(need: i64, goal: i64, value: i64, effort: i64, risk: i64) -> Candidate {
    Candidate {
        id: 0,
        need,
        goal,
        value,
        effort,
        risk,
        survival: false,
    }
}
pub fn reweight(tx: &Transaction<'_>, person: PersonId, offers: &mut [Proposal]) {
    let weights = tx
        .get::<game_traits::Traits>()
        .ok()
        .map(|t| t.weights(person));
    for o in offers {
        o.motive.reweight(|term| {
            weights
                .as_ref()
                .and_then(|w| w.get(term))
                .copied()
                .unwrap_or(100)
        });
    }
}
pub fn register(b: &mut Builder) -> Result<()> {
    planning::register(b)?;
    b.request::<Collect>(|tx, r| {
        require(
            tx.get::<Offers>()?.active.is_none(),
            "recursive choice collection",
        )?;
        let ongoing = tx.get::<Actions>()?.0.get(&r.person).map(|a| a.id);
        tx.edit::<Offers>()?.active = Some(Collection {
            person: r.person,
            capture: false,
            weights: None,
            heap: Vec::new(),
            ongoing,
            items: Vec::new(),
            identities: Default::default(),
        });
        let state = tx.get::<Offers>()?;
        if state.planning && !state.fanout || state.frame.is_some() || tx.has_before::<Offer>() {
            tx.notify(Review { person: r.person })?;
        } else {
            let capture = tx.get::<Offers>()?.clone();
            tx.notify_prepared(
                Review { person: r.person },
                || {
                    let mut o = capture.clone();
                    let active = o.active.as_mut().unwrap();
                    active.capture = true;
                    active.items.clear();
                    o.effects.clear();
                    o.sources.clear();
                    o
                },
                |tx, o: Offers| {
                    for proposal in o.active.unwrap().items {
                        tx.call(Offer {
                            person: r.person,
                            proposal,
                        })?;
                    }
                    if planning(tx) {
                        let state = tx.edit::<Offers>()?;
                        state.effects.extend(o.effects);
                        state.sources.extend(o.sources);
                    }
                    Ok(())
                },
            )?;
        }
        Ok(tx.edit::<Offers>()?.active.take().unwrap().finish())
    })?;
    b.request::<Offer>(|tx, r| {
        let pruning = tx
            .get::<Offers>()?
            .active
            .as_ref()
            .is_some_and(|c| !c.capture && c.items.len() >= LIMIT);
        let weights = if pruning {
            tx.get::<game_traits::Traits>()
                .ok()
                .map(|t| t.weights(r.person))
        } else {
            None
        };
        let state = tx.edit::<Offers>()?;
        if state.planning
            && let Some(source) = &state.source
        {
            state.sources.insert(r.proposal.key.clone(), source.clone());
        }
        let collection = state
            .active
            .as_mut()
            .ok_or_else(|| Error::Rejected("no choice collection".into()))?;
        require(collection.person == r.person, "offer for another person")?;
        collection.insert(r.proposal, weights)
    })
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<FrameClock>,
    pub interval: SimDuration,
    pub until: SimTime,
    pub people: std::collections::BTreeSet<PersonId>,
    #[serde(default)]
    pub actions: std::collections::BTreeMap<PersonId, Option<u64>>,
    pub disabled: std::collections::BTreeSet<String>,
    pub providers: std::collections::BTreeSet<String>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub auxiliary: std::collections::BTreeSet<String>,
    #[serde(default, skip_serializing_if = "FrameTuning::ordinary")]
    pub tuning: FrameTuning,
    pub proposals:
        std::collections::BTreeMap<PersonId, std::collections::BTreeMap<String, Vec<Proposal>>>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub decisions: std::collections::BTreeMap<PersonId, Pending>,
    pub prepared: u64,
    pub effectful: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameClock {
    pub pending: Option<(SimTime, ScheduleId)>,
    pub ticks: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTuning {
    pub recover_idle: bool,
    pub recheck_conflicts: bool,
    pub batch_food: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub batch_decisions: bool,
    #[serde(default, skip_serializing_if = "zero_cohort")]
    pub minimum_cohort: usize,
    #[serde(default, skip_serializing_if = "zero_cohort")]
    pub commit_batch: usize,
}
fn zero_cohort(value: &usize) -> bool {
    *value == 0
}
impl FrameTuning {
    pub fn ordinary(&self) -> bool {
        *self == Self::default()
    }
}
impl Offers {
    pub fn scratch(person: PersonId, ongoing: Option<u64>) -> Self {
        Self {
            active: Some(Collection {
                person,
                capture: true,
                weights: None,
                heap: Vec::new(),
                ongoing,
                items: vec![],
                identities: Default::default(),
            }),
            ..Default::default()
        }
    }
    fn flush_draft(&mut self) {
        if let Some(name) = self.draft_provider.take() {
            let items = std::mem::take(&mut self.active.as_mut().expect("draft collection").items);
            self.draft_groups.insert(name, items);
        }
    }
    pub fn switch_draft(&mut self, name: String) -> Result<()> {
        require(
            self.active.as_ref().is_some_and(|c| c.capture),
            "not a draft collection",
        )?;
        self.flush_draft();
        self.draft_provider = Some(name);
        Ok(())
    }
    pub fn draft_groups(
        mut self,
        accepted: bool,
    ) -> (
        std::collections::BTreeMap<String, Vec<Proposal>>,
        Option<String>,
    ) {
        if accepted {
            self.flush_draft();
        }
        (self.draft_groups, self.draft_provider)
    }
}
pub fn observed(tx: &Transaction<'_>, person: PersonId) -> bool {
    tx.get::<Offers>()
        .ok()
        .and_then(|o| o.frame.as_ref())
        .is_some_and(|f| tx.now() < f.until && f.people.contains(&person))
}
