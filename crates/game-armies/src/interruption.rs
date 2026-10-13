//! Linked Army cancellation selection and receiving activity effects.
use crate::*;
use game_core::{PersonId, SimTime};
use game_ecs::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptionActivity {
    Other,
    Talk { completes: SimTime },
    Approach,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmyInterruption {
    pub talks: Vec<Talk>,
    pub approach: Option<Approach>,
}
pub fn prepare_interruption_rows(store: &Store, actor: PersonId) -> ArmyInterruption {
    ArmyInterruption {
        talks: talks(store)
            .filter(|(_, row)| row.speaker == actor || row.listener == actor)
            .map(|(_, row)| row.clone())
            .collect(),
        approach: approach(store, actor).cloned(),
    }
}
impl ArmyInterruption {
    pub fn talk_reset(talk: &Talk, activity: InterruptionActivity) -> Option<PersonId> {
        matches!(activity, InterruptionActivity::Talk { completes } if completes == talk.completes)
            .then_some(talk.speaker)
    }
    pub fn approach_reset(approach: &Approach, activity: InterruptionActivity) -> Option<PersonId> {
        matches!(activity, InterruptionActivity::Approach).then_some(approach.actor)
    }
}
