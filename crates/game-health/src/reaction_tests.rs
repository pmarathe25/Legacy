use crate::*;
use game_core::*;
use game_ecs::{Schema, Store};
use std::collections::BTreeMap;

fn onset() -> Clinical {
    Clinical {
        illnesses: BTreeMap::from([(
            IllnessKind::Enteric,
            Infection {
                kind: IllnessKind::Enteric,
                acquired: SimTime(0),
                onset: SimTime(100),
                recovers: SimTime(1000),
                symptomatic: false,
            },
        )]),
        last_drink: Some((InventoryId(10), SimTime(1))),
        ..Clinical::default()
    }
}
fn fixture(reject_clinical: bool, reject_knowledge: bool) -> (Runtime, Store) {
    let mut schema = Schema::new();
    schema
        .register::<Clinical>(
            "health/clinical",
            if reject_clinical {
                |row| {
                    row.validate()?;
                    if row.illnesses.values().any(|illness| illness.symptomatic) {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                }
            } else {
                Clinical::validate
            },
        )
        .unwrap();
    schema
        .register::<Knowledge>(
            "health/knowledge",
            if reject_knowledge {
                |row| {
                    row.validate()?;
                    if row.water.values().any(|water| water.suspected_unsafe) {
                        Err(Error::Unavailable)
                    } else {
                        Ok(())
                    }
                }
            } else {
                Knowledge::validate
            },
        )
        .unwrap();
    let mut store = Store::with_schema(schema);
    insert_clinical(&mut store, PersonId(1), onset()).unwrap();
    let water = [(InventoryId(10), false), (InventoryId(11), true)]
        .into_iter()
        .map(|(inventory, purified)| {
            (
                inventory,
                WaterSafetyView {
                    inventory,
                    purified,
                    suspected_unsafe: false,
                    observed_at: SimTime(1),
                },
            )
        })
        .collect();
    insert_knowledge(
        &mut store,
        PersonId(1),
        Knowledge {
            water,
            ..Knowledge::default()
        },
    )
    .unwrap();
    (
        Runtime {
            rules: Rules::fixture(),
            ..Runtime::default()
        },
        store,
    )
}
#[test]
fn clinical_onset_and_received_water_suspicion_reject_as_one_plan_if_either_schema_rejects() {
    for (reject_clinical, reject_knowledge) in [(true, false), (false, true), (false, false)] {
        let (mut runtime, mut store) = fixture(reject_clinical, reject_knowledge);
        let before = store.clone();
        let before_runtime = runtime.clone();
        let plan = runtime
            .prepare_clinical_reconciliation(&store, SimTime(100), |_| true)
            .unwrap();
        assert_eq!(
            plan.effects(),
            &[HealthEffect::Clinical {
                actor: PersonId(1),
                event: ClinicalEvent::Started(IllnessKind::Enteric)
            }]
        );
        let result = plan.commit(&mut runtime, &mut store);
        if reject_clinical || reject_knowledge {
            assert_eq!(result, Err(Error::Unavailable));
            assert_eq!(store, before);
            assert_eq!(runtime, before_runtime);
        } else {
            assert_eq!(
                result.unwrap(),
                vec![HealthEffect::Clinical {
                    actor: PersonId(1),
                    event: ClinicalEvent::Started(IllnessKind::Enteric)
                }]
            );
            assert!(
                clinical(&store, PersonId(1)).unwrap().illnesses[&IllnessKind::Enteric].symptomatic
            );
            let known = knowledge(&store, PersonId(1)).unwrap();
            assert!(known.water[&InventoryId(10)].suspected_unsafe);
            assert!(!known.water[&InventoryId(10)].purified);
            assert_eq!(known.water[&InventoryId(10)].observed_at, SimTime(100));
            assert_eq!(
                known.water[&InventoryId(11)],
                knowledge(&before, PersonId(1)).unwrap().water[&InventoryId(11)]
            );
        }
    }
}
#[test]
fn folded_reaction_preserves_the_existing_sequential_clinical_and_memory_result() {
    let (mut runtime, mut folded) = fixture(false, false);
    let mut sequential = folded.clone();
    runtime
        .prepare_clinical_reconciliation(&folded, SimTime(100), |_| true)
        .unwrap()
        .commit(&mut runtime, &mut folded)
        .unwrap();
    let transition = reconcile_clinical(
        clinical(&sequential, PersonId(1)).unwrap(),
        &runtime.rules,
        SimTime(100),
        true,
    )
    .unwrap();
    insert_clinical(&mut sequential, PersonId(1), transition.state).unwrap();
    runtime
        .prepare_reaction(
            &sequential,
            SimTime(100),
            HealthReaction::IllnessStarted {
                actor: PersonId(1),
                kind: IllnessKind::Enteric,
            },
        )
        .unwrap()
        .commit(&mut runtime, &mut sequential)
        .unwrap();
    assert_eq!(folded, sequential);
}
#[test]
fn choosing_to_preserve_purified_memory_still_guards_the_actual_selection_input() {
    let (mut runtime, mut store) = fixture(false, false);
    let mut known = knowledge(&store, PersonId(1)).unwrap().clone();
    known.water.get_mut(&InventoryId(10)).unwrap().purified = true;
    insert_knowledge(&mut store, PersonId(1), known).unwrap();
    let plan = runtime
        .prepare_clinical_reconciliation(&store, SimTime(100), |_| true)
        .unwrap();
    let mut changed = knowledge(&store, PersonId(1)).unwrap().clone();
    changed.water.get_mut(&InventoryId(10)).unwrap().purified = false;
    insert_knowledge(&mut store, PersonId(1), changed).unwrap();
    let before = store.clone();
    assert_eq!(
        plan.commit(&mut runtime, &mut store),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
}
#[test]
fn outdated_drinks_and_respiratory_onsets_do_not_invent_unsafe_water_observations() {
    for respiratory in [false, true] {
        let (mut runtime, mut store) = fixture(false, false);
        let mut state = onset();
        if respiratory {
            let mut infection = state.illnesses.remove(&IllnessKind::Enteric).unwrap();
            infection.kind = IllnessKind::Respiratory;
            state.illnesses.insert(IllnessKind::Respiratory, infection);
        } else {
            runtime.rules.incubation = SimDuration(10);
        }
        insert_clinical(&mut store, PersonId(1), state).unwrap();
        let before = knowledge(&store, PersonId(1)).unwrap().clone();
        runtime
            .prepare_clinical_reconciliation(&store, SimTime(100), |_| true)
            .unwrap()
            .commit(&mut runtime, &mut store)
            .unwrap();
        assert_eq!(knowledge(&store, PersonId(1)), Some(&before));
    }
}
