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

pub(super) struct TeachingFrame {
    pub skills: CoreSkills,
    pub lessons: Vec<(u64, u64)>,
    pub rate_changed: bool,
}

impl CoreSkills {
    fn parallel_copy(&self) -> Self {
        let original = &self.owner;
        Self {
            owner: game_economy_skills::Skills {
                rules: original.rules.clone(),
                aliases: original.aliases.clone(),
                catalog_training: original.catalog_training.clone(),
                records: original.records.par_iter().cloned().collect(),
                events: original.events.par_iter().cloned().collect(),
                lessons: original.lessons.par_iter().cloned().collect(),
                teachers: original.teachers.par_iter().cloned().collect(),
                inherit_teachers: original.inherit_teachers,
            },
            practiced: self.practiced.par_iter().copied().collect(),
            completed: self.completed.par_iter().copied().collect(),
        }
    }
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
    /// A real meeting can become a lesson only through a person's own choice.
    pub(super) fn stage_skill_teaching(
        &self,
        social: &game_social::Social,
        tick: u64,
    ) -> Option<TeachingFrame> {
        let owner = self.skills.as_ref()?;
        if owner.owner.teachers.is_empty() || social.events.is_empty() {
            return None;
        }
        let pairs: Vec<_> = social
            .events
            .par_iter()
            .filter(|event| {
                [event.a, event.b].into_iter().all(|person| {
                    self.core
                        .people
                        .binary_search_by_key(&person, |row| row.id)
                        .ok()
                        .is_some_and(|index| {
                            !self.core.people[index].controlled
                                && self.core.people[index].health > 0
                        })
                })
            })
            .map(|event| (event.a, event.b))
            .collect();
        if pairs.is_empty() {
            return None;
        }
        let mut pressured: Vec<_> = social
            .events
            .par_iter()
            .flat_map_iter(|event| {
                event
                    .pressured_people()
                    .map(|person| (event.a, event.b, person))
            })
            .collect();
        if let Some(opinions) = self
            .opinions
            .as_ref()
            .filter(|_| social.pressures.par_iter().any(|policy| policy.enabled))
        {
            let mut policies: Vec<_> = social
                .pressures
                .par_iter()
                .filter(|policy| policy.enabled)
                .collect();
            policies.par_sort_unstable_by_key(|policy| policy.person);
            let feelings = opinions.index();
            pressured.par_extend(
                pairs
                    .par_iter()
                    .flat_map_iter(|&(a, b)| [(a, b), (b, a)])
                    .filter_map(|(teacher, student)| {
                        let policy = policies
                            .binary_search_by_key(&teacher, |policy| policy.person)
                            .ok()
                            .map(|index| policies[index])?;
                        let (fear, obligation) = feelings.feelings(teacher, student);
                        policy.responds(fear, obligation).then_some((
                            teacher.min(student),
                            teacher.max(student),
                            teacher,
                        ))
                    }),
            );
        }
        pressured.par_sort_unstable();
        pressured.dedup();
        let plan = owner.owner.plan_teaching_with_index(
            &pairs,
            None,
            tick,
            self.skill_rates.as_ref()?,
            |teacher, student, enabled| {
                enabled
                    || pressured
                        .binary_search(&(teacher.min(student), teacher.max(student), teacher))
                        .is_ok()
            },
        )?;
        let mut next = owner.parallel_copy();
        next.owner.events = owner
            .owner
            .events
            .par_iter()
            .filter(|event| event.tick == tick)
            .cloned()
            .collect();
        let previous = next.owner.lessons.len();
        let previous_events = next.owner.events.len();
        plan.commit(&mut next.owner);
        let rate_changed = next.owner.events.len() != previous_events;
        let learned = next.owner.lessons[previous..]
            .par_iter()
            .map(|lesson| (lesson.teacher, lesson.student))
            .collect();
        Some(TeachingFrame {
            skills: next,
            lessons: learned,
            rate_changed,
        })
    }
    pub(super) fn stage_skill_births(
        &self,
        staged: Option<CoreSkills>,
        family: Option<&super::family::FamilyFrame>,
        tick: u64,
    ) -> Result<Option<CoreSkills>, String> {
        let Some(family) =
            family.filter(|family| family.owner.rules.enroll_births && !family.newborns.is_empty())
        else {
            return Ok(staged);
        };
        let Some(original) = self
            .skills
            .as_ref()
            .filter(|owner| owner.owner.inherit_teachers)
        else {
            return Ok(staged);
        };
        let mut next = staged.unwrap_or_else(|| original.parallel_copy());
        let mut owner = Some(next.owner);
        game_economy_skills::inherit(&mut owner, &family.owner, &family.newborns, tick)?;
        next.owner = owner.expect("staged skill owner");
        Ok(Some(next))
    }
}
