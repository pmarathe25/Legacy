//! Control publication and immediate continuation follow the validated closing
//! survival responses, including a lifespan death earlier in the effect order.
use crate::{ControlEffect, ControlReceipt};
use game_core::{Error, PersonId};
use std::collections::BTreeMap;

/// Validates the projected physical facts before Invocation captures a death.
pub fn validate_death_body(alive: bool, health: u64) -> Result<(), Error> {
    if alive || health != 0 {
        return Err(Error::Unavailable);
    }
    Ok(())
}

/// Death cleanup may only be applied after the physical owner marks the body dead.
pub fn validate_death_cleanup(alive: bool) -> Result<(), Error> {
    if alive {
        return Err(Error::Unavailable);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClosingLifespan {
    Unchanged,
    Survives,
    Died,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClosingSurvival {
    pub alive: bool,
    pub lifespan: ClosingLifespan,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosingControl {
    pub effects: Vec<ControlEffect>,
    pub review_controller: bool,
    pub observe_deaths: bool,
}
fn survival(
    actor: PersonId,
    rows: &mut BTreeMap<PersonId, ClosingSurvival>,
    facts: &mut impl FnMut(PersonId) -> Result<ClosingSurvival, Error>,
) -> Result<ClosingSurvival, Error> {
    if let Some(row) = rows.get(&actor) {
        return Ok(*row);
    }
    let row = facts(actor)?;
    rows.insert(actor, row);
    Ok(row)
}
impl ControlReceipt {
    pub fn closing_outcome(
        &self,
        effects: Vec<ControlEffect>,
        mut facts: impl FnMut(PersonId) -> Result<ClosingSurvival, Error>,
    ) -> Result<ClosingControl, Error> {
        let mut rows = BTreeMap::new();
        let mut publications = Vec::new();
        let mut review_controller = false;
        for effect in effects {
            match &effect {
                ControlEffect::LifespanChanged { actor } => {
                    let mut row = survival(*actor, &mut rows, &mut facts)?;
                    if row.lifespan == ClosingLifespan::Died {
                        row.alive = false;
                    }
                    rows.insert(*actor, row);
                }
                ControlEffect::ControlChanged {
                    actor, controlled, ..
                } => {
                    if !survival(*actor, &mut rows, &mut facts)?.alive {
                        continue;
                    }
                    review_controller |= !controlled;
                }
                _ => {}
            }
            publications.push(effect);
        }
        let observe_deaths = match self.actor_after() {
            Some(actor) => !survival(actor, &mut rows, &mut facts)?.alive,
            None => false,
        };
        Ok(ClosingControl {
            effects: publications,
            review_controller,
            observe_deaths,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InvocationCost;
    use crate::memory_start::tests::{ACTOR, fixture};
    use crate::{AdmissionFacts, ControlAction, ControlEffect};
    use game_core::SimTime;
    use game_ecs::Batch;

    #[test]
    fn death_body_and_cleanup_require_the_owner_death_facts() {
        assert!(validate_death_body(false, 0).is_ok());
        assert!(validate_death_body(true, 0).is_err());
        assert!(validate_death_body(false, 1).is_err());
        assert!(validate_death_body(true, 1).is_err());
        assert!(validate_death_cleanup(false).is_ok());
        assert!(validate_death_cleanup(true).is_err());
    }

    #[test]
    fn closing_survival_orders_death_before_handoff_and_preserves_accepted_costs() {
        let (runtime, store, _) = fixture();
        let facts = AdmissionFacts {
            actor: ACTOR,
            now: SimTime(0),
            current: Some(ACTOR),
            alive: true,
            health: 100_000,
            independent: true,
            learning: 100,
            required_learning: 100,
            fatigue: 0,
            fatigue_limit: 100_000,
            archived: runtime.available_lives::<u8>(&store).into_iter().collect(),
        };
        let plan = runtime
            .control
            .prepare_review(&store, &facts, None)
            .unwrap();
        let mut batch = Batch::new();
        let receipt = plan
            .stage_into(&runtime.control, &store, &mut batch)
            .unwrap();
        let cost = ControlEffect::CostApplied {
            actor: ACTOR,
            cost: InvocationCost::Lifespan,
            severity: 1,
            permanent: true,
        };
        let handoff = ControlEffect::ControlChanged {
            actor: ACTOR,
            controlled: false,
            until: Some(SimTime(100)),
        };
        let death = ControlEffect::LifespanChanged { actor: ACTOR };
        let source = |_| {
            Ok(ClosingSurvival {
                alive: true,
                lifespan: ClosingLifespan::Died,
            })
        };
        let closing = receipt
            .closing_outcome(vec![cost.clone(), death.clone(), handoff.clone()], source)
            .unwrap();
        assert_eq!(closing.effects, vec![cost.clone(), death.clone()]);
        assert!(!closing.review_controller);
        assert!(closing.observe_deaths);
        let closing = receipt
            .closing_outcome(vec![handoff.clone(), cost.clone(), death.clone()], source)
            .unwrap();
        assert_eq!(closing.effects, vec![handoff.clone(), cost, death]);
        assert!(closing.review_controller);
        assert!(closing.observe_deaths);
        let alive = receipt
            .closing_outcome(vec![handoff.clone()], |_| {
                Ok(ClosingSurvival {
                    alive: true,
                    lifespan: ClosingLifespan::Survives,
                })
            })
            .unwrap();
        assert_eq!(alive.effects, vec![handoff]);
        assert!(alive.review_controller);
        assert!(!alive.observe_deaths);
        let dead = receipt
            .closing_outcome(alive.effects, |_| {
                Ok(ClosingSurvival {
                    alive: false,
                    lifespan: ClosingLifespan::Unchanged,
                })
            })
            .unwrap();
        assert!(dead.effects.is_empty());
        assert!(!dead.review_controller);
        assert!(dead.observe_deaths);
        let dismissal = runtime
            .control
            .prepare_action(&store, ACTOR, &ControlAction::Dismiss, &facts, None)
            .unwrap();
        let mut batch = Batch::new();
        let dismissed = dismissal
            .stage_into(&runtime.control, &store, &mut batch)
            .unwrap();
        assert!(
            !dismissed
                .closing_outcome(vec![], |_| panic!(
                    "dismissed owner has no survival dependency"
                ))
                .unwrap()
                .observe_deaths
        );
    }
}

#[cfg(test)]
mod composed_tests {
    use crate::*;
    use game_core::*;
    use game_ecs::{Batch, Schema, Store};
    use std::collections::BTreeSet;

    #[test]
    fn actual_lifespan_receiver_death_preempts_an_accepted_activation_handoff() {
        let actor = PersonId(10);
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        game_physiology::register_components(&mut schema).unwrap();
        game_lifecycle::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let control = ControlState::new(Rules::fixture()).unwrap();
        let lifecycle = game_lifecycle::Runtime {
            rules: game_lifecycle::Rules::human(),
        };
        game_physiology::insert_body(
            &mut store,
            actor,
            game_physiology::Body {
                vitals: Vitals {
                    hunger: 0,
                    fatigue: 0,
                    health: 100_000,
                    alive: true,
                },
                fatigue_remainder: 0,
                health_remainder: 0,
            },
        )
        .unwrap();
        game_lifecycle::insert_life(
            &mut store,
            game_lifecycle::Life {
                id: actor,
                born: SimTime(0),
                initial_age: SimDuration(lifecycle.rules.lifespan.0 - 1000),
                stage: LifeStage::Elder,
                reproduction: game_lifecycle::Reproduction::Neither,
                parents: vec![],
                caregivers: BTreeSet::new(),
                preference: None,
            },
        )
        .unwrap();
        insert_burden(
            &mut store,
            actor,
            Burden {
                burden: 650_000,
                exposure: 650_000,
                uninterrupted: 650_000,
                ..Burden::default()
            },
        )
        .unwrap();
        let facts = AdmissionFacts {
            actor,
            now: SimTime(0),
            current: Some(actor),
            alive: true,
            health: 100_000,
            independent: true,
            learning: 100,
            required_learning: 100,
            fatigue: 0,
            fatigue_limit: 100_000,
            archived: BTreeSet::from([PersonId(11)]),
        };
        control
            .prepare_action(
                &store,
                actor,
                &ControlAction::Activate {
                    lives: facts.archived.clone(),
                },
                &facts,
                Some(WarningId(100)),
            )
            .unwrap()
            .commit(&control, &mut store)
            .unwrap();
        for next_id in 101..110 {
            let pending = control.pending_consent(&store).unwrap().id;
            let prepared = control
                .prepare_action(
                    &store,
                    actor,
                    &ControlAction::AcceptWarning { warning: pending },
                    &facts,
                    Some(WarningId(next_id)),
                )
                .unwrap();
            if !prepared
                .effects
                .iter()
                .any(|effect| matches!(effect, ControlEffect::LifespanChanged { .. }))
            {
                prepared.commit(&control, &mut store).unwrap();
                continue;
            }
            let effects = prepared.effects.clone();
            let lifespan_facts = game_lifecycle::LifespanFacts {
                actor,
                now: facts.now,
                alive: true,
                lifespan_loss: prepared.lifespan_loss_after(&store, actor),
            };
            let lifespan = lifecycle
                .prepare_lifespan_change(&store, lifespan_facts)
                .unwrap();
            let survival = game_physiology::prepare_survival(&store, actor).unwrap();
            let charge = effects
                .iter()
                .find_map(|effect| match effect {
                    ControlEffect::FatigueCharge { quantity, .. } => Some(*quantity),
                    _ => None,
                })
                .unwrap();
            let charge =
                game_physiology::prepare_fatigue_charge(&store, actor, charge, facts.fatigue_limit)
                    .unwrap();
            let before = store.clone();
            let mut batch = Batch::new();
            let receipt = prepared.stage_into(&control, &store, &mut batch).unwrap();
            let charge = charge.stage_into_with_receipt(&store, &mut batch).unwrap();
            let alive = survival.check(&store, &batch, Some(&charge)).unwrap();
            assert!(alive);
            let response = lifespan
                .check(&lifecycle, &store, lifespan_facts, &batch)
                .unwrap();
            assert!(
                matches!(response, game_lifecycle::LifespanResponse::Died { actor: id } if id == actor)
            );
            let closing = receipt
                .closing_outcome(effects, |_| {
                    Ok(ClosingSurvival {
                        alive,
                        lifespan: match response {
                            game_lifecycle::LifespanResponse::Died { .. } => ClosingLifespan::Died,
                            _ => ClosingLifespan::Survives,
                        },
                    })
                })
                .unwrap();
            assert!(closing.effects.iter().any(|effect| matches!(
                effect,
                ControlEffect::CostApplied {
                    cost: InvocationCost::Lifespan,
                    permanent: true,
                    ..
                }
            )));
            assert!(
                !closing
                    .effects
                    .iter()
                    .any(|effect| matches!(effect, ControlEffect::ControlChanged { .. }))
            );
            assert!(!closing.review_controller);
            assert!(closing.observe_deaths);
            receipt.validate_batch(&control, &store, &batch).unwrap();
            assert_eq!(store, before);
            return;
        }
        panic!("accepted activation must produce its permanent lifespan cost");
    }
}
