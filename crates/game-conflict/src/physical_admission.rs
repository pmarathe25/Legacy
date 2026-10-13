//! Confinement and encounter restrictions on personal physical intentions.
use game_core::Error;
use game_physiology::PersonalActionKind;

pub fn physical_action_allowed(
    restricted: bool,
    in_combat: bool,
    kind: PersonalActionKind,
) -> bool {
    !in_combat
        && (!restricted
            || matches!(
                kind,
                PersonalActionKind::Rest
                    | PersonalActionKind::Eat
                    | PersonalActionKind::Tell
                    | PersonalActionKind::Release
            ))
}
pub fn require_release_admission(available: bool) -> Result<(), Error> {
    if available {
        Ok(())
    } else {
        Err(Error::Unavailable)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseCompletion {
    Release {
        actor: game_core::PersonId,
        target: game_core::PersonId,
    },
    Interrupted {
        actor: game_core::PersonId,
    },
}
pub fn release_completion(
    actor: game_core::PersonId,
    target: game_core::PersonId,
    available: bool,
) -> ReleaseCompletion {
    if available {
        ReleaseCompletion::Release { actor, target }
    } else {
        ReleaseCompletion::Interrupted { actor }
    }
}
impl ReleaseCompletion {
    pub fn idle_actor(self) -> game_core::PersonId {
        match self {
            Self::Release { actor, .. } | Self::Interrupted { actor } => actor,
        }
    }
}
/// Protect the exact facts of a failed release while ending its receiving work.
pub fn prepare_failed_release<C: game_ecs::Component, P: game_ecs::Component>(
    store: &game_ecs::Store,
    actor: game_core::PersonId,
    target: game_core::PersonId,
    mut contact_reads: game_ecs::ReadSet,
) -> game_physiology::PreparedActivityEnding<C> {
    for person in [actor, target] {
        let id = game_ecs::StableId(person.0);
        contact_reads.read::<game_physiology::Body>(store, id);
        contact_reads.read::<P>(store, id);
        contact_reads.read::<crate::Captivity>(store, id);
    }
    game_physiology::prepare_activity_ending(store, actor, contact_reads)
}

#[cfg(test)]
mod ending_tests {
    use super::*;
    use game_core::PersonId;
    use game_ecs::{Batch, StableId};
    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct ActorRecord(u8);
    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Activity(u8);
    #[test]
    fn native_release_selection_and_failed_receiving_ending_protect_linked_actor_facts() {
        let actor = PersonId(1);
        let target = PersonId(2);
        assert_eq!(
            release_completion(actor, target, true),
            ReleaseCompletion::Release { actor, target }
        );
        let effect = release_completion(actor, target, false);
        assert_eq!(effect, ReleaseCompletion::Interrupted { actor });
        assert_eq!(effect.idle_actor(), actor);
        let mut schema = game_ecs::Schema::new();
        crate::register_components(&mut schema).unwrap();
        game_physiology::register_components(&mut schema).unwrap();
        schema
            .register::<ActorRecord>("test/release-person", |_| Ok(()))
            .unwrap();
        schema
            .register::<Activity>("test/release-activity", |_| Ok(()))
            .unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        store
            .insert_or_spawn(StableId(actor.0), ActorRecord(1))
            .unwrap();
        store
            .insert_or_spawn(StableId(target.0), ActorRecord(2))
            .unwrap();
        store
            .insert_or_spawn(StableId(actor.0), Activity(1))
            .unwrap();
        let plan = prepare_failed_release::<Activity, ActorRecord>(
            &store,
            actor,
            target,
            game_ecs::ReadSet::new(),
        );
        let mut batch = Batch::new();
        let guard = plan.stage_into(&store, &mut batch, Activity(0)).unwrap();
        guard.validate(&store, &batch).unwrap();
        batch.insert(StableId(target.0), ActorRecord(3));
        assert!(guard.validate(&store, &batch).is_err());
        let plan = prepare_failed_release::<Activity, ActorRecord>(
            &store,
            actor,
            target,
            game_ecs::ReadSet::new(),
        );
        store
            .insert_or_spawn(StableId(target.0), ActorRecord(4))
            .unwrap();
        assert!(
            plan.stage_into(&store, &mut Batch::new(), Activity(0))
                .is_err()
        );
    }
}
