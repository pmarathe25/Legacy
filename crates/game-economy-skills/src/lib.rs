//! Work and teaching skill over actual people, jobs and encounters.
use game_economy_campaign::Campaign;
use game_economy_person::Person;
use game_families::Families;
use game_lot_runtime::{Events, Work};
use game_opinions::Opinions;
use game_skills::Completion;
pub use game_skills::{Record, Rule, Skills, TeacherPolicy};
use game_social::Social;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
#[macro_export]
macro_rules! continuing_work {
    ($world:expr, $meals:expr, $offers:expr) => {{
        let pressured = $world.autowork.as_ref().map_or_else(Vec::new, |auto| {
            game_economy_work::pressured_workers(auto, $world.opinions.as_ref(), $offers.as_ref())
        });
        let (mut rates, abandon) = game_economy_person::occupation::continuing(
            $world.physical.work(),
            &$world.people,
            &pressured,
            &$meals,
            $world.rules.max_work_hunger,
        );
        $crate::rates(
            $world.skills.as_ref(),
            $world.campaign.as_ref(),
            $world.physical.work(),
            &mut rates,
        );
        (rates, abandon)
    }};
}
#[macro_export]
macro_rules! validate_world {
    ($state:expr, $physical:expr, $ids:expr) => {
        if $state.skills.is_some() {
            let recipes = $physical.recipes().iter().map(|r| r.id).collect();
            $crate::validate($state.skills.as_ref(), &$ids, &recipes, $physical.tick())?;
        }
    };
}

