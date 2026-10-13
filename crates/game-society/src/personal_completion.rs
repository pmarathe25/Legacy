//! Conversation/instruction outputs and personal experience prepare before origin publication.
use crate::personal::{Completion, PersonalReceipt};
use game_characters::{
    receivers::PreparedMindEffects,
    social::{self, Interaction, Personal, World},
};
use game_core::*;
use game_ecs::{Batch, Component, ReadFootprint, ReadGuard, ReadSet, StableId, Store};
use std::collections::BTreeMap;
pub struct PreparedPersonalCompletion {
    world: World,
    at: SimTime,
    leader: PersonId,
    interaction: Interaction,
    reads: ReadSet,
    after: BTreeMap<PersonId, Personal>,
    pub effect: Completion,
}
pub struct PersonalCompletionPublication {
    world: World,
    at: SimTime,
    rows: ReadGuard,
}
pub fn prepare_completion<A: Component>(
    world: &World,
    store: &Store,
    actor: PersonId,
    at: SimTime,
    can_progress: bool,
) -> Result<
    (
        Completion,
        Option<PreparedPersonalCompletion>,
        PreparedMindEffects<A>,
    ),
    Error,
> {
    let mut minds = PreparedMindEffects::default();
    let Some(leader) = social::participant(store, actor) else {
        return Ok((Completion::Waiting, None, minds));
    };
    let interaction = social::interaction(store, leader)
        .cloned()
        .ok_or(Error::Unavailable)?;
    if interaction.completes() != at {
        return Ok((Completion::Waiting, None, minds));
    }
    if !can_progress {
        return Ok((Completion::Interrupted, None, minds));
    }
    let mut reads = ReadSet::new();
    reads.read::<social::InteractionRow>(store, StableId(leader.0));
    let actors = interaction.actors();
    let members =
        reads.membership::<social::Participant>(store, move |_, row| row.leader == leader);
    if members != actors.iter().map(|actor| StableId(actor.0)).collect() {
        return Err(Error::Unavailable);
    }
    for actor in &actors {
        reads.read::<social::Participant>(store, StableId(actor.0));
    }
    let mut after = BTreeMap::new();
    let (effect, receipt) = match &interaction {
        Interaction::Talk {
            first,
            second,
            topic,
            ..
        } => {
            for (actor, other) in [(*first, *second), (*second, *first)] {
                let row = reads
                    .read::<Personal>(store, StableId(actor.0))
                    .ok_or(Error::Unavailable)?;
                after.insert(
                    actor,
                    social::conversation_personal_after(row, other, *topic),
                );
            }
            minds.receive_conversation(
                store,
                &world.conversation_effect(store, *first, *second, *topic)?,
            )?;
            (
                Completion::Socialized {
                    first: *first,
                    second: *second,
                    topic: *topic,
                },
                PersonalReceipt::Socialized {
                    first: *first,
                    second: *second,
                    topic: *topic,
                },
            )
        }
        Interaction::Instruction {
            teacher,
            student,
            topic,
            routes,
            ..
        } => {
            let mut row = reads
                .read::<Personal>(store, StableId(student.0))
                .cloned()
                .ok_or(Error::Unavailable)?;
            row.learned.insert((*teacher, *topic));
            after.insert(*student, row);
            if *topic == social::InstructionTopic::Routes {
                minds.receive_routes(store, *student, routes)?;
            }
            (
                Completion::Instructed {
                    teacher: *teacher,
                    student: *student,
                    topic: *topic,
                },
                PersonalReceipt::Instructed {
                    teacher: *teacher,
                    student: *student,
                },
            )
        }
        Interaction::Practice { actor, skill, .. } => (
            Completion::Practiced {
                actor: *actor,
                skill: *skill,
            },
            PersonalReceipt::Produced { actor: *actor },
        ),
    };
    crate::personal_receipts::project_receipts(store, &mut reads, &mut after, [receipt], at)?;
    Ok((
        effect.clone(),
        Some(PreparedPersonalCompletion {
            world: world.clone(),
            at,
            leader,
            interaction,
            reads,
            after,
            effect,
        }),
        minds,
    ))
}
impl PreparedPersonalCompletion {
    pub fn interaction(&self) -> &Interaction {
        &self.interaction
    }
    pub fn leader(&self) -> PersonId {
        self.leader
    }
    pub fn stage_into(
        self,
        world: &World,
        store: &Store,
        at: SimTime,
        batch: &mut Batch,
    ) -> Result<PersonalCompletionPublication, Error> {
        self.stage(world, store, at, batch, None)
    }
    pub fn stage_with_cleanup(
        self,
        world: &World,
        store: &Store,
        at: SimTime,
        batch: &mut Batch,
        cleanup: &social::SocialInterruptionPublication,
    ) -> Result<PersonalCompletionPublication, Error> {
        self.stage(world, store, at, batch, Some(cleanup))
    }
    fn stage(
        self,
        world: &World,
        store: &Store,
        at: SimTime,
        batch: &mut Batch,
        cleanup: Option<&social::SocialInterruptionPublication>,
    ) -> Result<PersonalCompletionPublication, Error> {
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
        if let Some(cleanup) = cleanup {
            cleanup.authorize_completion(store, batch, self.leader, &self.interaction)?;
            writes.protect::<social::InteractionRow>(StableId(self.leader.0));
            for actor in self.interaction.actors() {
                writes.protect::<social::Participant>(StableId(actor.0));
            }
        }
        Ok(PersonalCompletionPublication {
            world: self.world,
            at: self.at,
            rows: self.reads.bind(store, batch, &writes)?,
        })
    }
}
impl PersonalCompletionPublication {
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
