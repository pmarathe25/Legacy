//! Saved autonomous character state; only game-characters registers and writes it.
use game_autonomy_policy::{Mind, Rules};
use game_choice::Schedule;
use game_core::*;
use game_packed::Packed;
use game_sim::*;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
/// Learn an actual failed attempt under the characters resource owner.
pub fn record_failure(tx: &mut Transaction<'_>, person: PersonId, id: u64) -> Result<()> {
    let now = tx.now();
    let retry = now.after(tx.get::<Minds>()?.rules.retry)?;
    if let Some(m) = tx.edit::<Minds>()?.people.get_mut(&person) {
        m.retry_after.retain(|_, at| *at > now);
        m.retry_after.insert(id, retry);
        m.failures = m.failures.saturating_add(1);
    }
    Ok(())
}
/// Complete only the still-current full-review generation and tuning interval.
pub fn complete_full_tier(
    tx: &mut Transaction<'_>,
    person: PersonId,
    now: SimTime,
    tier: Option<(u64, u64)>,
) -> Result<()> {
    if let Some((interval, generation)) = tier
        && game_choice::frame_tuning(tx).full_review_millis == interval
        && let Some(mind) = tx.edit::<Minds>()?.people.get_mut(&person)
    {
        mind.frame_tier.get_or_insert_default().complete_full(
            now,
            SimDuration(interval),
            generation,
        )?;
    }
    Ok(())
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Minds {
    pub people: Packed<PersonId, Mind>,
    pub rules: Arc<Rules>,
    pub running: bool,
    pub schedule: Schedule,
    /// Transaction-local selected reviews. Entries must be cleared before commit.
    #[serde(skip)]
    pub paused_reviews: BTreeMap<PersonId, bool>,
    /// Urgent evidence received during transaction-local cohort reviews.
    #[serde(skip)]
    pub cohort_urgency: BTreeMap<PersonId, u64>,
}
type MindMark = (
    game_packed::Mark,
    Arc<Rules>,
    bool,
    Schedule,
    BTreeMap<PersonId, bool>,
    BTreeMap<PersonId, u64>,
);
impl Resource for Minds {
    const KEY: &'static str = "characters.minds";
    fn validate(&self) -> Result<()> {
        self.rules.validate()?;
        require(self.paused_reviews.is_empty(), "unfinished paused review")?;
        require(self.cohort_urgency.is_empty(), "unfinished cohort review")?;
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
            self.paused_reviews.clone(),
            self.cohort_urgency.clone(),
        ))
    }
    fn restore(&mut self, checkpoint: Checkpoint) {
        let (mark, rules, running, schedule, paused_reviews, cohort_urgency) =
            *checkpoint.downcast::<MindMark>().expect("mind checkpoint");
        self.people.rollback(mark);
        self.rules = rules;
        self.running = running;
        self.schedule = schedule;
        self.paused_reviews = paused_reviews;
        self.cohort_urgency = cohort_urgency;
    }
    fn commit(&mut self, checkpoint: Checkpoint) {
        let checkpoint = checkpoint.downcast::<MindMark>().expect("mind checkpoint");
        self.people.commit(checkpoint.0);
    }
    fn validate_checkpoint(&self, _checkpoint: &dyn std::any::Any) -> Result<()> {
        self.rules.validate()?;
        require(self.paused_reviews.is_empty(), "unfinished paused review")?;
        require(self.cohort_urgency.is_empty(), "unfinished cohort review")
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