pub fn validate(
    skills: Option<&Skills>,
    people: &HashSet<u64>,
    recipes: &HashSet<u32>,
    tick: u64,
) -> Result<(), String> {
    if let Some(skills) = skills {
        skills.validate(people, recipes, tick)?;
    }
    Ok(())
}
/// Read trained rates from the old generation, including only actively invoked lives.
pub fn rates(
    skills: Option<&Skills>,
    campaign: Option<&Campaign>,
    work: &[Work],
    rates: &mut [(u64, u64)],
) {
    let Some(skills) = skills else {
        return;
    };
    let index = skills.index();
    let invocation = campaign.and_then(|c| c.lineage.invocation.as_ref());
    rates.par_iter_mut().for_each(|(job, rate)| {
        let Some(work) = work
            .binary_search_by_key(job, |w| w.job.id)
            .ok()
            .map(|i| &work[i])
        else {
            return;
        };
        let own = index.rate(work.job.worker, work.job.recipe);
        *rate = invocation
            .filter(|i| i.owner == Some(work.job.worker))
            .map_or(own, |i| {
                i.active
                    .iter()
                    .map(|&life| index.rate(life, work.job.recipe))
                    .fold(own, u64::max)
            });
    });
}
pub struct WorkRates<'a> {
    pub lead: &'a [(u64, u64)],
    pub helpers: &'a [(u64, u64, u64)],
}
pub fn advance(
    old: Option<&Skills>,
    work: &[Work],
    work_rates: WorkRates<'_>,
    physical: &Events,
    social: (Option<&Social>, Option<&Opinions>),
    people: &[Person],
    tick: u64,
) -> Option<Skills> {
    let old = old?;
    let WorkRates {
        lead: rates,
        helpers,
    } = work_rates;
    let (social, opinions) = social;
    let practicing: HashSet<_> = old
        .rules
        .iter()
        .filter(|rule| rule.practice_each_tick)
        .map(|rule| rule.recipe)
        .collect();
    let aliases: HashMap<_, _> = old
        .aliases
        .iter()
        .map(|alias| (alias.recipe, alias.canonical))
        .collect();
    let uses_practice = |recipe| practicing.contains(aliases.get(&recipe).unwrap_or(&recipe));
    let mut completions: Vec<_> = physical
        .finished
        .par_iter()
        .filter_map(|id| {
            let row = work
                .binary_search_by_key(id, |w| w.job.id)
                .ok()
                .map(|i| &work[i])?;
            if uses_practice(row.job.recipe) {
                return None;
            }
            Some(Completion {
                job: *id,
                worker: row.job.worker,
                recipe: row.job.recipe,
                finished_at: tick,
            })
        })
        .collect();
    if !practicing.is_empty() {
        completions.par_extend(rates.par_iter().filter_map(|&(id, rate)| {
            let row = work
                .binary_search_by_key(&id, |row| row.job.id)
                .ok()
                .map(|i| &work[i])?;
            (rate > 0 && row.job.remaining > 0 && uses_practice(row.job.recipe)).then_some(
                Completion {
                    job: id,
                    worker: row.job.worker,
                    recipe: row.job.recipe,
                    finished_at: tick,
                },
            )
        }));
        completions.par_extend(helpers.par_iter().filter_map(|&(id, person, progress)| {
            let row = work
                .binary_search_by_key(&id, |row| row.job.id)
                .ok()
                .map(|i| &work[i])?;
            (progress > 0 && uses_practice(row.job.recipe)).then_some(Completion {
                job: id,
                worker: person,
                recipe: row.job.recipe,
                finished_at: tick,
            })
        }));
    }
    let pairs: Vec<_> = social.map_or_else(Vec::new, |s| {
        s.events
            .par_iter()
            .filter(|e| {
                [e.a, e.b].into_iter().all(|id| {
                    people
                        .binary_search_by_key(&id, |p| p.id)
                        .ok()
                        .is_some_and(|i| !people[i].controlled)
                })
            })
            .map(|e| (e.a, e.b))
            .collect()
    });
    let mut pressured: Vec<_> = social.map_or_else(Vec::new, |social| {
        social
            .events
            .par_iter()
            .flat_map_iter(|event| {
                event
                    .pressured_people()
                    .map(|person| (event.a, event.b, person))
            })
            .collect()
    });
    // A person may willingly meet but still teach only because of their own
    // fear or obligation toward the other participant.
    if !pairs.is_empty()
        && !old.teachers.is_empty()
        && let (Some(social), Some(opinions)) = (social, opinions)
    {
        let mut policies: Vec<_> = social
            .pressures
            .par_iter()
            .filter(|row| row.enabled)
            .collect();
        if !policies.is_empty() {
            policies.par_sort_unstable_by_key(|row| row.person);
            let feelings = opinions.index();
            pressured.par_extend(
                pairs
                    .par_iter()
                    .flat_map_iter(|&(a, b)| [(a, b), (b, a)])
                    .filter_map(|(teacher, student)| {
                        let policy = policies
                            .binary_search_by_key(&teacher, |row| row.person)
                            .ok()
                            .map(|i| policies[i])?;
                        let (fear, obligation) = feelings.feelings(teacher, student);
                        policy.responds(fear, obligation).then_some((
                            teacher.min(student),
                            teacher.max(student),
                            teacher,
                        ))
                    }),
            );
        }
    }
    pressured.par_sort_unstable();
    let teaching = if pressured.is_empty()
        && old
            .teachers
            .par_iter()
            .all(|p| !p.enabled || p.eligible_at > tick)
    {
        None
    } else if pressured.is_empty() {
        old.plan_teaching(&pairs, None, tick)
    } else {
        old.plan_teaching_with(&pairs, None, tick, |teacher, student, enabled| {
            enabled
                || pressured
                    .binary_search(&(teacher.min(student), teacher.max(student), teacher))
                    .is_ok()
        })
    };
    let mut next = old.prepare(&completions, tick);
    if let Some(teaching) = teaching {
        teaching.commit(&mut next);
    }
    Some(next)
}
/// Newborns inherit enabled teaching inclinations, not records or ability.
pub fn inherit(
    skills: &mut Option<Skills>,
    family: &Families,
    newborns: &[u64],
    tick: u64,
) -> Result<(), String> {
    let Some(skills) = skills
        .as_mut()
        .filter(|s| s.inherit_teachers && !newborns.is_empty())
    else {
        return Ok(());
    };
    let eligible_at = tick
        .checked_add(family.rules.adult_age)
        .ok_or("teaching eligibility overflow")?;
    let mut teachers: Vec<_> = skills.teachers.par_iter().filter(|p| p.enabled).collect();
    teachers.par_sort_unstable_by_key(|p| (p.person, p.recipe));
    let mut members: Vec<_> = family.members.par_iter().collect();
    members.par_sort_unstable_by_key(|m| m.id);
    let additions: Vec<_> = newborns
        .par_iter()
        .flat_map_iter(|id| {
            let Some(member) = members
                .binary_search_by_key(id, |m| m.id)
                .ok()
                .map(|i| members[i])
            else {
                return Vec::new();
            };
            let mut recipes = Vec::new();
            for &parent in &member.parents {
                let a = teachers.partition_point(|p| p.person < parent);
                let b = teachers.partition_point(|p| p.person <= parent);
                recipes.extend(teachers[a..b].iter().map(|p| p.recipe));
            }
            recipes.sort_unstable();
            recipes.dedup();
            recipes
                .into_iter()
                .map(|recipe| TeacherPolicy {
                    person: *id,
                    recipe,
                    enabled: true,
                    eligible_at,
                })
                .collect()
        })
        .collect();
    skills.teachers.par_extend(additions);
    Ok(())
}
