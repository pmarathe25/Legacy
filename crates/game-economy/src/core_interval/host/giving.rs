//! Personal requests and sharing move real possessions without binding either person.
use super::*;

pub(super) struct GivingFrame {
    pub owner: crate::Giving,
    pub plan: crate::giving::Plan,
    pub actors: Vec<u64>,
    pub contacts: Vec<(u64, u64)>,
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
        let practices = owner.practices.take();
        let mut actors = if owner.asking.is_empty()
            && owner.requests.is_empty()
            && owner
                .learning
                .par_iter()
                .all(|learning| learning.ask_helper.is_none())
            && practices
                .as_ref()
                .is_none_or(|owner| owner.minds.par_iter().all(|mind| !mind.ask_willing))
        {
            Vec::new()
        } else {
            owner.ask(
                &self.core.people,
                self.core.physical.goods(),
                &view,
                (&asking_busy, practices.as_ref(), &communication),
                tick,
                (&locations, None),
            )?
        };
        owner.practices = practices;
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
        actors.par_sort_unstable();
        actors.dedup();
        let contacts: Vec<_> = owner
            .requests
            .par_iter()
            .filter(|request| actors.binary_search(&request.requester).is_ok())
            .map(|request| (request.requester, request.target))
            .collect();
        busy.par_extend(actors.par_iter().copied());
        busy.par_sort_unstable();
        busy.dedup();
        let plan = crate::giving::Context {
            owner: Some(&owner),
            missions: None,
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            work_requests: self
                .autowork
                .as_ref()
                .and_then(|owner| owner.requests.as_ref()),
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
            contacts,
        }))
    }
    pub(super) fn stage_practices(
        &self,
        frame: &mut GivingFrame,
        contacts: &[(u64, u64)],
        events: &game_lot_runtime::Events,
        tick: u64,
    ) -> Result<(), String> {
        let Some(opening) = self.giving.as_ref() else {
            return Ok(());
        };
        let Some(practices) = opening.practices.as_ref() else {
            return Ok(());
        };
        let view = self.core.physical.view(tick)?;
        let locations = game_economy_person::travel::Colocation::new(self.core.travel.as_ref());
        let pairs: Vec<_> = contacts.par_iter().copied().collect();
        let mut encounters: Vec<_> = pairs
            .into_par_iter()
            .map(|(first, second)| -> Result<_, String> {
                let a = self
                    .core
                    .people
                    .binary_search_by_key(&first, |row| row.id)
                    .map_err(|_| "unknown practice contact")?;
                let b = self
                    .core
                    .people
                    .binary_search_by_key(&second, |row| row.id)
                    .map_err(|_| "unknown practice contact")?;
                let a = &self.core.people[a];
                let b = &self.core.people[b];
                let region = locations
                    .region((a.id, a.region), (b.id, b.region))
                    .ok_or("practice contact lost location")?;
                Ok(game_social::Encounter {
                    tick,
                    region,
                    a: first.min(second),
                    b: first.max(second),
                    pressured: None,
                    also_pressured: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if let Some(social) = self
            .social
            .as_ref()
            .filter(|owner| owner.last_tick == Some(tick - 1))
        {
            encounters.par_extend(social.owner.events.par_iter().cloned().map(|mut event| {
                event.tick = tick;
                event
            }));
        }
        encounters.par_sort_unstable_by_key(|event| (event.a, event.b));
        encounters.dedup_by_key(|event| (event.a, event.b));
        let practice_requests: Vec<_> = self
            .practice_work_asks
            .par_iter()
            .map(|ask| game_economy_work::PracticeRequest {
                requester: ask.requester,
                worker: ask.worker,
                recipe: ask.recipe,
                expires: ask.expires,
            })
            .collect();
        let (next, reactions) = crate::giving_practices::advance(
            practices,
            crate::giving_practices::Context {
                giving: opening,
                people: &self.core.people,
                goods: self.core.physical.goods(),
                view: &view,
                opinions: self.opinions.as_ref(),
                goals: self.goals.as_ref(),
                requests: self
                    .autowork
                    .as_ref()
                    .and_then(|owner| owner.requests.as_ref()),
                work_asks: &self.practice_work_asks,
                practice_requests: &practice_requests,
                started_work: &[],
                encounters: &encounters,
                gifts: &frame.plan,
                promise_deliveries: &[],
                events,
                tick,
            },
        )?;
        if !reactions.is_empty() {
            return Err("unexpected work-practice reaction".into());
        }
        frame.owner.practices = Some(next);
        Ok(())
    }
}
