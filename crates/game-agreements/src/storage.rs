//! Promises and personal memory are native rows in the host's shared store.
use crate::*;
use game_ecs::{Schema, StableId, Store};
use std::{marker::PhantomData, ops::Index};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub last_event: u64,
    pub last_observation_event: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub agreements: BTreeMap<AgreementId, AgreementView>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_search: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingWork {
    pub millis: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingReport {
    pub sender: PersonId,
    pub recipient: PersonId,
    pub started_at: SimTime,
    pub completes: SimTime,
    pub snapshot: AgreementView,
}
fn invalid() -> Error {
    Error::InvalidConfiguration("invalid agreement component".into())
}
impl Agreement {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.proposer.0 == 0
            || !(2..=8).contains(&self.parties.len())
            || self.parties.iter().any(|id| id.0 == 0)
            || !self.parties.contains(&self.proposer)
            || !self.accepted.is_subset(&self.parties)
            || !self.accepted.contains(&self.proposer)
            || self.terms.is_empty()
            || self.terms.len() > 16
            || self.terms.iter().any(|term| {
                let [a, b] = term.parties();
                a == b || !self.parties.contains(&a) || !self.parties.contains(&b)
            })
            || self.obligations.iter().any(|id| id.0 == 0)
            || self
                .obligations
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
                .len()
                != self.obligations.len()
        {
            return Err(invalid());
        }
        if let Some(p) = &self.procedure
            && (p.eligible_proposers.is_empty()
                || !p.eligible_proposers.is_subset(&self.parties)
                || !p.approvers.is_subset(&self.parties)
                || p.threshold == 0
                || p.threshold > p.approvers.len()
                || p.term_approvers.iter().any(|(i, ids)| {
                    usize::from(*i) >= self.terms.len()
                        || ids.is_empty()
                        || !ids.is_subset(&self.parties)
                }))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Obligation {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.agreement.0 == 0
            || self.performed > self.amount()
            || self.amount() == 0 && self.defaulted
            || self.cargo.values().any(|q| *q == 0)
            || self
                .cargo
                .values()
                .try_fold(0_u64, |sum, q| sum.checked_add(*q))
                .is_none_or(|sum| sum > self.amount() - self.performed)
            || match self.term {
                AgreementTerm::OutputShare { denominator, .. } => {
                    denominator == 0
                        || self.share_remainder
                            >= if self.share_scale == 0 {
                                denominator
                            } else {
                                self.share_scale
                            }
                }
                _ => self.accrued != 0 || self.share_remainder != 0 || self.share_scale != 0,
            }
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Amendment {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.agreement.0 == 0
            || self.proposer.0 == 0
            || self.replacements.is_empty()
            || self.replacements.len() > 16
            || self.votes.keys().any(|id| id.0 == 0)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Knowledge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.agreements.iter().any(|(id, view)| {
            id.0 == 0
                || *id != view.id
                || view.observed_at > view.learned_at
                || view
                    .obligations
                    .iter()
                    .any(|o| o.observed_at > o.learned_at || o.performed > o.amount())
        }) {
            return Err(invalid());
        }
        Ok(())
    }
}
impl PendingReport {
    pub fn validate(&self) -> Result<(), Error> {
        if self.sender.0 == 0
            || self.recipient.0 == 0
            || self.sender == self.recipient
            || self.started_at >= self.completes
            || self.snapshot.learned_at > self.started_at
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Agreement>("agreements/agreement", Agreement::validate)?;
    schema.register::<Obligation>("agreements/obligation", Obligation::validate)?;
    schema.register::<Amendment>("agreements/amendment", Amendment::validate)?;
    schema.register::<Knowledge>("agreements/knowledge", Knowledge::validate)?;
    schema.register::<Requests>("agreements/requests", |_| Ok(()))?;
    schema.register::<PendingWork>("agreements/pending_work", |row| {
        if row.millis == 0 {
            Err(invalid())
        } else {
            Ok(())
        }
    })?;
    schema.register::<PendingReport>("agreements/report", PendingReport::validate)
}
macro_rules! allocated {
    ($ty:ty,$id:ident,$get:ident,$mut:ident,$all:ident,$insert:ident) => {
        pub fn $get(store: &Store, id: $id) -> Option<&$ty> {
            store.get(StableId(id.0))
        }
        pub fn $mut(store: &mut Store, id: $id) -> Option<&mut $ty> {
            store.get_mut(StableId(id.0))
        }
        pub fn $all(store: &Store) -> impl Iterator<Item = ($id, &$ty)> {
            store.iter::<$ty>().map(|(id, row)| ($id(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: $id, row: $ty) -> Result<(), Error> {
            if id != row.id {
                return Err(invalid());
            }
            store.insert_or_spawn(StableId(id.0), row)
        }
    };
}
allocated!(
    Agreement,
    AgreementId,
    agreement,
    agreement_mut,
    agreements,
    insert_agreement
);
allocated!(
    Obligation,
    ObligationId,
    obligation,
    obligation_mut,
    obligations,
    insert_obligation
);
allocated!(
    Amendment,
    AmendmentId,
    amendment,
    amendment_mut,
    amendments,
    insert_amendment
);
macro_rules! personal {
    ($ty:ty,$get:ident,$mut:ident,$all:ident,$insert:ident) => {
        pub fn $get(store: &Store, id: PersonId) -> Option<&$ty> {
            store.get(StableId(id.0))
        }
        pub fn $mut(store: &mut Store, id: PersonId) -> Option<&mut $ty> {
            store.get_mut(StableId(id.0))
        }
        pub fn $all(store: &Store) -> impl Iterator<Item = (PersonId, &$ty)> {
            store.iter::<$ty>().map(|(id, row)| (PersonId(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: PersonId, row: $ty) -> Result<(), Error> {
            store.insert_or_spawn(StableId(id.0), row)
        }
    };
}
personal!(
    Knowledge,
    knowledge,
    knowledge_mut,
    knowledges,
    insert_knowledge
);
personal!(
    Requests,
    requests,
    requests_mut,
    request_rows,
    insert_requests
);
pub fn pending_work(store: &Store, id: ObligationId) -> Option<&u64> {
    store
        .get::<PendingWork>(StableId(id.0))
        .map(|row| &row.millis)
}
pub fn pending_works(store: &Store) -> impl Iterator<Item = (ObligationId, &u64)> {
    store
        .iter::<PendingWork>()
        .map(|(id, row)| (ObligationId(id.0), &row.millis))
}
pub fn report(store: &Store, id: PersonId) -> Option<&PendingReport> {
    store.get(StableId(id.0))
}
pub fn reports(store: &Store) -> impl Iterator<Item = (PersonId, &PendingReport)> {
    store
        .iter::<PendingReport>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_report(store: &mut Store, id: PersonId, row: PendingReport) -> Result<(), Error> {
    if row.sender != id {
        return Err(invalid());
    }
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn remove_report(store: &mut Store, id: PersonId) -> Result<Option<PendingReport>, Error> {
    store.remove(StableId(id.0))
}
pub fn known_agreements(
    store: &Store,
    id: PersonId,
) -> Option<&BTreeMap<AgreementId, AgreementView>> {
    knowledge(store, id).map(|row| &row.agreements)
}

/// Borrowed ledger queries. This holds no shadow rows and does not own a store.
pub struct Ledger<'a> {
    pub(crate) store: &'a Store,
}
impl<'a> Ledger<'a> {
    pub fn new(store: &'a Store) -> Self {
        Self { store }
    }
    pub fn agreement(&self, id: AgreementId) -> Option<&'a Agreement> {
        agreement(self.store, id)
    }
    pub fn obligation(&self, id: ObligationId) -> Option<&'a Obligation> {
        obligation(self.store, id)
    }

    pub(crate) fn agreements(&self) -> Rows<'a, AgreementId, Agreement> {
        Rows::new(self.store, AgreementId, |id: AgreementId| id.0)
    }
    pub(crate) fn obligations(&self) -> Rows<'a, ObligationId, Obligation> {
        Rows::new(self.store, ObligationId, |id: ObligationId| id.0)
    }
    pub(crate) fn amendments(&self) -> Rows<'a, AmendmentId, Amendment> {
        Rows::new(self.store, AmendmentId, |id: AmendmentId| id.0)
    }
    pub(crate) fn pending_work(&self) -> PendingRows<'a> {
        PendingRows { store: self.store }
    }
    pub(crate) fn knowledge(&self) -> KnowledgeRows<'a> {
        KnowledgeRows { store: self.store }
    }
    pub(crate) fn next_search(&self) -> RequestRows<'a> {
        RequestRows { store: self.store }
    }
}
pub(crate) struct Rows<'a, K, C> {
    store: &'a Store,
    key: fn(u64) -> K,
    id: fn(K) -> u64,
    marker: PhantomData<C>,
}
impl<'a, K: Copy + 'static, C: game_ecs::Component> Rows<'a, K, C> {
    fn new(store: &'a Store, key: fn(u64) -> K, id: fn(K) -> u64) -> Self {
        Self {
            store,
            key,
            id,
            marker: PhantomData,
        }
    }
    pub fn get(&self, id: &K) -> Option<&'a C> {
        self.store.get(StableId((self.id)(*id)))
    }
    pub fn contains_key(&self, id: &K) -> bool {
        self.get(id).is_some()
    }
    pub fn values(&self) -> impl Iterator<Item = &'a C> + 'a {
        self.store.iter::<C>().map(|(_, row)| row)
    }
    pub fn iter(&self) -> impl Iterator<Item = (K, &'a C)> + 'a {
        let key = self.key;
        self.store
            .iter::<C>()
            .map(move |(id, row)| (key(id.0), row))
    }
    pub fn len(&self) -> usize {
        self.store.count::<C>()
    }
}
impl<K: Copy + 'static, C: game_ecs::Component> Index<&K> for Rows<'_, K, C> {
    type Output = C;
    fn index(&self, id: &K) -> &C {
        self.get(id).expect("agreement row exists")
    }
}
pub(crate) struct PendingRows<'a> {
    store: &'a Store,
}
impl<'a> PendingRows<'a> {
    pub fn get(&self, id: &ObligationId) -> Option<&'a u64> {
        pending_work(self.store, *id)
    }
    pub fn iter(&self) -> impl Iterator<Item = (ObligationId, &'a u64)> + 'a {
        pending_works(self.store)
    }
    pub fn keys(&self) -> impl Iterator<Item = ObligationId> + 'a {
        self.iter().map(|(id, _)| id)
    }
}
pub(crate) struct KnowledgeRows<'a> {
    store: &'a Store,
}
impl<'a> KnowledgeRows<'a> {
    pub fn get(&self, id: &PersonId) -> Option<&'a BTreeMap<AgreementId, AgreementView>> {
        known_agreements(self.store, *id)
    }
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (PersonId, &'a BTreeMap<AgreementId, AgreementView>)> + 'a {
        knowledges(self.store).map(|(id, row)| (id, &row.agreements))
    }
}
pub(crate) struct RequestRows<'a> {
    store: &'a Store,
}
impl<'a> RequestRows<'a> {
    pub fn get(&self, id: &PersonId) -> Option<&'a SimTime> {
        requests(self.store, *id).map(|row| &row.next_search)
    }
    pub fn keys(&self) -> impl Iterator<Item = PersonId> + 'a {
        request_rows(self.store).map(|(id, _)| id)
    }
}
pub fn refresh_status(store: &mut Store, id: AgreementId, now: SimTime) -> Result<(), Error> {
    let status = Ledger::new(store).status_for(id, now)?;
    agreement_mut(store, id).ok_or(Error::Unavailable)?.status = status;
    Ok(())
}
pub fn mark_default(store: &mut Store, id: ObligationId, now: SimTime) -> Result<bool, Error> {
    let row = obligation(store, id).ok_or(Error::Unavailable)?;
    if row.defaulted
        || row.performed >= row.amount()
        || !row.term.deadline().is_some_and(|due| due <= now)
    {
        return Ok(false);
    }
    let agreement = row.agreement;
    Ledger::new(store).status_for(agreement, now)?;
    obligation_mut(store, id).unwrap().defaulted = true;
    refresh_status(store, agreement, now)?;
    Ok(true)
}
