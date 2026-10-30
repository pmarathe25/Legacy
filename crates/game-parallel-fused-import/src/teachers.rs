//! Enroll newborns in their parents' opt-in teaching roles without inheriting skill.
use game_parallel::{SkillTeacherPolicy, Skills};
use rayon::prelude::*;

pub fn enroll(skills: &mut Skills, births: &[(u64, [u64; 2], usize)], tick: u64, adult_age: u64) {
    if !skills.inherit_teachers || births.is_empty() {
        return;
    }
    let mut policies: Vec<_> = skills
        .teachers
        .par_iter()
        .filter(|policy| policy.enabled)
        .collect();
    if policies.is_empty() {
        return;
    }
    if policies
        .par_windows(2)
        .any(|pair| (pair[0].person, pair[0].recipe) > (pair[1].person, pair[1].recipe))
    {
        policies.par_sort_unstable_by_key(|policy| (policy.person, policy.recipe));
    }
    let eligible_at = tick.saturating_add(adult_age);
    let rows: Vec<_> = births
        .par_iter()
        .flat_map_iter(|&(person, parents, count)| {
            let mut recipes = Vec::new();
            for &parent in parents.iter().take(count) {
                let start = policies.partition_point(|policy| policy.person < parent);
                let end = policies.partition_point(|policy| policy.person <= parent);
                recipes.extend(policies[start..end].iter().map(|policy| policy.recipe));
            }
            recipes.sort_unstable();
            recipes.dedup();
            recipes.into_iter().map(move |recipe| SkillTeacherPolicy {
                person,
                recipe,
                enabled: true,
                eligible_at,
            })
        })
        .collect();
    skills.teachers.par_extend(rows);
}

#[cfg(test)]
#[path = "../tests/support/teachers.rs"]
mod tests;
