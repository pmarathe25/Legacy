//! Saved autonomous character state; only game-characters registers and writes it.
use game_autonomy_policy::{Mind, Rules};
use game_choice::Schedule;
use game_core::*;
use game_packed::Packed;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Minds {
    pub people: Packed<PersonId, Mind>,
    pub rules: Arc<Rules>,
    pub running: bool,
    pub schedule: Schedule,
}
type MindMark = (game_packed::Mark, Arc<Rules>, bool, Schedule);
impl Resource for Minds {
    const KEY: &'static str = "characters.minds";
    fn validate(&self) -> Result<()> {
        self.rules.validate()?;
        require(
            self.running || self.schedule == Schedule::default(),
            "unstarted review clock",
        )?;
        require(
            self.schedule.agenda.valid()
                && self
                    .schedule
                    .ready
                    .keys()
                    .all(|p| self.people.contains_key(p) && self.schedule.agenda.get(p).is_none())
                && self
                    .schedule
                    .agenda
                    .iter()
                    .all(|(person, _)| self.people.contains_key(person)),
            "invalid review agenda",
        )
    }
    fn checkpoint(&mut self) -> Checkpoint {
        Box::new((
            self.people.begin(),
            self.rules.clone(),
            self.running,
            self.schedule.clone(),
        ))
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        let (mark, rules, running, schedule) =
            *checkpoint.downcast::<MindMark>().expect("mind checkpoint");
        self.people.rollback(mark);
        self.rules = rules;
        self.running = running;
        self.schedule = schedule;
    }
    fn commit(&mut self, checkpoint: Checkpoint) {
        let checkpoint = checkpoint.downcast::<MindMark>().expect("mind checkpoint");
        self.people.commit(checkpoint.0);
    }
    fn validate_checkpoint(&self, _checkpoint: &dyn std::any::Any) -> Result<()> {
        self.rules.validate()
    }
}

impl Minds {
    pub fn mark_frame_review(&mut self, person: PersonId, now: SimTime, start: SimTime) -> bool {
        self.people
            .get_mut(&person)
            .is_some_and(|m| m.review_clock.get_or_insert_default().frame(now, start))
    }
    pub fn clear_wake_at(&mut self, person: PersonId, now: SimTime) {
        if self.schedule.waking.get(&person) == Some(&now) {
            self.schedule.waking.remove(&person);
        }
    }
}
