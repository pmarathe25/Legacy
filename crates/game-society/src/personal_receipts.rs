//! Prepared personal experience receiving, including a reserved newborn identity.
use crate::personal::PersonalReceipt;
use game_characters::social::{self, Personal, UpbringingEvidence, World};
use game_core::*;
use game_ecs::{Batch, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeMap;
pub struct PreparedPersonalReceipts {
    world: World,
    at: SimTime,
    reads: ReadSet,
    after: BTreeMap<PersonId, Personal>,
}
pub struct PersonalReceiptPublication {
    world: World,
    at: SimTime,
    rows: ReadGuard,
}
fn observe(
    store: &Store,
    reads: &mut ReadSet,
    after: &mut BTreeMap<PersonId, Personal>,
    actor: PersonId,
    evidence: UpbringingEvidence,
) {
    if let std::collections::btree_map::Entry::Vacant(entry) = after.entry(actor) {
        let Some(row) = reads.read::<Personal>(store, StableId(actor.0)).cloned() else {
            return;
        };
        entry.insert(row);
    }
    after
        .get_mut(&actor)
        .unwrap()
        .development
        .observe_event(evidence);
}
pub fn prepare_receipt(
    world: &World,
    store: &Store,
    receipt: PersonalReceipt,
    at: SimTime,
) -> Result<PreparedPersonalReceipts, Error> {
    prepare_receipts(world, store, [receipt], at)
}
pub fn prepare_receipts(
    world: &World,
    store: &Store,
    receipts: impl IntoIterator<Item = PersonalReceipt>,
    at: SimTime,
) -> Result<PreparedPersonalReceipts, Error> {
    if !world.rules.valid() {
        return Err(Error::InvalidConfiguration(
            "invalid social development rules".into(),
        ));
    }
    let mut reads = ReadSet::new();
    let mut after = BTreeMap::new();
    project_receipts(store, &mut reads, &mut after, receipts, at)?;
    Ok(PreparedPersonalReceipts {
        world: world.clone(),
        at,
        reads,
        after,
    })
}
pub(crate) fn project_receipts(
    store: &Store,
    reads: &mut ReadSet,
    after: &mut BTreeMap<PersonId, Personal>,
    receipts: impl IntoIterator<Item = PersonalReceipt>,
    at: SimTime,
) -> Result<(), Error> {
    for receipt in receipts {
        use PersonalReceipt::*;
        match receipt {
            Born {
                child,
                temperament,
                witnesses,
            } => {
                if child.0 == 0
                    || reads.read::<Personal>(store, StableId(child.0)).is_some()
                    || after.contains_key(&child)
                {
                    return Err(Error::Unavailable);
                }
                reads.entity(store, StableId(child.0));
                let mut development =
                    social::Development::baseline(temperament, social::Aspiration::Comfort)?;
                development.next_choice = at;
                after.insert(
                    child,
                    Personal {
                        development,
                        ..Default::default()
                    },
                );
                for witness in witnesses.into_iter().filter(|witness| {
                    witness.actor != child && witness.same_location && witness.alive
                }) {
                    observe(
                        store,
                        reads,
                        after,
                        witness.actor,
                        UpbringingEvidence::WitnessedBirth,
                    );
                }
            }
            Cared { caregiver, child } => {
                observe(store, reads, after, child, UpbringingEvidence::ReceivedCare);
                observe(store, reads, after, caregiver, UpbringingEvidence::GaveCare);
            }
            Comforted { caregiver, child } => {
                observe(
                    store,
                    reads,
                    after,
                    child,
                    UpbringingEvidence::ReceivedComfort,
                );
                observe(
                    store,
                    reads,
                    after,
                    caregiver,
                    UpbringingEvidence::GaveComfort,
                );
            }
            Played { actor, interest } => observe(
                store,
                reads,
                after,
                actor,
                UpbringingEvidence::Played(interest),
            ),
            Produced { actor } => observe(store, reads, after, actor, UpbringingEvidence::Produced),
            Instructed { teacher, student } => {
                observe(
                    store,
                    reads,
                    after,
                    student,
                    UpbringingEvidence::ReceivedInstruction,
                );
                observe(
                    store,
                    reads,
                    after,
                    teacher,
                    UpbringingEvidence::GaveInstruction,
                );
            }
            Socialized {
                first,
                second,
                topic,
            } => {
                observe(
                    store,
                    reads,
                    after,
                    first,
                    UpbringingEvidence::Conversation(topic),
                );
                observe(
                    store,
                    reads,
                    after,
                    second,
                    UpbringingEvidence::Conversation(topic),
                );
            }
        }
    }
    Ok(())
}

impl PreparedPersonalReceipts {
    pub fn stage_into(
        self,
        world: &World,
        store: &Store,
        at: SimTime,
        batch: &mut Batch,
    ) -> Result<PersonalReceiptPublication, Error> {
        if world != &self.world || at != self.at {
            return Err(Error::Unavailable);
        }
        self.reads.validate(store)?;
        let mut writes = ReadFootprint::new();
        for (actor, row) in self.after {
            let id = StableId(actor.0);
            if batch.touches::<Personal>(id) {
                return Err(Error::Unavailable);
            }
            writes.protect::<Personal>(id);
            batch.insert(id, row);
        }
        Ok(PersonalReceiptPublication {
            world: self.world,
            at: self.at,
            rows: self.reads.bind(store, batch, &writes)?,
        })
    }
    pub fn commit(self, world: &World, store: &mut Store, at: SimTime) -> Result<(), Error> {
        let mut batch = Batch::new();
        let receipt = self.stage_into(world, store, at, &mut batch)?;
        receipt.validate(world, store, at, &batch)?;
        store.apply_batch(batch)
    }
}
impl PersonalReceiptPublication {
    pub fn validate(
        &self,
        world: &World,
        store: &Store,
        at: SimTime,
        batch: &Batch,
    ) -> Result<(), Error> {
        if world != &self.world || at != self.at {
            return Err(Error::Unavailable);
        }
        self.rows.validate(store, batch)
    }
    pub fn row_guard(&self) -> ReadGuard {
        self.rows.clone()
    }
}
