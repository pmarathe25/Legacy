//! Individual purpose follows completed work, including work published early.
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
        let events: Vec<_> = progress
            .par_iter()
            .map(|&person| game_purpose::Event {
                person,
                outcome: game_purpose::Outcome::Progress,
            })
            .collect();
        let next = current.owner.prepare(&events, tick, |id| {
            people
                .binary_search_by_key(&id, |person| person.id)
                .ok()
                .is_some_and(|index| people[index].health > 0)
        });
        let mut harmed: Vec<_> = next
            .people
            .par_iter()
            .filter(|person| {
                tick >= person.eligible_at && person.deprivation >= next.rules.damage_threshold
            })
            .map(|person| person.person)
            .collect();
        harmed.par_sort_unstable();
        people.par_iter_mut().for_each(|person| {
            if person.health > 0 && harmed.binary_search(&person.id).is_ok() {
                person.health = person.health.saturating_sub(next.rules.damage);
            }
        });
        Some(CorePurpose {
            owner: next,
            progress: vec![],
        })
    }
}
