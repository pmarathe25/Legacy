use crate::*;
use game_agreements::AgreementTerm;
use game_core::*;
use game_missions::MissionObjective;
use std::collections::BTreeSet;

impl Ledger<'_> {
    pub fn validate(&self, refs: &References, now: SimTime) -> Result<(), Error> {
        let bad = || Error::InvalidSave("invalid regional commitments or personal reports".into());
        let mut ratifications = BTreeSet::new();
        let mut bindings = BTreeSet::new();
        for (id, c) in self.commitments.iter() {
            if id.0 == 0 || id != c.id || c.revisions.is_empty() {
                return Err(bad());
            }
            let institutions = c.revisions[0]
                .proposal
                .participants
                .keys()
                .copied()
                .collect::<BTreeSet<_>>();
            let mut prior = SimTime(0);
            for (number, r) in c.revisions.iter().enumerate() {
                if usize::try_from(r.number).ok() != Some(number)
                    || r.proposed_at < prior
                    || r.proposed_at > now
                    || r.proposal
                        .participants
                        .keys()
                        .copied()
                        .collect::<BTreeSet<_>>()
                        != institutions
                    || !proposal_refs(&r.proposal, refs, r.proposed_at)
                    || r.expired_at
                        .is_some_and(|at| at <= r.proposal.ratify_by || at > now)
                    || r.expired_at.is_some()
                        && (crate::state::fully_ratified(r)
                            || r.ratifications
                                .values()
                                .any(|d| d.outcome == Decision::Rejected))
                    || r.requested
                        .iter()
                        .any(|i| usize::from(*i) >= r.proposal.terms.len())
                {
                    return Err(bad());
                }
                prior = r.proposed_at;
                for (institution, proposal) in &r.ratification_proposals {
                    if !r.proposal.participants.contains_key(institution)
                        || !refs.proposals.contains(proposal)
                        || !ratifications.insert(*proposal)
                    {
                        return Err(bad());
                    }
                }
                for (institution, d) in &r.ratifications {
                    if r.ratification_proposals.get(institution) != Some(&d.proposal)
                        || d.decided_at < r.proposed_at
                        || d.decided_at > r.proposal.ratify_by
                        || d.decided_at > now
                    {
                        return Err(bad());
                    }
                }
                for (index, b) in &r.backed {
                    let term = r.proposal.terms.get(usize::from(*index)).ok_or_else(bad)?;
                    if !r.requested.contains(index)
                        || !crate::state::fully_ratified(r)
                        || !backing_refs(term, b, refs, now)
                    {
                        return Err(bad());
                    }
                    if let Backing::Delivery {
                        obligation: Some(obligation),
                        ..
                    } = b.backing
                        && !bindings.insert(obligation)
                    {
                        return Err(bad());
                    }
                }
            }
        }
        for (actor, row) in knowledges(self.store) {
            let known = &row.commitments;
            if !refs.people.contains(&actor) {
                return Err(bad());
            }
            for (id, k) in known {
                let actual = self.commitments.get(id).ok_or_else(bad)?;
                for (number, r) in &k.revisions {
                    let real = actual.revision(*number).ok_or_else(bad)?;
                    if r.proposal.value != real.proposal
                        || r.proposal.observed_at < real.proposed_at
                        || !dated_valid(&r.proposal, actor, refs, now)
                    {
                        return Err(bad());
                    }
                    for (institution, d) in &r.ratifications {
                        if real.ratifications.get(institution) != Some(&d.value)
                            || !dated_valid(d, actor, refs, now)
                            || d.observed_at != d.value.decided_at
                        {
                            return Err(bad());
                        }
                    }
                    for (index, b) in &r.backed {
                        let term = real
                            .proposal
                            .terms
                            .get(usize::from(*index))
                            .ok_or_else(bad)?;
                        if !dated_valid(b, actor, refs, now)
                            || b.observed_at != b.value.changed_at
                            || !backing_refs(term, &b.value, refs, now)
                            || real.backed.get(index).is_none_or(|current| {
                                !crate::state::compatible_backing(
                                    &b.value.backing,
                                    &current.backing,
                                ) || b.value.performed > current.performed
                            })
                        {
                            return Err(bad());
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn dated_valid<T>(d: &Dated<T>, actor: PersonId, refs: &References, now: SimTime) -> bool {
    d.observed_at <= d.learned_at
        && d.learned_at <= now
        && match d.source {
            KnowledgeSource::Observed(person) => person == actor,
            KnowledgeSource::Reported(person) => refs.people.contains(&person),
            KnowledgeSource::Recorded(record) => refs.records.contains(&record),
        }
}
fn delivery_refs(term: &AgreementTerm, refs: &References, after: SimTime) -> bool {
    matches!(term,AgreementTerm::Delivery {debtor,creditor,source,destination,quantity,deadline}
        if debtor!=creditor && refs.people.contains(debtor) && refs.people.contains(creditor)
        && source!=destination && refs.inventories.contains(source) && refs.inventories.contains(destination) && quantity.0>0 && *deadline>after)
}
fn proposal_refs(p: &Proposal, refs: &References, after: SimTime) -> bool {
    (2..=8).contains(&p.participants.len()) && (1..=16).contains(&p.terms.len()) && p.ratify_by>after
        && p.participants.values().any(|r|r.representative==p.proposer)
        && p.participants.iter().all(|(id,r)|*id==r.institution && refs.institutions.contains(id) && refs.areas.contains(&r.settlement) && refs.people.contains(&r.representative))
        && p.terms.iter().all(|t| {
            let Some(a)=p.participants.get(&t.provider()) else{return false};let Some(b)=p.participants.get(&t.beneficiary()) else{return false};
            if t.provider()==t.beneficiary() {return false}
            match t {
                Term::Delivery {term,..}=>delivery_refs(term,refs,p.ratify_by) && term.parties()==[a.representative,b.representative],
                Term::Passage {areas,until,..}=>!areas.is_empty() && areas.len()<=16 && areas.is_subset(&refs.areas) && *until>p.ratify_by,
                Term::Protection {patrol,..}=>{
                    let people=patrol.recruits.iter().copied().chain([patrol.leader]).collect::<BTreeSet<_>>();
                    matches!(&patrol.objective,MissionObjective::Patrol {areas,until} if !areas.is_empty() && areas.len()<=16 && areas.is_subset(&refs.areas) && *until>p.ratify_by && patrol.deadline>=*until)
                        && people.len()<=32 && people.is_subset(&refs.people) && patrol.home==a.settlement && patrol.supplies.len()<=32
                        && patrol.supplies.iter().all(|s|people.contains(&s.carrier) && refs.inventories.contains(&s.source) && s.quantity.0>0)
                        && patrol.compensation.iter().all(|(worker,terms)|people.contains(worker) && terms.len()<=8 && terms.iter().all(|t|delivery_refs(t,refs,p.ratify_by) && t.parties()==[a.representative,*worker]))
                }
            }
        })
}
pub(crate) fn backing_shape(term: &Term, b: &BackedTerm) -> bool {
    match (term, &b.backing) {
        (
            Term::Delivery {
                term: AgreementTerm::Delivery { quantity, .. },
                ..
            },
            Backing::Delivery { obligation, .. },
        ) => {
            (obligation.is_none()
                == matches!(b.progress, Progress::Refused | Progress::AwaitingWork))
                && b.performed <= *quantity
                && (b.progress == Progress::Fulfilled) == (b.performed == *quantity)
        }
        (Term::Passage { until, .. }, Backing::Passage { .. }) => {
            b.performed.0 == 0
                && (b.progress != Progress::Fulfilled
                    || b.changed_at >= *until && b.defaulted_at.is_none())
        }
        (Term::Protection { patrol, .. }, Backing::Protection { .. }) => {
            b.performed.0 == 0
                && (b.progress != Progress::Fulfilled
                    || matches!(patrol.objective,MissionObjective::Patrol {until,..} if b.changed_at>=until))
        }
        _ => false,
    }
}
fn backing_refs(term: &Term, b: &BackedTerm, refs: &References, now: SimTime) -> bool {
    if b.changed_at > now || b.defaulted_at.is_some_and(|at| at > b.changed_at) {
        return false;
    }
    if !backing_shape(term, b) {
        return false;
    }
    match (term, &b.backing) {
        (
            Term::Delivery {
                term: AgreementTerm::Delivery { quantity, .. },
                ..
            },
            Backing::Delivery {
                agreement,
                obligation,
            },
        ) => {
            refs.agreements.contains(agreement)
                && obligation.is_none_or(|o| refs.obligations.contains(&o))
                && (obligation.is_none()
                    == matches!(b.progress, Progress::Refused | Progress::AwaitingWork))
                && b.performed <= *quantity
                && (b.progress == Progress::Fulfilled) == (b.performed == *quantity)
                && (b.progress != Progress::Defaulted || b.defaulted_at.is_some())
        }
        (
            Term::Passage { grantor, until, .. },
            Backing::Passage {
                institution,
                proposal,
            },
        ) => {
            grantor == institution
                && refs.proposals.contains(proposal)
                && b.performed.0 == 0
                && (b.progress != Progress::Fulfilled
                    || b.changed_at >= *until && b.defaulted_at.is_none())
                && (b.progress != Progress::Defaulted || b.defaulted_at.is_some())
        }
        (Term::Protection { patrol, .. }, Backing::Protection { mission }) => {
            refs.missions.contains(mission)
                && b.performed.0 == 0
                && (b.progress != Progress::Fulfilled
                    || matches!(patrol.objective,MissionObjective::Patrol {until,..} if b.changed_at>=until))
                && (b.progress != Progress::Defaulted || b.defaulted_at.is_some())
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TalkActivity {
    pub commitment: RealmCommitmentId,
    pub recipient: PersonId,
    pub completes: SimTime,
}
pub trait PendingFacts {
    fn talk_activity(&self, actor: PersonId) -> Option<TalkActivity>;
    fn physical_due(&self, actor: PersonId, at: SimTime) -> bool;
}
impl Runtime {
    pub fn validate(
        &self,
        store: &game_ecs::Store,
        refs: &References,
        now: SimTime,
        next_event: u64,
        pending: &impl PendingFacts,
    ) -> Result<(), Error> {
        let bad =
            || Error::InvalidSave("invalid Realm native rows or communication references".into());
        self.rules.validate().map_err(|_| bad())?;
        Ledger::new(store).validate(refs, now)?;
        if self.last_event >= next_event
            || request_rows(store).any(|(id, _)| !refs.people.contains(&id))
        {
            return Err(bad());
        }
        for (id, c) in commitments(store) {
            if id != c.id || c.validate().is_err() {
                return Err(bad());
            }
            for r in &c.revisions {
                for (index, work) in &r.pending {
                    if !refs.people.contains(&work.actor) || !r.requested.contains(index) {
                        return Err(bad());
                    }
                    match (&work.owner,r.proposal.terms.get(usize::from(*index))) {
                    (PendingOwner::Agreement(id),Some(Term::Delivery{term,..}))=>if game_agreements::agreement(store,*id).is_none_or(|a|!a.terms.contains(term)){return Err(bad());},
                    (PendingOwner::Mission(id),Some(Term::Protection{patrol,..}))=>if game_missions::mission(store,*id).is_none_or(|m|m.sponsor!=work.actor || m.proposal.leader!=patrol.leader){return Err(bad());},
                    (PendingOwner::Passage(id),Some(Term::Passage{..}))=>if game_society::proposal(store,*id).is_none_or(|p|p.change!=game_society::institutions::InstitutionChange::AuthorizeRegionalPassage{commitment:c.id,revision:r.number,index:*index}){return Err(bad());},
                    _=>return Err(bad()),
                }
                }
            }
        }
        for (speaker, talk) in talks(store) {
            if speaker != talk.speaker
                || talk.validate().is_err()
                || !refs.people.contains(&speaker)
                || !refs.people.contains(&talk.recipient)
                || !refs.areas.contains(&talk.area)
                || talk.completes <= now
                || pending.talk_activity(speaker)
                    != Some(TalkActivity {
                        commitment: talk.report.commitment,
                        recipient: talk.recipient,
                        completes: talk.completes,
                    })
                || !pending.physical_due(speaker, talk.completes)
                || talk.report.sent_at > now
                || talk
                    .institutions
                    .iter()
                    .any(|i| !refs.institutions.contains(&i.id))
                || talk
                    .stocks
                    .iter()
                    .any(|s| !refs.inventories.contains(&s.inventory))
            {
                return Err(bad());
            }
            Ledger::new(store)
                .prepare_receive_report(talk.recipient, &talk.report, now)
                .map_err(|_| bad())?;
        }
        for actor in &refs.people {
            if pending.talk_activity(*actor).is_some() && talk(store, *actor).is_none() {
                return Err(bad());
            }
        }
        Ok(())
    }
}
