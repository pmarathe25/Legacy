//! Personal aspirations and learning from actually witnessed requested work.
use super::*;

impl CoreHost {
    pub(super) fn stage_aspiration_observations(
        &self,
        requests: Option<&game_autowork::requests::Requests>,
        people: &[Person],
        tick: u64,
    ) -> Option<game_economy_goals::Dreams> {
        let owner = self
            .dreams
            .as_ref()
            .filter(|owner| !owner.learning.is_empty())?;
        let requests = requests?;
        let observations: Vec<_> = requests
            .memories
            .par_iter()
            .filter(|memory| memory.last_success_at == Some(tick))
            .filter(|memory| {
                people
                    .binary_search_by_key(&memory.observer, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0 && !people[i].controlled)
            })
            .map(|memory| (memory.observer, memory.recipe))
            .collect();
        if observations.is_empty() {
            return None;
        }
        let mut next = owner.clone();
        next.learn(&observations, tick);
        Some(next)
    }

    pub(super) fn stage_aspirations(
        &self,
        goals: &mut Option<game_goals::Goals>,
        requests: Option<&game_autowork::requests::Requests>,
        people: &[Person],
        family: Option<&super::family::FamilyFrame>,
        tick: u64,
    ) -> Result<Option<game_economy_goals::Dreams>, String> {
        let Some(owner) = &self.dreams else {
            return Ok(None);
        };
        let current = goals
            .as_ref()
            .or(self.goals.as_ref())
            .ok_or("aspirations require personal goals")?;
        let draft = owner
            .prepare(current, tick, |id| {
                people
                    .binary_search_by_key(&id, |p| p.id)
                    .ok()
                    .is_some_and(|i| people[i].health > 0 && !people[i].controlled)
            })
            .map_err(str::to_owned)?;
        let mut next = if let Some(draft) = draft {
            if let Some(updated) = draft.goals {
                *goals = Some(updated);
            }
            Some(draft.state)
        } else if !owner.events.is_empty() {
            let mut next = owner.clone();
            next.events.clear();
            Some(next)
        } else {
            None
        };
        if owner.inherit_births
            && !owner.learning.is_empty()
            && family
                .is_some_and(|frame| frame.owner.rules.enroll_births && !frame.newborns.is_empty())
        {
            let mut inherited = Some(next.take().unwrap_or_else(|| owner.clone()));
            crate::development::enroll_dream_learning(
                &mut inherited,
                family.map(|frame| &frame.owner),
                tick,
            );
            next = inherited;
        }
        if let Some(requests) = requests.filter(|_| !owner.learning.is_empty()) {
            let observations: Vec<_> = requests
                .memories
                .par_iter()
                .filter(|memory| memory.last_success_at == Some(tick))
                .filter(|memory| {
                    people
                        .binary_search_by_key(&memory.observer, |p| p.id)
                        .ok()
                        .is_some_and(|i| people[i].health > 0 && !people[i].controlled)
                })
                .map(|memory| (memory.observer, memory.recipe))
                .collect();
            if !observations.is_empty() {
                next.get_or_insert_with(|| owner.clone())
                    .learn(&observations, tick);
            }
        }
        Ok(next)
    }
}
