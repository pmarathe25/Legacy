//! Prepared actor choices retain their exact regional facts until batch admission.
use crate::*;
use game_core::*;
use game_ecs::{Batch, Component, ReadFootprint, StableId, Store};
use std::marker::PhantomData;
#[derive(Clone, Debug)]
pub struct PreparedProposal<A: Component> {
    change: PreparedChange,
    actor: PersonId,
    proposal: Proposal,
    facts: ProposalFacts,
    now: SimTime,
    area: AreaId,
    witnesses: Vec<PersonId>,
    reads: ReadFootprint,
    marker: PhantomData<A>,
}
impl<A: Component> PreparedProposal<A> {
    pub(crate) fn new(
        store: &Store,
        change: PreparedChange,
        actor: PersonId,
        proposal: Proposal,
        facts: ProposalFacts,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<Self, Error> {
        let area = actor_area(context.presence, actor).ok_or(Error::Unavailable)?;
        let witnesses = positioned_people(context.presence, area)
            .filter(|p| actor_area(context.presence, *p) == Some(area))
            .collect();
        let reads = Runtime::default().proposal_reads::<A>(store, actor, &proposal, context);
        Ok(Self {
            change,
            actor,
            proposal,
            facts,
            now: context.now,
            area,
            witnesses,
            reads,
            marker: PhantomData,
        })
    }
    pub fn revision_number(&self) -> Option<u32> {
        self.change.revision_number()
    }
    pub fn check(
        &self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<(), Error> {
        if context.now != self.now
            || actor_area(context.presence, self.actor) != Some(self.area)
            || runtime.proposal_facts::<A>(store, self.actor, &self.proposal, true, context)?
                != self.facts
            || positioned_people(context.presence, self.area)
                .filter(|p| actor_area(context.presence, *p) == Some(self.area))
                .collect::<Vec<_>>()
                != self.witnesses
        {
            return Err(Error::Unavailable);
        }
        self.change.check(store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
        batch: &mut Batch,
    ) -> Result<RealmBatchGuard, Error> {
        self.check(runtime, store, context)?;
        let mut guard = self.change.stage_into(store, batch)?;
        guard.protect_reads(self.reads);
        guard.validate(batch)?;
        Ok(guard)
    }
}
#[derive(Clone, Debug)]
pub struct PreparedWork<A: Component> {
    request: PreparedWorkRequest,
    facts: ProposalFacts,
    proposal: Proposal,
    now: SimTime,
    area: AreaId,
    reuse: Option<game_agreements::AgreementView>,
    reads: ReadFootprint,
    marker: PhantomData<A>,
}
impl<A: Component> PreparedWork<A> {
    pub(crate) fn new(
        store: &Store,
        request: PreparedWorkRequest,
        facts: ProposalFacts,
        proposal: Proposal,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<Self, Error> {
        let area = actor_area(context.presence, request.actor).ok_or(Error::Unavailable)?;
        let reuse = if let Some(PendingOwner::Agreement(id)) = request.existing {
            game_agreements::known_agreement(store, request.actor, id).cloned()
        } else {
            None
        };
        let mut reads =
            Runtime::default().proposal_reads::<A>(store, request.actor, &proposal, context);
        if matches!(request.request, Request::Delivery { .. }) {
            reads.protect::<game_agreements::Knowledge>(StableId(request.actor.0));
        }
        if let Some(PendingOwner::Agreement(id)) = request.existing {
            reads.protect::<game_agreements::Agreement>(StableId(id.0));
        }
        Ok(Self {
            request,
            facts,
            proposal,
            now: context.now,
            area,
            reuse,
            reads,
            marker: PhantomData,
        })
    }
    pub fn request(&self) -> &Request {
        &self.request.request
    }
    pub fn existing(&self) -> Option<&PendingOwner> {
        self.request.existing.as_ref()
    }
    pub fn bind(self, owner: PendingOwner) -> Result<PreparedBoundWork<A>, Error> {
        let actor = self.request.actor;
        let key = self.request.key;
        let request = self.request.request.clone();
        let existing = self.request.existing.clone();
        Ok(PreparedBoundWork {
            change: self.request.bind(owner)?,
            actor,
            key,
            request,
            existing,
            facts: self.facts,
            proposal: self.proposal,
            now: self.now,
            area: self.area,
            reuse: self.reuse,
            reads: self.reads,
            marker: PhantomData,
        })
    }
}
#[derive(Clone, Debug)]
pub struct PreparedBoundWork<A: Component> {
    change: PreparedChange,
    actor: PersonId,
    key: TermKey,
    request: Request,
    existing: Option<PendingOwner>,
    facts: ProposalFacts,
    proposal: Proposal,
    now: SimTime,
    area: AreaId,
    reuse: Option<game_agreements::AgreementView>,
    reads: ReadFootprint,
    marker: PhantomData<A>,
}
impl<A: Component> PreparedBoundWork<A> {
    pub fn check(
        &self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<(), Error> {
        if context.now != self.now
            || actor_area(context.presence, self.actor) != Some(self.area)
            || runtime.proposal_facts::<A>(store, self.actor, &self.proposal, true, context)?
                != self.facts
        {
            return Err(Error::Unavailable);
        }
        let current =
            Ledger::new(store).prepare_request(self.actor, self.key, &self.facts, self.now)?;
        if current.request != self.request
            || current.existing != self.existing
            || self.reuse.as_ref().is_some_and(|before| {
                game_agreements::known_agreement(store, self.actor, before.id) != Some(before)
            })
        {
            return Err(Error::Unavailable);
        }
        self.change.check(store)
    }
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
        batch: &mut Batch,
    ) -> Result<RealmBatchGuard, Error> {
        self.check(runtime, store, context)?;
        let mut guard = self.change.stage_into(store, batch)?;
        guard.protect_reads(self.reads);
        guard.validate(batch)?;
        Ok(guard)
    }
    pub fn stage_with_agreement_publication(
        self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
        source: &game_agreements::ProposalPublicationFinalization,
        batch: &mut Batch,
    ) -> Result<RealmBatchGuard, Error> {
        self.check(runtime, store, context)?;
        if self.existing.is_some() || !matches!(self.request, Request::Delivery { .. }) {
            return Err(Error::Unavailable);
        }
        let PendingOwner::Agreement(id) = self
            .change
            .after
            .as_ref()
            .and_then(|c| c.revision(self.key.revision))
            .and_then(|r| r.pending.get(&self.key.index))
            .ok_or(Error::Unavailable)?
            .owner
        else {
            return Err(Error::Unavailable);
        };
        let Request::Delivery { parties, term, .. } = self.request else {
            return Err(Error::Unavailable);
        };
        let mut guard = self.change.stage_into(store, batch)?;
        guard.protect_agreement_reads(
            self.reads,
            source,
            batch,
            (id, self.actor),
            &parties,
            &term,
        )?;
        guard.validate(batch)?;
        Ok(guard)
    }
}

#[derive(Clone, Debug)]
pub struct PreparedWorkReaction<A: Component> {
    admission: PreparedBoundWork<A>,
    reaction: PreparedInstitutionReaction,
}
impl<A: Component> PreparedBoundWork<A> {
    pub fn receive_publications(
        self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
        publications: &[game_society::projected_publication::PreparedCommandPublication],
    ) -> Result<PreparedWorkReaction<A>, Error> {
        self.check(runtime, store, context)?;
        let views = game_society::projected_publication::combined_personal_views(
            store,
            self.actor,
            publications,
        );
        if runtime.proposal_facts_with_views::<A>(
            store,
            self.actor,
            &self.proposal,
            true,
            context,
            &views,
        )? != self.facts
        {
            return Err(Error::Unavailable);
        }
        let reaction = runtime.prepare_institution_reaction(
            store,
            context.now,
            publications,
            vec![self.change.clone()],
            true,
            context.presence,
        )?;
        Ok(PreparedWorkReaction {
            admission: self,
            reaction,
        })
    }
}
impl<A: Component> PreparedWorkReaction<A> {
    pub fn stage_into(
        self,
        runtime: &Runtime,
        store: &Store,
        context: &RealmContext<'_, impl Presence>,
        source: &game_society::projected_publication::CommandPublicationGuard,
        batch: &mut Batch,
    ) -> Result<RealmBatchGuard, Error> {
        self.admission.check(runtime, store, context)?;
        let mut guard = self.reaction.stage_into(store, source, batch)?;
        guard.protect_projected_reads(self.admission.reads, source, batch)?;
        guard.validate(batch)?;
        Ok(guard)
    }
}
