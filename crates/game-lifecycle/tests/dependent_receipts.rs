use game_cargo::{ConsumptionFacts, Inventory, InventoryLocation};
use game_characters::decisions::{self, Aspiration, Mind, Personality};
use game_core::{
    AreaId, ChildhoodInterest, Error, Good, InventoryId, LifeStage, PersonId, Quantity, SimTime,
    Vitals,
};
use game_ecs::{Batch, Schema, StableId, Store};
use game_lifecycle::{DependentAction, Life, Reproduction, Rules, Runtime};
use game_physiology::Body;
use std::collections::{BTreeMap, BTreeSet};

fn fixture(reject: u8) -> (Runtime, game_lineage::Runtime, Store) {
    let mut schema = Schema::new();
    if reject == 1 {
        schema
            .register::<Inventory>("cargo/inventory", |stock| {
                stock.validate()?;
                if stock.quantity.0 < 4_000 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
    } else {
        game_cargo::register_components(&mut schema).unwrap();
    }
    if reject == 2 {
        schema
            .register::<Body>("physiology/body", |body| {
                body.validate()?;
                if body.vitals.hunger < 1_000 {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
    } else {
        game_physiology::register_components(&mut schema).unwrap();
    }
    if reject == 3 {
        schema
            .register::<Life>("lifecycle/life", |life| {
                life.validate()?;
                if !life.caregivers.is_empty() {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        schema
            .register::<game_lifecycle::Pregnancy>(
                "lifecycle/pregnancy",
                game_lifecycle::Pregnancy::validate,
            )
            .unwrap();
    } else {
        game_lifecycle::register_components(&mut schema).unwrap();
    }
    decisions::register_minds::<u8>(&mut schema).unwrap();
    if reject == 4 {
        schema
            .register::<game_lineage::History>("lineage/history", game_lineage::History::validate)
            .unwrap();
        schema
            .register::<game_lineage::Guardian>("lineage/guardian", |guardian| {
                guardian.validate()?;
                if guardian
                    .provision
                    .as_ref()
                    .is_some_and(|provision| provision.delivery.last_receipt > 0)
                {
                    Err(Error::Unavailable)
                } else {
                    Ok(())
                }
            })
            .unwrap();
        schema
            .register::<game_lineage::InvocationLearning>(
                "lineage/invocation_learning",
                |learning| {
                    if learning.learned == 0 {
                        Ok(())
                    } else {
                        Err(Error::Unavailable)
                    }
                },
            )
            .unwrap();
    } else {
        game_lineage::register_components(&mut schema).unwrap();
    }
    let mut store = Store::with_schema(schema);
    let runtime = Runtime {
        rules: Rules::human(),
    };
    for actor in [PersonId(1), PersonId(2)] {
        game_lifecycle::insert_life(
            &mut store,
            Life {
                id: actor,
                born: SimTime(0),
                initial_age: if actor == PersonId(1) {
                    runtime.rules.infancy
                } else {
                    runtime.rules.adulthood
                },
                stage: if actor == PersonId(1) {
                    LifeStage::Child
                } else {
                    LifeStage::Adult
                },
                reproduction: Reproduction::Neither,
                parents: vec![],
                caregivers: BTreeSet::new(),
                preference: (actor == PersonId(1))
                    .then_some((ChildhoodInterest::Food, SimTime(300_000))),
            },
        )
        .unwrap();
        game_physiology::insert_body(
            &mut store,
            actor,
            Body {
                vitals: Vitals {
                    hunger: 1_000,
                    fatigue: 0,
                    health: 100,
                    alive: true,
                },
                fatigue_remainder: 0,
                health_remainder: 0,
            },
        )
        .unwrap();
        decisions::insert_mind::<u8>(
            &mut store,
            actor,
            Mind::new(Personality::industrious(), Aspiration::Comfort).unwrap(),
        )
        .unwrap();
    }
    game_cargo::insert_inventory(
        &mut store,
        Inventory {
            id: InventoryId(10),
            location: InventoryLocation::Area(AreaId(20)),
            quantity: Quantity(4_000),
            capacity: Quantity(4_000),
            access: BTreeSet::from([PersonId(2)]),
            last_granted: None,
            good: Good::Food,
            spoils_at: None,
        },
    )
    .unwrap();
    let delivery = game_logistics::Delivery::new(
        game_logistics::Requirement {
            good: Good::Food,
            quantity: Quantity(2_000),
            source: None,
        },
        game_logistics::Destination::Contact(PersonId(1)),
    )
    .unwrap();
    let lineage = game_lineage::Runtime::initialize(
        &mut store,
        StableId(1),
        game_lineage::MentorshipRules::default(),
        game_lineage::History {
            incarnations: vec![game_lineage::Incarnation {
                person: PersonId(1),
                bound_at: SimTime(0),
                died_at: None,
            }],
        },
        [(
            PersonId(2),
            game_lineage::Guardian {
                target: Some(PersonId(1)),
                sensed_location: Some(game_core::Location::AtArea(AreaId(20))),
                sensed_at: SimTime(0),
                goal: game_lineage::GuardianGoal::Mentoring,
                explanation: "actual food provision".into(),
                assessments: BTreeMap::new(),
                assignment: None,
                requested_delivery: None,
                provision: Some(game_lineage::CareProvision {
                    delivery,
                    observed_at: SimTime(0),
                }),
                knows_invocation: true,
            },
        )],
    )
    .unwrap();
    (runtime, lineage, store)
}
#[test]
fn actual_food_care_debit_relief_caregiver_and_relationship_publish_together_or_remain_unchanged() {
    for rejected_owner in 0..=4 {
        let (lifecycle, lineage, mut store) = fixture(rejected_owner);
        let before = store.snapshot().unwrap();
        let quantity = Quantity(1_000);
        let consumption = game_cargo::prepare_portion_consumption(
            &store,
            PersonId(2),
            InventoryId(10),
            quantity,
            ConsumptionFacts {
                good: Good::Food,
                accessible: true,
                contact: true,
                unreserved: Quantity(4_000),
            },
        )
        .unwrap();
        let relief = game_physiology::prepare_meal_relief(&store, PersonId(1), 500).unwrap();
        let care = lifecycle
            .prepare_dependent_reaction(
                &store,
                PersonId(2),
                DependentAction::Care(PersonId(1)),
                SimTime(10),
                true,
                true,
            )
            .unwrap()
            .unwrap();
        let relationship = game_characters::receivers::prepare_care_relationship::<u8>(
            &store,
            PersonId(1),
            PersonId(2),
        )
        .unwrap();
        let receipt = game_characters::receivers::prepare_care_receipt::<u8>(
            &store,
            PersonId(2),
            PersonId(1),
            Good::Food,
            quantity,
            1,
        )
        .unwrap();
        let guardian_receipt = lineage
            .prepare_care_receipt(&store, PersonId(2), PersonId(1), Good::Food, quantity, 1)
            .unwrap();
        let mut batch = Batch::new();
        let consumption_guard = consumption.stage_into(&store, &mut batch);
        if rejected_owner == 1 {
            assert!(matches!(consumption_guard, Err(Error::Unavailable)));
            assert_eq!(store.snapshot().unwrap(), before);
            continue;
        }
        let guards = vec![
            consumption_guard.unwrap(),
            relief.stage_into(&store, &mut batch).unwrap(),
            care.stage_into(&lifecycle, &store, &mut batch).unwrap(),
            relationship.stage_into(&store, &mut batch).unwrap(),
            receipt.stage_into(&store, &mut batch).unwrap(),
        ];
        let guardian = guardian_receipt
            .stage_into(&lineage, &store, &mut batch)
            .unwrap();
        guardian.validate(&lineage, &store, &batch).unwrap();
        for guard in &guards {
            guard.validate(&store, &batch).unwrap();
        }
        assert_eq!(store.snapshot().unwrap(), before);
        let published = store.apply_batch(batch);
        if rejected_owner == 0 {
            published.unwrap();
            assert_eq!(
                game_cargo::inventory(&store, InventoryId(10))
                    .unwrap()
                    .quantity,
                Quantity(3_000)
            );
            assert_eq!(
                game_cargo::inventory(&store, InventoryId(10))
                    .unwrap()
                    .last_granted,
                Some(PersonId(2))
            );
            assert_eq!(
                game_physiology::body(&store, PersonId(1))
                    .unwrap()
                    .vitals
                    .hunger,
                500
            );
            assert_eq!(
                game_lifecycle::life(&store, PersonId(1))
                    .unwrap()
                    .caregivers,
                BTreeSet::from([PersonId(2)])
            );
            assert_eq!(
                game_lifecycle::life(&store, PersonId(1))
                    .unwrap()
                    .preference,
                None
            );
            assert!(guardian.finish());
            assert_eq!(
                game_lineage::guardian(&store, PersonId(2))
                    .unwrap()
                    .provision
                    .as_ref()
                    .unwrap()
                    .delivery
                    .remaining,
                Quantity(1_000)
            );
            assert!(
                !lineage
                    .prepare_care_receipt(&store, PersonId(2), PersonId(1), Good::Food, quantity, 1)
                    .unwrap()
                    .commit(&lineage, &mut store)
                    .unwrap()
            );
            let relation = &decisions::mind::<u8>(&store, PersonId(1))
                .unwrap()
                .relationships[&PersonId(2)];
            assert_eq!((relation.trust, relation.affection), (50, 50));
        } else {
            assert_eq!(published, Err(Error::Unavailable));
            assert_eq!(store.snapshot().unwrap(), before);
        }
    }
}
