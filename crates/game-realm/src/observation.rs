//! Institutional announcements reveal local proposal terms without remote refresh.
use crate::*;
use game_core::*;
use game_ecs::Store;
use game_society::institutions::{InstitutionChange, RuleProposalStatus};
use std::collections::BTreeMap;
impl Runtime {
    pub fn prepare_observation(
        &self,
        store: &Store,
        now: SimTime,
        presence: &impl Presence,
    ) -> Result<Vec<PreparedChange>, Error> {
        if !self.has_commitments(store) {
            return Ok(Vec::new());
        }
        let ledger = Ledger::new(store);
        let mut plans = BTreeMap::<RealmCommitmentId, PreparedChange>::new();
        for actor in presence.people() {
            let Some(area) = actor_area(presence, actor) else {
                continue;
            };
            for institution in institution_view(store, actor)
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
                    let actual = game_society::proposal(store, p.id).ok_or(Error::Unavailable)?;
                    let Some(c) = commitment(store, id) else {
                        continue;
                    };
                    let Some(r) = c.revision(revision) else {
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
                    if let std::collections::btree_map::Entry::Vacant(entry) = plans.entry(id) {
                        entry.insert(ledger.edit(id)?);
                    }
                    let plan = plans.get_mut(&id).unwrap();
                    let statement = Dated {
                        value: r.proposal.clone(),
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
                    if matches!(actual.change, InstitutionChange::RatifyRegional { .. })
                        && p.status == RuleProposalStatus::Proposed
                    {
                        let current = plan.after.as_mut().unwrap().revisions.last_mut().unwrap();
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
        Ok(plans.into_values().collect())
    }
}
