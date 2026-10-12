//! Society owns published institutional snapshots; Realm receives their promises.
use crate::*;
use game_core::*;
use game_ecs::{Batch, Store};
use game_society::{
    institutions::{InstitutionChange, RuleProposalStatus},
    projected_publication::{CommandPublicationGuard, PreparedCommandPublication},
};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct PreparedInstitutionReaction {
    changes: Vec<PreparedChange>,
    publications: Vec<PreparedCommandPublication>,
}
impl PreparedInstitutionReaction {
    pub fn stage_into(
        self,
        store: &Store,
        source: &CommandPublicationGuard,
        batch: &mut Batch,
    ) -> Result<RealmBatchGuard, Error> {
        source.authorizes_publications(batch, &self.publications)?;
        stage_changes(self.changes, store, batch)
    }
}
impl Runtime {
    pub fn prepare_institution_reaction(
        &self,
        store: &Store,
        now: SimTime,
        publications: &[PreparedCommandPublication],
        initial: Vec<PreparedChange>,
        observe: bool,
        presence: &impl Presence,
    ) -> Result<PreparedInstitutionReaction, Error> {
        let mut plans = initial
            .into_iter()
            .map(|p| (p.commitment_id(), p))
            .collect::<BTreeMap<_, _>>();
        if observe && self.has_commitments(store) {
            for publication in publications {
                for (actor, views) in publication.personal_views(store) {
                    let Some(area) = actor_area(presence, actor) else {
                        continue;
                    };
                    for institution in views
                        .into_iter()
                        .filter(|i| i.observed_at == now && i.charter.jurisdiction == area)
                    {
                        for p in institution.proposals {
                            let (id, revision) = match p.change {
                                InstitutionChange::RatifyRegional {
                                    commitment,
                                    revision,
                                    ..
                                }
                                | InstitutionChange::AuthorizeRegionalPassage {
                                    commitment,
                                    revision,
                                    ..
                                } => (commitment, revision),
                                _ => continue,
                            };
                            let base = plans
                                .get(&id)
                                .and_then(|p| p.after.as_ref())
                                .or_else(|| commitment(store, id));
                            let Some(r) = base.and_then(|c| c.revision(revision)) else {
                                continue;
                            };
                            if now < r.proposed_at
                                || !r
                                    .proposal
                                    .participants
                                    .values()
                                    .any(|p| p.settlement == area)
                            {
                                continue;
                            }
                            let proposal = r.proposal.clone();
                            if let std::collections::btree_map::Entry::Vacant(entry) =
                                plans.entry(id)
                            {
                                entry.insert(Ledger::new(store).edit(id)?);
                            }
                            let plan = plans.get_mut(&id).unwrap();
                            let statement = Dated {
                                value: proposal,
                                observed_at: now,
                                learned_at: now,
                                source: KnowledgeSource::Observed(actor),
                            };
                            let known = plan.personal_patch(store, actor);
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
                            if matches!(p.change, InstitutionChange::RatifyRegional { .. })
                                && p.status == RuleProposalStatus::Proposed
                            {
                                let current =
                                    plan.after.as_mut().unwrap().revisions.last_mut().unwrap();
                                if current.number == revision
                                    && !current.ratification_proposals.contains_key(&institution.id)
                                    && current.proposal.participants.contains_key(&institution.id)
                                    && now >= current.proposed_at
                                    && now <= current.proposal.ratify_by
                                    && current.status() == Status::AwaitingRatification
                                    && p.id.0 > 0
                                    && !current
                                        .ratification_proposals
                                        .values()
                                        .any(|id| *id == p.id)
                                {
                                    current.ratification_proposals.insert(institution.id, p.id);
                                }
                            }
                        }
                    }
                }
            }
            // Preserve the old hook's reconciliation of existing work, while
            // borrowing each selected projected commitment rather than a world.
            let ids = Ledger::new(store).reconciliation_ids();
            for id in ids {
                let later = if let Some(first) = plans.get(&id) {
                    Ledger::projected(store, first.after.as_ref().ok_or(Error::Unavailable)?)
                        .prepare_reconciliation(id, now, presence)?
                } else {
                    Ledger::new(store).prepare_reconciliation(id, now, presence)?
                };
                if let Some(first) = plans.remove(&id) {
                    plans.insert(id, first.merge_later(later)?);
                } else {
                    plans.insert(id, later);
                }
            }
        }
        Ok(PreparedInstitutionReaction {
            changes: plans.into_values().collect(),
            publications: publications.to_vec(),
        })
    }
}
