//! Individual purpose follows personal interpretations of realized activities.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorePurpose {
    pub owner: game_purpose::Purpose,
    /// People whose work finished early in the current physical interval.
    #[serde(default)]
    pub progress: Vec<u64>,
}

impl CorePurpose {
    pub(super) fn validate(&self, people: &HashSet<u64>) -> Result<(), String> {
        self.owner.validate(people).map_err(str::to_owned)?;
        if self.progress.par_windows(2).any(|rows| rows[0] >= rows[1])
            || self
                .progress
                .par_iter()
                .any(|person| !people.contains(person))
        {
            return Err("invalid pending purpose progress".into());
        }
        Ok(())
    }
}

impl CoreHost {
    pub(super) fn next_purpose(&self) -> Option<u64> {
        let period = self.purpose.as_ref()?.owner.rules.elapsed_period_millis?;
        let offset = (self.core.simulated_millis() % u128::from(period)) as u64;
        Some(period - offset)
    }

    pub(super) fn stage_elapsed_purpose(
        &self,
        people: &mut [Person],
        tick: u64,
    ) -> (CorePurpose, Vec<u64>) {
        let previous = self
            .purpose
            .as_ref()
            .expect("purpose deadline has an owner");
        let owner = previous.owner.prepare(&[], tick, |id| alive(people, id));
        let casualties = harm(&owner, people, tick);
        let purpose = CorePurpose {
            owner,
            progress: previous.progress.clone(),
        };
        (purpose, casualties)
    }

    pub(super) fn record_personal_choices(
        &mut self,
        run: &mut CoreRun,
        choices: Vec<CoreChoiceEvent>,
    ) {
        if choices.is_empty() {
            return;
        }
        if let Some(purpose) = &mut self.purpose {
            use game_purpose::{ActionOutcome, Experience};
            let experiences: Vec<_> = choices
                .par_iter()
                .filter_map(|event| {
                    let key = event.key?;
                    let outcome = match event.outcome {
                        CoreChoiceOutcome::Accepted => ActionOutcome::Accepted,
                        CoreChoiceOutcome::Denied => ActionOutcome::Denied,
                        CoreChoiceOutcome::Stopped => ActionOutcome::Stopped,
                        _ => return None,
                    };
                    let index = self
                        .core
                        .people
                        .binary_search_by_key(&event.actor, |person| person.id)
                        .ok()?;
                    if self.core.people[index].health == 0 {
                        return None;
                    }
                    Some(Experience {
                        person: event.actor,
                        domain: key.domain,
                        outcome,
                        tick: event.tick,
                    })
                })
                .collect();
            purpose.owner.appraise(&experiences);
        }
        run.events.push(CoreRunEvent::PersonalChoices(choices));
    }

    pub(super) fn stage_purpose_progress(&self, finished: &[u64]) -> Option<Vec<u64>> {
        let owner = self.purpose.as_ref()?;
        let mut progress = owner.progress.clone();
        progress.par_extend(finished.par_iter().filter_map(|job| {
            self.core
                .physical
                .work()
                .binary_search_by_key(job, |row| row.job.id)
                .ok()
                .map(|index| self.core.physical.work()[index].job.worker)
        }));
        progress.par_sort_unstable();
        progress.dedup();
        Some(progress)
    }

    pub(super) fn stage_purpose(
        &self,
        finished: &[u64],
        people: &mut [Person],
        tick: u64,
    ) -> Option<CorePurpose> {
        let current = self.purpose.as_ref()?;
        let progress = self
            .stage_purpose_progress(finished)
            .expect("purpose owner");
        let mut events: Vec<_> = progress
            .par_iter()
            .map(|&person| game_purpose::Event {
                person,
                outcome: game_purpose::Outcome::Progress,
            })
            .collect();
        events.par_extend(
            self.goal_setbacks
                .par_iter()
                .map(|&(person, _)| game_purpose::Event {
                    person,
                    outcome: game_purpose::Outcome::Blocked,
                }),
        );
        let alive = |id| alive(people, id);
        let next = if current.owner.rules.elapsed_period_millis.is_some() {
            current.owner.settle_outcomes(&events, tick, alive)
        } else {
            let next = current.owner.prepare(&events, tick, alive);
            harm(&next, people, tick);
            next
        };
        Some(CorePurpose {
            owner: next,
            progress: vec![],
        })
    }
}

fn alive(people: &[Person], id: u64) -> bool {
    people
        .binary_search_by_key(&id, |person| person.id)
        .ok()
        .is_some_and(|index| people[index].health > 0)
}

fn harm(owner: &game_purpose::Purpose, people: &mut [Person], tick: u64) -> Vec<u64> {
    if owner.rules.damage == 0 {
        return Vec::new();
    }
    let harmed: Vec<_> = owner
        .people
        .par_iter()
        .filter(|person| {
            tick >= person.eligible_at && person.deprivation >= owner.rules.damage_threshold
        })
        .map(|person| person.person)
        .collect();
    people.par_iter_mut().filter_map(|person| {
        if person.health == 0 || harmed.binary_search(&person.id).is_err() {
            return None;
        }
        person.health = person.health.saturating_sub(owner.rules.damage);
        (person.health == 0).then_some(person.id)
    }).collect()
}
