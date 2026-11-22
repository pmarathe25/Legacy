//! Personal hunting choices and physical yield reservations.
use super::*;

#[derive(Default)]
pub(super) struct HuntRound {
    pub actors: Vec<u64>,
    pub yields: Vec<crate::hunting::Receipt>,
    pub deaths: Vec<game_fauna::AnimalDeath>,
}

impl CoreHost {
    pub(super) fn hunt_at_decision(&mut self, occupied: &[u64]) -> Result<HuntRound, String> {
        if self
            .hunting
            .as_ref()
            .is_none_or(|owner| !owner.elapsed_actions)
        {
            return Ok(HuntRound::default());
        }
        self.hunt_now(occupied, None)
    }

    /// Pursue one reachable animal chosen by the controlled actor.
    pub fn hunt_controlled(
        &mut self,
        person: u64,
        animal: u64,
    ) -> Result<Option<crate::hunting::Receipt>, String> {
        let actor = self.controlled_person(person)?;
        let region = actor.region.ok_or("the controlled hunter is in transit")?;
        if actor.health == 0
            || !self.regional_stock_reachable(person, region)
            || self
                .local_walk_exclusions(&[])
                .binary_search(&person)
                .is_ok()
            || self.held_at(person).is_some()
        {
            return Err("the controlled hunter is unavailable".into());
        }
        if self.core.next_deadline(None)?.after_millis == 0 {
            return Err("settle the current event before hunting".into());
        }
        Ok(self
            .hunt_now(&[], Some((person, animal)))?
            .yields
            .into_iter()
            .next())
    }

    fn hunt_now(
        &mut self,
        occupied: &[u64],
        manual: Option<(u64, u64)>,
    ) -> Result<HuntRound, String> {
        let tick = self.core.physical.tick();
        let view = self.core.physical.view(tick)?;
        let busy = self.local_walk_exclusions(occupied);
        let away = self.block_away_from_stock(&busy);
        let busy = away.as_deref().unwrap_or(&busy);
        let held = self.block_held(busy);
        let busy = held.as_deref().unwrap_or(busy);
        let excluded: Vec<_> =
            self.animal_restraints
                .as_ref()
                .map_or_else(Vec::new, |restraints| {
                    restraints
                        .holds
                        .par_iter()
                        .map(|hold| hold.animal)
                        .collect()
                });
        let key = self.core.decision_time_key();
        let plan = crate::hunting::Context {
            owner: manual.is_none().then_some(self.hunting.as_ref()).flatten(),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            requests: &[],
            busy,
            feed_targets: &excluded,
            max_hunger: u32::MAX,
            seed: self.core.seed,
            draw_key: key,
            tick,
            manual,
        }
        .prepare(1)?;
        let mut result = HuntRound::default();
        if plan.yields.is_empty() {
            result.actors = plan.actors;
            return Ok(result);
        }
        let custody = self
            .core
            .physical
            .prepare_interval_hunts(&plan.yields, self.core.seed ^ key)?;
        let hunted = plan.accepted(custody.transactions());
        if !hunted.is_empty() {
            let fauna = self
                .fauna
                .as_ref()
                .ok_or("missing wildlife owner")?
                .prepare_encounters(&hunted, &[])
                .map_err(str::to_owned)?;
            let feelings = self.stage_animal_feelings(Some(&plan), &hunted, &[], None, &[], None);
            result.yields =
                plan.yields
                    .par_iter()
                    .filter_map(|row| {
                        hunted.binary_search(&row.animal).is_ok().then_some(
                            crate::hunting::Receipt {
                                person: row.hunter,
                                animal: row.animal,
                                good: row.good,
                                quantity: row.quantity,
                            },
                        )
                    })
                    .collect();
            self.core.physical.validate_interval_consumption(&custody)?;
            self.core.physical.commit_interval_consumption(custody)?;
            let live = self.fauna.as_mut().expect("prepared wildlife");
            result.deaths = live.publish_encounter_receipts(fauna);
            if let Some(feelings) = feelings {
                self.animal_feelings = Some(feelings);
            }
        }
        result.actors = plan.actors;
        Ok(result)
    }

