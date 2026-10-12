//! Selected work-owner records guard a regional receiving plan before publication.
use crate::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
#[derive(Clone, Debug)]
pub(crate) enum WorkRead {
    Canonical(Commitment),
    Agreement(game_agreements::Agreement),
    Obligation(game_agreements::Obligation),
    Mission(game_missions::Mission),
    KnownAgreement {
        actor: PersonId,
        id: AgreementId,
        before: Option<game_agreements::AgreementView>,
    },
    KnownMission {
        actor: PersonId,
        id: MissionId,
        before: Option<game_missions::MissionView>,
    },
}
impl WorkRead {
    pub(crate) fn check(&self, store: &Store) -> Result<(), Error> {
        let unchanged = match self {
            Self::Canonical(row) => commitment(store, row.id) == Some(row),
            Self::Agreement(row) => game_agreements::agreement(store, row.id) == Some(row),
            Self::Obligation(row) => game_agreements::obligation(store, row.id) == Some(row),
            Self::Mission(row) => game_missions::mission(store, row.id) == Some(row),
            Self::KnownAgreement { actor, id, before } => {
                game_agreements::known_agreement(store, *actor, *id) == before.as_ref()
            }
            Self::KnownMission { actor, id, before } => {
                game_missions::known_mission(store, *actor, *id) == before.as_ref()
            }
        };
        if unchanged {
            Ok(())
        } else {
            Err(Error::Unavailable)
        }
    }
    pub(crate) fn check_batch(&self, batch: &Batch) -> Result<(), Error> {
        let touched = match self {
            Self::Canonical(row) => batch.touches::<Commitment>(StableId(row.id.0)),
            Self::Agreement(row) => batch.touches::<game_agreements::Agreement>(StableId(row.id.0)),
            Self::Obligation(row) => {
                batch.touches::<game_agreements::Obligation>(StableId(row.id.0))
            }
            Self::Mission(row) => batch.touches::<game_missions::Mission>(StableId(row.id.0)),
            Self::KnownAgreement { actor, .. } => {
                batch.touches::<game_agreements::Knowledge>(StableId(actor.0))
            }
            Self::KnownMission { actor, .. } => {
                batch.touches::<game_missions::Knowledge>(StableId(actor.0))
            }
        };
        if touched {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl Ledger<'_> {
    pub(crate) fn work_reads(&self, key: TermKey, actor: PersonId) -> Result<Vec<WorkRead>, Error> {
        let r = self
            .commitments
            .get(&key.commitment)
            .and_then(|c| c.revision(key.revision))
            .ok_or(Error::Unavailable)?;
        let mut reads = Vec::new();
        if let Some(pending) = r.pending.get(&key.index) {
            match pending.owner {
                PendingOwner::Agreement(id) => {
                    let a = game_agreements::agreement(self.store, id).ok_or(Error::Unavailable)?;
                    reads.push(WorkRead::Agreement(a.clone()));
                    for id in &a.obligations {
                        reads.push(WorkRead::Obligation(
                            game_agreements::obligation(self.store, *id)
                                .ok_or(Error::Unavailable)?
                                .clone(),
                        ));
                    }
                    reads.push(WorkRead::KnownAgreement {
                        actor,
                        id,
                        before: game_agreements::known_agreement(self.store, actor, id).cloned(),
                    });
                }
                PendingOwner::Mission(id) => {
                    reads.push(WorkRead::Mission(
                        game_missions::mission(self.store, id)
                            .ok_or(Error::Unavailable)?
                            .clone(),
                    ));
                    reads.push(WorkRead::KnownMission {
                        actor,
                        id,
                        before: game_missions::known_mission(self.store, actor, id).cloned(),
                    });
                }
                PendingOwner::Passage(_) => {}
            }
        }
        if let Some(backed) = r.backed.get(&key.index) {
            match backed.backing {
                Backing::Delivery {
                    agreement,
                    obligation: Some(id),
                } => {
                    reads.push(WorkRead::Obligation(
                        game_agreements::obligation(self.store, id)
                            .ok_or(Error::Unavailable)?
                            .clone(),
                    ));
                    reads.push(WorkRead::KnownAgreement {
                        actor,
                        id: agreement,
                        before: game_agreements::known_agreement(self.store, actor, agreement)
                            .cloned(),
                    });
                }
                Backing::Protection { mission: id } => {
                    reads.push(WorkRead::Mission(
                        game_missions::mission(self.store, id)
                            .ok_or(Error::Unavailable)?
                            .clone(),
                    ));
                    reads.push(WorkRead::KnownMission {
                        actor,
                        id,
                        before: game_missions::known_mission(self.store, actor, id).cloned(),
                    });
                }
                _ => {}
            }
        }
        Ok(reads)
    }
}
