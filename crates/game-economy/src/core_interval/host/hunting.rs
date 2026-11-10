//! Personal hunting choices and physical yield reservations.
use super::*;

impl CoreHost {
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
        let Some(owner) = &self.hunting else {
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
