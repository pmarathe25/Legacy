//! Personal requests and sharing move real possessions without binding either person.
use super::*;

pub(super) struct GivingFrame {
    pub owner: crate::Giving,
    pub plan: crate::giving::Plan,
    pub actors: Vec<u64>,
}

impl CoreHost {
    pub(super) fn plan_giving(
        &self,
        external: &[u64],
        occupied: &[u64],
        dependents: &[u64],
        medical: Option<&crate::medical::Plan>,
        base: u64,
    ) -> Result<Option<GivingFrame>, String> {
        let Some(owner) = &self.giving else {
            return Ok(None);
        };
        let mut owner = owner.clone();
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
        let free_children: Vec<_> = dependents
            .par_iter()
            .copied()
            .filter(|person| {
                external.binary_search(person).is_err()
                    && medical.is_none_or(|plan| plan.actors.binary_search(person).is_err())
            })
            .collect();
        let mut asking_busy = game_economy_work::exclude_sorted(&busy, &free_children);
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let communication = game_economy_confinement::Communication {
            before: None,
            after: None,
        };
        let mut actors = if owner.asking.is_empty()
            && owner.requests.is_empty()
            && owner
                .learning
                .par_iter()
                .all(|learning| learning.ask_helper.is_none())
        {
            Vec::new()
        } else {
            owner.ask(
                &self.core.people,
                self.core.physical.goods(),
                &view,
                (&asking_busy, None, &communication),
                tick,
                (&locations, None),
            )?
        };
        asking_busy.par_extend(actors.par_iter().copied());
        asking_busy.par_sort_unstable();
        asking_busy.dedup();
        let relationships = self
            .social
            .as_ref()
            .map_or(&[][..], |social| social.owner.relationships.as_slice());
        let discovered = crate::giving_discovery::prepare(
            &mut owner,
            crate::giving_discovery::Context {
                people: &self.core.people,
                goods: self.core.physical.goods(),
                view: &view,
                busy: &asking_busy,
                relationships,
                opinions: self.opinions.as_ref(),
                locations: &locations,
                walking: None,
                communication: &communication,
                seed: self.core.seed,
                tick,
            },
        )?;
        actors.par_extend(discovered.into_par_iter());
        busy.par_extend(actors.par_iter().copied());
        busy.par_sort_unstable();
        busy.dedup();
        let plan = crate::giving::Context {
            owner: Some(&owner),
            missions: None,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            work_requests: None,
            goods: self.core.physical.goods(),
            view: &view,
            busy: &busy,
            communication: &communication,
            manual: None,
            tick,
            seed: self.core.seed,
            locations: &locations,
            walking: None,
        }
        .prepare(base)?;
        actors.par_extend(plan.actors.par_iter().copied());
        actors.par_sort_unstable();
        actors.dedup();
        Ok(Some(GivingFrame {
            owner,
            plan,
            actors,
        }))
    }
}