    pub(super) fn plan_hunting(
        &self,
        occupied: &[u64],
        cultivation: Option<&game_economy_agriculture::Plan>,
        irrigation: Option<&game_economy_agriculture::irrigation::Plan>,
        gathering: Option<&crate::gathering::Plan>,
        (care, restraints, handling): (
            Option<&crate::animal_care::Plan>,
            Option<&game_animal_restraint::Plan>,
            Option<&crate::animal_handling::Plan>,
        ),
        base: u64,
    ) -> Result<Option<crate::hunting::Plan>, String> {
        let Some(owner) = self.hunting.as_ref().filter(|owner| !owner.elapsed_actions) else {
            return Ok(None);
        };
        let rest = self.blocked_with_rest(occupied);
        let mut busy = rest.as_deref().unwrap_or(occupied).to_vec();
        if let Some(plan) = cultivation {
            busy.par_extend(
                plan.actors
                    .par_iter()
                    .chain(plan.requesters.par_iter())
                    .copied(),
            );
        }
        if let Some(plan) = irrigation {
            busy.par_extend(plan.parallel_actors());
        }
        if let Some(plan) = gathering {
            busy.par_extend(plan.actors.par_iter().copied());
        }
        if let Some(plan) = care {
            busy.par_extend(plan.actors.par_iter().copied());
        }
        let mut feed_targets = care.map_or_else(Vec::new, |plan| plan.targets());
        if let Some(plan) = restraints {
            busy.par_extend(plan.actors.par_iter().copied());
            feed_targets.par_extend(plan.reserved.par_iter().copied());
            feed_targets.par_sort_unstable();
            feed_targets.dedup();
        }
        if let Some(plan) = handling {
            busy.par_extend(plan.actors.par_iter().copied());
            feed_targets.par_extend(plan.events.par_iter().map(|event| event.animal));
            feed_targets.par_sort_unstable();
            feed_targets.dedup();
        }
        busy.par_extend(
            self.core
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase == game_lot_runtime::Phase::Working)
                .map(|row| row.job.worker),
        );
        busy.par_sort_unstable();
        busy.dedup();
        let tick = self
            .core
            .physical
            .tick()
            .checked_add(1)
            .ok_or("tick overflow")?;
        let view = self.core.physical.view(tick)?;
        crate::hunting::Context {
            owner: Some(owner),
            fauna: self.fauna.as_ref(),
            people: &self.core.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            requests: &[],
            busy: &busy,
            feed_targets: &feed_targets,
            max_hunger: self.core.rules.max_work_hunger,
            seed: self.core.seed,
            draw_key: tick,
            tick,
            manual: None,
        }
        .prepare(base)
        .map(Some)
    }
    pub(super) fn stage_animal_feelings(
        &self,
        plan: Option<&crate::hunting::Plan>,
        hunted: &[u64],
        feeding: &[game_fauna::Feeding],
        handling: Option<&crate::animal_handling::Plan>,
        product_handling: &[game_fauna::handling::Event],
        restraints: Option<&game_animal_restraint::Plan>,
    ) -> Option<crate::AnimalFeelings> {
        let owner = self.animal_feelings.as_ref()?;
        let mut events: Vec<_> = feeding
            .par_iter()
            .filter_map(|food| {
                food.person.map(|person| crate::animal_feelings::Event {
                    person,
                    animal: food.animal,
                    kind: crate::animal_feelings::Experience::Feed,
                })
            })
            .collect();
        if let Some(plan) = plan {
            events.par_extend(plan.yields.par_iter().filter_map(|yield_| {
                hunted.binary_search(&yield_.animal).is_ok().then_some(
                    crate::animal_feelings::Event {
                        person: yield_.hunter,
                        animal: yield_.animal,
                        kind: crate::animal_feelings::Experience::Hunt,
                    },
                )
            }));
        }
        if let Some(plan) = handling {
            events.par_extend(plan.events.par_iter().filter_map(|event| {
                let kind = match event.outcome {
                    crate::AnimalHandlingOutcome::Accepted => {
                        crate::animal_feelings::Experience::HandleAccepted
                    }
                    crate::AnimalHandlingOutcome::Refused => {
                        crate::animal_feelings::Experience::HandleRefused
                    }
                    crate::AnimalHandlingOutcome::Unavailable => return None,
                };
                Some(crate::animal_feelings::Event {
                    person: event.person,
                    animal: event.animal,
                    kind,
                })
            }));
        }
        events.par_extend(product_handling.par_iter().filter_map(|event| {
            let kind = match event.outcome {
                game_fauna::handling::Outcome::Accepted => {
                    crate::animal_feelings::Experience::HandleAccepted
                }
                game_fauna::handling::Outcome::Refused => {
                    crate::animal_feelings::Experience::HandleRefused
                }
                game_fauna::handling::Outcome::Unavailable => return None,
            };
            Some(crate::animal_feelings::Event {
                person: event.person,
                animal: event.animal,
                kind,
            })
        }));
        if let Some(state) = restraints.and_then(|plan| plan.state.as_ref()) {
            events.par_extend(state.events.par_iter().flat_map_iter(|event| {
                use crate::animal_feelings::Experience;
                use game_animal_restraint::EventKind;
                let actor = event.actor.and_then(|person| {
                    let kind = match event.kind {
                        EventKind::Yielded => Experience::CaptureYielded,
                        EventKind::Forced => Experience::CaptureForced,
                        EventKind::Resisted => Experience::CaptureResisted,
                        EventKind::Released => Experience::Released,
                        EventKind::ReleaseBlocked => Experience::ReleaseBlocked,
                        _ => return None,
                    };
                    Some(crate::animal_feelings::Event {
                        person,
                        animal: event.animal,
                        kind,
                    })
                });
                let defender = event.defender.and_then(|person| {
                    let kind = match event.kind {
                        EventKind::ReleaseBlocked => Experience::DefenseSucceeded,
                        EventKind::Released => Experience::DefenseOvercome,
                        _ => return None,
                    };
                    Some(crate::animal_feelings::Event {
                        person,
                        animal: event.animal,
                        kind,
                    })
                });
                [actor, defender].into_iter().flatten()
            }));
        }
        let events = owner.relevant(events);
        (!events.is_empty()).then(|| owner.advance(&events))
    }
}
