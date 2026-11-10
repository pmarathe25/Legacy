//! Individual taking proposals settle against actual possessions.
use super::*;

pub(super) struct TakingFrame {
    pub owner: game_economy_taking::Taking,
    pub plan: game_economy_taking::Plan,
}

impl CoreHost {
    pub(super) fn plan_taking(
        &self,
        occupied: &[u64],
        base: u64,
    ) -> Result<Option<TakingFrame>, String> {
        let Some(owner) = &self.taking else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|work| work.phase == game_lot_runtime::Phase::Working)
                .map(|work| work.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let plan = game_economy_taking::Context {
            owner,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            locations: &locations,
            busy: &busy,
            blocked: occupied,
            can_touch: &|_, _| true,
            max_hunger: self.core.rules.max_work_hunger,
            manual: None,
            action_base: base,
            seed: self.core.seed,
            tick,
        }
        .prepare()?;
        Ok(Some(TakingFrame {
            owner: owner.clone(),
            plan,
        }))
    }
    pub(super) fn stage_taking_reactions(
        &self,
        owner: &mut game_economy_taking::Taking,
    ) -> Vec<game_opinions::PersonalChange> {
        let mut changes = owner.personal_changes(&owner.events);
        if !owner.witnesses.is_empty() && !owner.events.is_empty() {
            let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
            let can_observe = |observer, actor| {
                let find = |id| {
                    self.core
                        .people
                        .binary_search_by_key(&id, |person| person.id)
                        .ok()
                        .map(|index| &self.core.people[index])
                };
                find(observer).zip(find(actor)).is_some_and(|(a, b)| {
                    a.health > 0
                        && b.health > 0
                        && locations
                            .region((a.id, a.region), (b.id, b.region))
                            .is_some()
                })
            };
            let events = std::mem::take(&mut owner.events);
            changes.par_extend(owner.observe_witnesses(&events, &can_observe));
            owner.events = events;
        }
        changes
    }
    /// Testimony uses opening memories and actual contacts. The listener owns belief.
    pub(super) fn stage_taking_reports(
        &self,
        owner: &mut game_economy_taking::Taking,
        contacts: &[(u64, u64)],
        tick: u64,
    ) -> Vec<game_opinions::PersonalChange> {
        let Some(previous) = self
            .taking
            .as_ref()
            .filter(|previous| !previous.report_policies.is_empty() && !contacts.is_empty())
        else {
            return Vec::new();
        };
        let feelings = self.opinions.as_ref().map(game_opinions::Opinions::index);
        owner.receive_reports(previous, contacts, tick, &|listener, speaker| {
            feelings.as_ref().map_or((0, 0, 0), |index| {
                let (fear, obligation) = index.feelings(speaker, listener);
                (index.trust(listener, speaker), fear, obligation)
            })
        })
    }
}
