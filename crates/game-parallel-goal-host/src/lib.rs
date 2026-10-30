//! Validate personal goal changes against authoritative fused state.
use game_parallel::{Dream, Goal, SetDream, SetGoal, World};
use game_parallel_fused_data::Block;
pub fn set(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    request: SetGoal,
) -> Result<(), String> {
    check_actor(world, blocks, positions, request.person)?;
    if request.target == 0
        || !world.production.as_ref().is_some_and(|production| {
            production
                .recipes
                .iter()
                .any(|recipe| recipe.id == request.recipe)
        })
    {
        return Err("invalid goal target or recipe".into());
    }
    let goals = world.goals.as_mut().ok_or("goals are disabled")?;
    if let Some(goal) = goals
        .people
        .iter_mut()
        .find(|goal| goal.person == request.person && goal.recipe == request.recipe)
    {
        if request.target < goal.progress {
            return Err("target is below completed progress".into());
        }
        goal.priority = request.priority;
        goal.target = request.target;
        goal.enabled = request.enabled;
        goal.next_review = 0;
    } else {
        goals.people.push(Goal {
            person: request.person,
            recipe: request.recipe,
            priority: request.priority,
            target: request.target,
            progress: 0,
            next_review: 0,
            enabled: request.enabled,
        });
    }
    Ok(())
}
fn check_actor(
    world: &World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    person: u64,
) -> Result<(), String> {
    if world
        .lineage
        .as_ref()
        .and_then(|lineage| lineage.suspended_person(world.tick))
        == Some(person)
    {
        return Err("player control is suspended".into());
    }
    game_parallel_fused_conflict::check(world, &[person])?;
    let index = positions
        .binary_search_by_key(&person, |row| row.0)
        .map_err(|_| "unknown person")?;
    let (_, block, offset) = positions[index];
    if blocks[block].health[offset] == 0 {
        return Err("person is dead".into());
    }
    Ok(())
}
/// Change an aspiration without changing an existing work goal or its progress.
pub fn set_dream(
    world: &mut World,
    blocks: &[Block],
    positions: &[(u64, usize, usize)],
    request: SetDream,
) -> Result<(), String> {
    check_actor(world, blocks, positions, request.person)?;
    if request.batch == 0
        || request.review_interval == 0
        || !world.production.as_ref().is_some_and(|production| {
            production
                .recipes
                .iter()
                .any(|recipe| recipe.id == request.recipe)
        })
    {
        return Err("invalid aspiration or recipe".into());
    }
    let dreams = world.dreams.as_mut().ok_or("aspirations are disabled")?;
    let replacement = Dream {
        person: request.person,
        recipe: request.recipe,
        priority: request.priority,
        batch: request.batch,
        review_interval: request.review_interval,
        next_review: world.tick,
        enabled: request.enabled,
    };
    if let Some(existing) = dreams
        .people
        .iter_mut()
        .find(|dream| dream.person == request.person && dream.recipe == request.recipe)
    {
        *existing = replacement;
    } else {
        dreams.people.push(replacement);
    }
    Ok(())
}
/// Attach the checked command to the fused host, preserving generation invalidation.
#[macro_export]
macro_rules! fused_goal_methods {
    () => {
        pub fn set_dream(&mut self, request: game_parallel::SetDream) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            game_parallel_goal_host::set_dream(
                &mut self.template,
                &self.blocks,
                &self.positions,
                request,
            )?;
            self.revision = revision;
            Ok(())
        }
        pub fn set_goal(&mut self, request: game_parallel::SetGoal) -> Result<(), String> {
            let revision = self.revision.checked_add(1).ok_or("generation overflow")?;
            game_parallel_goal_host::set(
                &mut self.template,
                &self.blocks,
                &self.positions,
                request,
            )?;
            self.revision = revision;
            Ok(())
        }
    };
}
