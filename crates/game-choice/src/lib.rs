//! Feature offers are gathered before a character chooses an executable owner request.
mod keys;
mod templates;
pub use keys::Keys;
pub use templates::Templates;
mod agenda;
mod undertaking;
pub use agenda::{Next, Schedule};
use game_core::*;
use game_sim::*;
pub use game_utility::Candidate;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
pub use undertaking::{Resolved, Undertake};
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
    #[serde(skip)]
    active: Option<Collection>,
}
type Rank = (i64, Reverse<u64>);
#[derive(Clone, Debug, PartialEq, Eq)]
struct Collection {
    person: PersonId,
    capture: bool,
    weights: Option<game_traits::Weights>,
    ranked: bool,
    ongoing: Option<u64>,
    items: Vec<(Proposal, Rank)>,
    identities: rustc_hash::FxHashMap<u64, usize>,
}
impl Collection {
    fn insert(&mut self, proposal: Proposal, weights: Option<game_traits::Weights>) -> Result<()> {
        if self.capture {
            self.items.push((proposal, (0, Reverse(0))));
            return Ok(());
        }
        let key_hash = identity(&proposal.key);
        require(
            !self.identities.contains_key(&key_hash)
                || self.items.iter().all(|(p, _)| p.key != proposal.key),
            "duplicate proposal key",
        )?;
        let rank = |p: &Proposal| {
            let mut c = p.motive;
            c.reweight(|term| {
                weights
                    .as_ref()
                    .and_then(|w| w.get(term))
                    .copied()
                    .unwrap_or(100)
            });
            (c.score(), Reverse(c.id))
        };
        let score = if self.items.len() >= LIMIT {
            if !self.ranked || self.weights != weights {
                for (p, score) in &mut self.items {
                    *score = rank(p);
                }
                self.weights = weights;
                self.ranked = true;
                for i in (0..self.items.len() / 2).rev() {
                    self.sift_down(i);
                }
            }
            rank(&proposal)
        } else {
            (0, Reverse(0))
        };
        *self.identities.entry(key_hash).or_default() += 1;
        self.items.push((proposal, score));
        if self.items.len() > LIMIT {
            let mut i = self.items.len() - 1;
            while i > 0 && self.less(i, (i - 1) / 2) {
                self.items.swap(i, (i - 1) / 2);
                i = (i - 1) / 2;
            }
            let (removed, _) = self.items.swap_remove(0);
            self.sift_down(0);
            let removed_hash = identity(&removed.key);
            let count = self.identities.get_mut(&removed_hash).unwrap();
            *count -= 1;
            if *count == 0 {
                self.identities.remove(&removed_hash);
            }
        }
        Ok(())
    }
    fn less(&self, a: usize, b: usize) -> bool {
        let priority = |i: usize| {
            let (p, rank) = &self.items[i];
            (
                self.ongoing.is_some_and(|id| p.continuation == Some(id)),
                *rank,
                &p.key,
            )
        };
        priority(a) < priority(b)
    }
    fn sift_down(&mut self, mut i: usize) {
        loop {
            let mut child = 2 * i + 1;
            if child >= self.items.len() {
                break;
            }
            if child + 1 < self.items.len() && self.less(child + 1, child) {
                child += 1;
            }
            if !self.less(child, i) {
                break;
            }
            self.items.swap(i, child);
            i = child;
        }
    }
}
impl Resource for Offers {
    const KEY: &'static str = "choice.offers";
    fn validate(&self) -> Result<()> {
        require(self.active.is_none(), "unfinished choice collection")
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
struct Offer {
    person: PersonId,
    proposal: Proposal,
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
/// Inert fallback; the historical wire key remains readable in saved requests.
#[derive(Clone, Serialize, Deserialize)]
pub struct Wait;
impl Request for Wait {
    const KEY: &'static str = "characters.wait";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Run(pub Pending);
impl Request for Run {
    const KEY: &'static str = "choice.run";
    type Response = Outcome;
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
    if !collecting(tx, person) {
        return Ok(false);
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
    tx.prepare::<Collect, Offers, _>(Collect { person }, |view| Ok(query(view)))
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
/// Profile proposal providers without coupling the kernel to gameplay diagnostics.
pub fn provider(
    b: &mut Builder,
    name: &'static str,
    listener: impl Fn(&mut Transaction<'_>, &Review) -> Result<()> + Send + Sync + 'static,
) -> Result<()> {
    b.subscribe_isolated::<Review, Offers>(name, move |tx, e| {
        let _span = game_diagnostics::span(name);
        listener(tx, e)
    })
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    game_identity::plugin(b)?;
    b.plugin("choice", 1)?;
    b.resource::<Offers>()?;
    undertaking::register(b)?;
    b.request::<Wait>(|_, _| Ok(()))?;
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
            ranked: false,
            ongoing,
            items: Vec::new(),
            identities: Default::default(),
        });
        if tx.has_before::<Offer>() {
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
                    o
                },
                |tx, o: Offers| {
                    for (proposal, _) in o.active.unwrap().items {
                        tx.call(Offer {
                            person: r.person,
                            proposal,
                        })?;
                    }
                    Ok(())
                },
            )?;
        }
        let mut collection = tx.edit::<Offers>()?.active.take().unwrap();
        collection
            .items
            .sort_unstable_by(|(a, _), (b, _)| a.key.cmp(&b.key));
        Ok(collection.items.into_iter().map(|(p, _)| p).collect())
    })?;
    b.request::<Offer>(|tx, r| {
        let weights = tx
            .get::<game_traits::Traits>()
            .ok()
            .map(|t| t.weights(r.person));
        let collection = tx
            .edit::<Offers>()?
            .active
            .as_mut()
            .ok_or_else(|| Error::Rejected("no choice collection".into()))?;
        require(collection.person == r.person, "offer for another person")?;
        collection.insert(r.proposal, weights)
    })?;
    b.request::<Run>(|tx, r| {
        let response = tx.invoke(r.0)?;
        require(
            response != serde_json::Value::Bool(false),
            "opportunity no longer available",
        )?;
        Ok(serde_json::from_value(response).unwrap_or(Outcome::Performed))
    })
}
