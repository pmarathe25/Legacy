//! Admission and immediate local publication belong to the regional owner.
use crate::state::KnowledgePatch;
use crate::*;
use game_core::*;
use game_ecs::{Component, Store};
use game_society::institutions::AuthorityFacts;
use std::collections::BTreeMap;
impl PreparedChange {
    pub fn commitment_id(&self) -> RealmCommitmentId {
        self.id
    }
    pub fn revision_number(&self) -> Option<u32> {
        self.after.as_ref().map(|c| c.current().number)
    }
    pub(crate) fn personal_patch(&mut self, store: &Store, actor: PersonId) -> &mut Knowledge {
        let index = if let Some(index) = self.knowledge.iter().position(|p| p.actor == actor) {
            index
        } else {
            let before = Ledger::new(store).view(actor, self.id).cloned();
            self.knowledge.push(KnowledgePatch {
                actor,
                before: before.clone(),
                after: before.unwrap_or_default(),
            });
            self.knowledge.len() - 1
        };
        &mut self.knowledge[index].after
    }
    pub fn publish_proposal(
        mut self,
        store: &Store,
        revision: u32,
        area: AreaId,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<Self, Error> {
        let commitment = self
            .after
            .as_ref()
            .or(self.before.as_ref())
            .ok_or(Error::Unavailable)?;
        let r = commitment.revision(revision).ok_or(Error::Unavailable)?;
        if !r
            .proposal
            .participants
            .values()
            .any(|p| p.settlement == area)
            || now < r.proposed_at
        {
            return Err(Error::Unavailable);
        }
        let statement = crate::state::dated(r.proposal.clone(), PersonId(0), now);
        for actor in
            positioned_people(presence, area).filter(|a| actor_area(presence, *a) == Some(area))
        {
            let mut statement = statement.clone();
            statement.source = KnowledgeSource::Observed(actor);
            let known = self.personal_patch(store, actor);
            if let Some(old) = known.revisions.get_mut(&revision) {
                if statement.observed_at >= old.proposal.observed_at {
                    old.proposal = statement;
                }
            } else {
                known.revisions.insert(
                    revision,
                    KnownRevision {
                        proposal: statement,
                        ratifications: BTreeMap::new(),
                        backed: BTreeMap::new(),
                    },
                );
            }
        }
        Ok(self)
    }
    pub fn publish_decision(
        mut self,
        store: &Store,
        revision: u32,
        institution: InstitutionId,
        area: AreaId,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<Self, Error> {
        self = self.publish_proposal(store, revision, area, now, presence)?;
        let decision = self
            .after
            .as_ref()
            .or(self.before.as_ref())
            .and_then(|c| c.revision(revision))
            .and_then(|r| r.ratifications.get(&institution))
            .cloned()
            .ok_or(Error::Unavailable)?;
        if decision.decided_at != now {
            return Err(Error::Unavailable);
        }
        for actor in
            positioned_people(presence, area).filter(|a| actor_area(presence, *a) == Some(area))
        {
            self.personal_patch(store, actor)
                .revisions
                .get_mut(&revision)
                .ok_or(Error::Unavailable)?
                .ratifications
                .insert(
                    institution,
                    Dated {
                        value: decision.clone(),
                        observed_at: decision.decided_at,
                        learned_at: now,
                        source: KnowledgeSource::Observed(actor),
                    },
                );
        }
        Ok(self)
    }
}
impl Runtime {
    pub fn prepare_proposal<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        id: RealmCommitmentId,
        proposal: &Proposal,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<PreparedProposal<A>, Error> {
        let (now, presence) = (context.now, context.presence);
        let area = actor_area(presence, actor).ok_or(Error::Unavailable)?;
        if proposal.proposer != actor {
            return Err(Error::Unavailable);
        }
        let facts = self.proposal_facts::<A>(store, actor, proposal, true, context)?;
        let change = Ledger::new(store)
            .prepare_propose(id, proposal.clone(), &facts, now)?
            .publish_proposal(store, 0, area, now, presence)?;
        PreparedProposal::new(store, change, actor, proposal.clone(), facts, context)
    }
    pub fn prepare_revision<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        id: RealmCommitmentId,
        proposal: &Proposal,
        context: &RealmContext<'_, impl Presence>,
    ) -> Result<PreparedProposal<A>, Error> {
        let (now, presence) = (context.now, context.presence);
        let area = actor_area(presence, actor).ok_or(Error::Unavailable)?;
        if proposal.proposer != actor {
            return Err(Error::Unavailable);
        }
        let facts = self.proposal_facts::<A>(store, actor, proposal, true, context)?;
        let change = Ledger::new(store).prepare_revise(id, proposal.clone(), &facts, now)?;
        let revision = change.revision_number().unwrap();
        let change = change.publish_proposal(store, revision, area, now, presence)?;
        PreparedProposal::new(store, change, actor, proposal.clone(), facts, context)
    }
    pub fn prepare_work<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        key: TermKey,
        now: SimTime,
        authority: &AuthorityFacts,
        presence: &impl Presence,
    ) -> Result<PreparedWork<A>, Error> {
        actor_area(presence, actor).ok_or(Error::Unavailable)?;
        let proposal = &commitment(store, key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?
            .proposal;
        let facts = self.proposal_facts::<A>(
            store,
            actor,
            proposal,
            true,
            &RealmContext {
                now,
                authority,
                presence,
            },
        )?;
        let request = Ledger::new(store).prepare_request(actor, key, &facts, now)?;
        PreparedWork::new(
            store,
            request,
            facts,
            proposal.clone(),
            &RealmContext {
                now,
                authority,
                presence,
            },
        )
    }
}

impl PreparedChange {
    /// A later receiving operation borrowed this plan's selected projected row.
    /// It preserves earlier edits and combines disjoint personal evidence slots.
    pub(crate) fn merge_later(mut self, later: PreparedChange) -> Result<Self, Error> {
        if self.id != later.id
            || self.before != later.before
            || later.projection.as_ref() != self.after.as_ref()
        {
            return Err(Error::Unavailable);
        }
        if later.after.is_some() {
            self.after = later.after;
        }
        for patch in later.knowledge {
            if let Some(prior) = self.knowledge.iter_mut().find(|p| p.actor == patch.actor) {
                if prior.before != patch.before {
                    return Err(Error::Unavailable);
                }
                merge_known(&mut prior.after, patch.after, prior.before.as_ref())?;
            } else {
                self.knowledge.push(patch);
            }
        }
        self.requests.extend(later.requests);
        self.reads.extend(later.reads);
        Ok(self)
    }
}
fn merge_known(
    first: &mut Knowledge,
    later: Knowledge,
    before: Option<&Knowledge>,
) -> Result<(), Error> {
    for (number, r) in later.revisions {
        let old = before.and_then(|k| k.revisions.get(&number));
        if let Some(prior) = first.revisions.get_mut(&number) {
            if old.is_none_or(|old| old.proposal != r.proposal) {
                if old.is_none_or(|old| old.proposal != prior.proposal)
                    && prior.proposal != r.proposal
                {
                    return Err(Error::Unavailable);
                }
                prior.proposal = r.proposal;
            }
            for (id, statement) in r.ratifications {
                let old = old.and_then(|r| r.ratifications.get(&id));
                if old == Some(&statement) {
                    continue;
                }
                if prior.ratifications.get(&id) != old
                    && prior.ratifications.get(&id) != Some(&statement)
                {
                    return Err(Error::Unavailable);
                }
                prior.ratifications.insert(id, statement);
            }
            for (id, statement) in r.backed {
                let old = old.and_then(|r| r.backed.get(&id));
                if old == Some(&statement) {
                    continue;
                }
                if prior.backed.get(&id) != old && prior.backed.get(&id) != Some(&statement) {
                    return Err(Error::Unavailable);
                }
                prior.backed.insert(id, statement);
            }
        } else {
            first.revisions.insert(number, r);
        }
    }
    Ok(())
}
