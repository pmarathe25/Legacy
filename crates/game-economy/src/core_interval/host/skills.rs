//! Personal skill credits from actual work, retained across interval saves.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSkillCredit {
    pub job: u64,
    pub worker: u64,
    pub recipe: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreSkills {
    pub owner: game_economy_skills::Skills,
    /// Jobs that made actual progress in this physical interval.
    #[serde(default)]
    pub practiced: Vec<CoreSkillCredit>,
    /// Jobs whose completion was published before this boundary.
    #[serde(default)]
    pub completed: Vec<CoreSkillCredit>,
}

impl CoreSkills {
    pub(super) fn initial_rates(
        &self,
        core: &CoreSnapshot,
        rates: &[(u64, u64)],
    ) -> Result<Vec<(u64, u64)>, String> {
        let people: HashSet<_> = core.people.par_iter().map(|person| person.id).collect();
        let recipes: HashSet<_> = core
            .physical
            .recipes
            .par_iter()
            .map(|recipe| recipe.id)
            .collect();
        self.owner
            .validate(&people, &recipes, core.physical.custody.tick)?;
        if !self.owner.teachers.is_empty() || self.owner.inherit_teachers {
            return Err("personal teaching is not yet supported by the interval host".into());
        }
        let index = self.owner.index();
        rates
            .par_iter()
            .map(|&(id, base)| {
                let factor = core
                    .physical
                    .work
                    .binary_search_by_key(&id, |row| row.job.id)
                    .ok()
                    .map_or(1, |position| {
                        let job = &core.physical.work[position].job;
                        index.rate(job.worker, job.recipe)
                    });
                Ok((
                    id,
                    base.checked_mul(factor).ok_or("skill work rate overflow")?,
                ))
            })
            .collect()
    }
    pub(super) fn practice_recipes(&self) -> HashSet<u32> {
        let owner = &self.owner;
        let mut recipes: HashSet<_> = owner
            .rules
            .par_iter()
            .filter(|rule| rule.practice_each_tick)
            .map(|rule| rule.recipe)
            .collect();
        if recipes.is_empty() {
            return recipes;
        }
        let aliases: Vec<_> = owner
            .aliases
            .par_iter()
            .filter(|alias| recipes.contains(&alias.canonical))
            .map(|alias| alias.recipe)
            .collect();
        recipes.extend(aliases);
        recipes
    }
    pub(super) fn validate(&self, core: &CoreInterval) -> Result<(), String> {
        let people: HashSet<_> = core.people.par_iter().map(|row| row.id).collect();
        let recipes: HashSet<_> = core
            .physical
            .recipes()
            .par_iter()
            .map(|row| row.id)
            .collect();
        self.owner
            .validate(&people, &recipes, core.physical.tick())?;
        if !self.owner.teachers.is_empty() || self.owner.inherit_teachers {
            return Err("personal teaching is not yet supported by the interval host".into());
        }
        for rows in [&self.practiced, &self.completed] {
            if rows.par_windows(2).any(|pair| pair[0].job >= pair[1].job)
                || rows.par_iter().any(|row| {
                    row.job == 0 || !people.contains(&row.worker) || !recipes.contains(&row.recipe)
                })
            {
                return Err("invalid pending skill credit".into());
            }
        }
        Ok(())
    }
}

impl CoreHost {
    /// Set a job's work rate from the person's own skill at the time it starts.
    pub(super) fn skill_start_rates(
        &self,
        starts: &[(game_production_model::Start, u64)],
    ) -> Result<Option<Vec<(game_production_model::Start, u64)>>, String> {
        let Some(index) = &self.skill_rates else {
            return Ok(None);
        };
        if starts.is_empty() {
            return Ok(None);
        }
        starts
            .par_iter()
            .map(|&(start, base)| {
                let rate = base
                    .checked_mul(index.rate(start.worker, start.recipe))
                    .ok_or("skill work rate overflow")?;
                Ok((start, rate))
            })
            .collect::<Result<Vec<_>, String>>()
            .map(Some)
    }
    pub(super) fn plan_skill_practice(&self, millis: u64) -> Vec<CoreSkillCredit> {
        if millis == 0 {
            return Vec::new();
        }
        let recipes = &self.skill_practice_recipes;
        if recipes.is_empty() {
            return Vec::new();
        }
        let Some(interval) = self.core.physical.work_interval() else {
            return Vec::new();
        };
        interval
            .rates
            .par_iter()
            .filter(|(_, rate)| *rate > 0)
            .filter_map(|&(job, _)| {
                let index = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&job, |row| row.job.id)
                    .ok()?;
                let work = &self.core.physical.work()[index];
                (work.job.remaining > 0 && recipes.contains(&work.job.recipe)).then_some(
                    CoreSkillCredit {
                        job,
                        worker: work.job.worker,
                        recipe: work.job.recipe,
                    },
                )
            })
            .collect()
    }
    pub(super) fn remember_skill_practice(&mut self, rows: Vec<CoreSkillCredit>) {
        if rows.is_empty() {
            return;
        }
        let owner = self.skills.as_mut().expect("planned skill owner");
        owner.practiced.par_extend(rows);
        owner.practiced.par_sort_unstable_by_key(|row| row.job);
        owner.practiced.dedup_by_key(|row| row.job);
    }
    pub(super) fn plan_skill_completions(&self, finished: &[u64]) -> Vec<CoreSkillCredit> {
        if self.skills.is_none() || finished.is_empty() {
            return Vec::new();
        }
        finished
            .par_iter()
            .filter_map(|&job| {
                let index = self
                    .core
                    .physical
                    .work()
                    .binary_search_by_key(&job, |row| row.job.id)
                    .ok()?;
                let work = &self.core.physical.work()[index];
                Some(CoreSkillCredit {
                    job,
                    worker: work.job.worker,
                    recipe: work.job.recipe,
                })
            })
            .collect()
    }
    pub(super) fn remember_skill_completions(&mut self, rows: Vec<CoreSkillCredit>) {
        if rows.is_empty() {
            return;
        }
        let owner = self.skills.as_mut().expect("planned skill owner");
        owner.completed.par_extend(rows);
        owner.completed.par_sort_unstable_by_key(|row| row.job);
        owner.completed.dedup_by_key(|row| row.job);
    }
    pub(super) fn stage_skills(&self, finished: &[u64], tick: u64) -> Option<CoreSkills> {
        let owner = self.skills.as_ref()?;
        let practiced = &self.skill_practice_recipes;
        let mut completed = owner.completed.clone();
        completed.par_extend(self.plan_skill_completions(finished));
        let mut credits: Vec<_> = completed
            .into_par_iter()
            .filter(|row| !practiced.contains(&row.recipe))
            .map(|row| game_economy_skills::Completion {
                job: row.job,
                worker: row.worker,
                recipe: row.recipe,
                finished_at: tick,
            })
            .collect();
        credits.par_extend(
            owner
                .practiced
                .par_iter()
                .filter(|row| practiced.contains(&row.recipe))
                .map(|row| game_economy_skills::Completion {
                    job: row.job,
                    worker: row.worker,
                    recipe: row.recipe,
                    finished_at: tick,
                }),
        );
        if credits.is_empty()
            && owner.owner.events.is_empty()
            && owner.practiced.is_empty()
            && owner.completed.is_empty()
        {
            return None;
        }
        Some(CoreSkills {
            owner: owner.owner.prepare(&credits, tick),
            practiced: vec![],
            completed: vec![],
        })
    }
}
