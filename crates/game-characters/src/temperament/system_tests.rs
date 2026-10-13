use super::*;
use crate::decisions::{Aspiration, Explanation, Goal, Mind, Personality};
use game_core::{AreaId, Good, InventoryId, KnowledgeSource, Quantity, SimTime, StockKnowledge};
use game_genetics::{AlleleSource, HeritableCharacteristic};

struct Source(i32);
impl AlleleSource for Source {
    fn alleles_named(&self, _: PersonId, _: &str) -> Result<[i32; 2], Error> {
        Ok([self.0; 2])
    }
}
struct Unread;
impl AlleleSource for Unread {
    fn alleles_named(&self, _: PersonId, _: &str) -> Result<[i32; 2], Error> {
        panic!("unknown experience must not read genes")
    }
}
fn mind() -> Mind<u8> {
    Mind {
        interests: BTreeMap::new(),
        routes: BTreeMap::new(),
        last_failure: None,
        child_signals: BTreeMap::new(),
        personality: Personality::industrious(),
        dream: Aspiration::Independence,
        sites: BTreeMap::new(),
        relationships: BTreeMap::new(),
        values: BTreeMap::new(),
        goal: Goal::Rest,
        plan: None,
        explanation: Explanation {
            goal: Goal::Rest,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        },
        retry_after: SimTime(0),
    }
}
fn schema() -> Schema {
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    crate::decisions::register_minds::<u8>(&mut schema).unwrap();
    game_genetics::register_components(&mut schema).unwrap();
    schema.register::<u64>("fixture/fact", |_| Ok(())).unwrap();
    schema
}
fn fixture() -> (Runtime, Store) {
    let runtime = Runtime::new(super::tests::definitions()).unwrap();
    let mut store = Store::with_schema(schema());
    runtime
        .register(
            &mut store,
            PersonId(1),
            Temperament::from(&Personality::industrious()),
        )
        .unwrap();
    crate::decisions::insert_mind(&mut store, PersonId(1), mind()).unwrap();
    (runtime, store)
}

#[test]
fn learning_and_expression_use_actual_opaque_alleles_without_rewriting_genomes() {
    let (runtime, mut store) = fixture();
    let mut genes =
        game_genetics::Runtime::new(game_genetics::Rules::default(), runtime.definitions.loci())
            .unwrap();
    genes.seed_person(&mut store, 7, PersonId(1)).unwrap();
    genes
        .set_alleles(
            &mut store,
            PersonId(1),
            &game_genetics::LocusId("temperament.test".into()),
            [1_000; 2],
        )
        .unwrap();
    let inherited = game_genetics::person_genome(&store, PersonId(1))
        .unwrap()
        .clone();
    let prepared = runtime
        .prepare_learning::<u8>(
            &store,
            &[(PersonId(1), "care")],
            &genes.allele_source(&store),
        )
        .unwrap();
    prepared
        .commit(&runtime, &mut store, |store, actor, locus| {
            genes.alleles_named(store, actor, locus)
        })
        .unwrap();
    assert_eq!(
        person_temperament(&store, PersonId(1))
            .unwrap()
            .learned_offsets,
        [1, 0, 1, 0]
    );
    let personality = &crate::decisions::mind::<u8>(&store, PersonId(1))
        .unwrap()
        .personality;
    assert_eq!(
        (
            personality.discipline,
            personality.empathy,
            personality.ambition
        ),
        (901, 701, 900)
    );
    assert_eq!(
        game_genetics::person_genome(&store, PersonId(1)),
        Some(&inherited)
    );
}

#[test]
fn declared_learning_order_keeps_saturation_and_unknown_definitions_are_noops() {
    let (mut runtime, mut store) = fixture();
    runtime.definitions.learning.push(LearningDefinition {
        id: "undo".into(),
        deltas: [-1, 0, -1, 0],
    });
    let empty = runtime
        .prepare_learning::<u8>(
            &store,
            &[(PersonId(1), "unknown"), (PersonId(99), "care")],
            &Unread,
        )
        .unwrap();
    assert_eq!(empty.affected().count(), 0);
    let before = store.clone();
    empty
        .commit(&runtime, &mut store, |_, _, _| {
            panic!("no affected expression")
        })
        .unwrap();
    assert_eq!(store, before);
    person_temperament_mut(&mut store, PersonId(1))
        .unwrap()
        .learned_offsets = [2, 0, 2, 0];
    let prepared = runtime
        .prepare_learning::<u8>(
            &store,
            &[(PersonId(1), "care"), (PersonId(1), "undo")],
            &Source(1_000),
        )
        .unwrap();
    prepared
        .commit(&runtime, &mut store, |_, _, _| Ok([1_000; 2]))
        .unwrap();
    assert_eq!(
        person_temperament(&store, PersonId(1))
            .unwrap()
            .learned_offsets,
        [1, 0, 1, 0]
    );
}

#[test]
fn personality_commit_preserves_current_water_knowledge_plans_and_other_mind_fields() {
    let (runtime, mut store) = fixture();
    let prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1_000))
        .unwrap();
    let (current, stocks) = store
        .get_pair_mut::<crate::decisions::Mind<u8>, crate::decisions::Stocks>(game_ecs::StableId(1))
        .unwrap();
    stocks.known.insert(
        InventoryId(9),
        StockKnowledge {
            inventory: InventoryId(9),
            area: AreaId(1),
            good: Good::Water,
            quantity: Quantity(333),
            accessible: true,
            observed_at: SimTime(0),
            learned_at: SimTime(0),
            source: KnowledgeSource::Observed(PersonId(1)),
        },
    );
    current.goal = Goal::Care;
    current.retry_after = SimTime(123);
    let expected_stocks = stocks.clone();
    let mut expected = current.clone();
    expected.personality.discipline = 901;
    expected.personality.empathy = 701;
    prepared
        .commit(&runtime, &mut store, |_, _, _| Ok([1_000; 2]))
        .unwrap();
    assert_eq!(
        crate::decisions::mind::<u8>(&store, PersonId(1)),
        Some(&expected)
    );
    assert_eq!(
        crate::decisions::stocks(&store, PersonId(1)),
        Some(&expected_stocks)
    );
}

#[test]
fn stale_personality_trait_header_or_alleles_are_rejected_before_learning() {
    for mutation in 0..4 {
        let (mut runtime, mut store) = fixture();
        let prepared = runtime
            .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1_000))
            .unwrap();
        let mut value = 1_000;
        match mutation {
            0 => runtime.definitions.learned_limit += 1,
            1 => {
                person_temperament_mut(&mut store, PersonId(1))
                    .unwrap()
                    .learned_offsets[0] = 1
            }
            2 => {
                crate::decisions::mind_mut::<u8>(&mut store, PersonId(1))
                    .unwrap()
                    .personality
                    .empathy += 1
            }
            3 => value = 0,
            _ => unreachable!(),
        }
        let before = (runtime.clone(), store.clone());
        assert_eq!(
            prepared.commit(&runtime, &mut store, |_, _, _| Ok([value; 2])),
            Err(Error::Unavailable)
        );
        assert_eq!((runtime, store), before);
    }
}

#[test]
fn malformed_current_minds_and_invalid_candidates_cannot_publish_trait_rows() {
    let (runtime, mut store) = fixture();
    let mut prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1_000))
        .unwrap();
    prepared.rows[0].next_personality.as_mut().unwrap().empathy = 1_001;
    let before = store.clone();
    assert!(
        prepared
            .commit(&runtime, &mut store, |_, _, _| Ok([1_000; 2]))
            .is_err()
    );
    assert_eq!(store, before);
    let prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1_000))
        .unwrap();
    crate::decisions::mind_mut::<u8>(&mut store, PersonId(1))
        .unwrap()
        .interests
        .insert(game_core::ChildhoodInterest::Craft, 1_001);
    let before = store.clone();
    assert!(
        prepared
            .commit(&runtime, &mut store, |_, _, _| Ok([1_000; 2]))
            .is_err()
    );
    assert_eq!(store, before);
    assert!(
        runtime
            .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1_000))
            .is_err()
    );
}

#[test]
fn rejecting_trait_admission_never_partially_changes_the_expressed_personality() {
    let mut schema = Schema::new();
    crate::decisions::register_minds::<u8>(&mut schema).unwrap();
    game_genetics::register_components(&mut schema).unwrap();
    schema
        .register::<PersonTemperament>("characters/temperament", |row| {
            row.validate()?;
            if row.learned_offsets != [0; 4] {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let runtime = Runtime::new(super::tests::definitions()).unwrap();
    let mut store = Store::with_schema(schema);
    runtime
        .register(
            &mut store,
            PersonId(1),
            Temperament::from(&Personality::industrious()),
        )
        .unwrap();
    crate::decisions::insert_mind(&mut store, PersonId(1), mind()).unwrap();
    let prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1_000))
        .unwrap();
    let before = store.clone();
    assert_eq!(
        prepared.commit(&runtime, &mut store, |_, _, _| Ok([1_000; 2])),
        Err(Error::Unavailable)
    );
    assert_eq!(store, before);
    let prepared = runtime
        .prepare_expression::<u8>(&store, &[PersonId(1)], &Source(1_000))
        .unwrap();
    prepared
        .commit(&runtime, &mut store, |_, _, _| Ok([1_000; 2]))
        .unwrap();
    assert_eq!(
        person_temperament(&store, PersonId(1))
            .unwrap()
            .learned_offsets,
        [0; 4]
    );
    assert_eq!(
        crate::decisions::mind::<u8>(&store, PersonId(1))
            .unwrap()
            .personality
            .empathy,
        700
    );
}

#[test]
fn typed_lived_event_uses_receiving_owner_tags_and_bounded_personality_change() {
    let (mut runtime, mut store) = fixture();
    runtime.definitions.learning = vec![
        LearningDefinition {
            id: "gave_care".into(),
            deltas: [0, 1, 0, 0],
        },
        LearningDefinition {
            id: "received_care".into(),
            deltas: [1, 0, 0, 0],
        },
    ];
    runtime
        .register(&mut store, PersonId(2), Temperament::default())
        .unwrap();
    crate::decisions::insert_mind(&mut store, PersonId(2), mind()).unwrap();
    for _ in 0..5 {
        runtime
            .prepare_event::<u8>(
                &store,
                ExperienceEvent::Care {
                    caregiver: PersonId(1),
                    child: PersonId(2),
                },
                &Source(500),
            )
            .unwrap()
            .commit(&runtime, &mut store, |_, _, _| Ok([500; 2]))
            .unwrap();
    }
    assert_eq!(
        person_temperament(&store, PersonId(1))
            .unwrap()
            .learned_offsets,
        [0, 2, 0, 0]
    );
    assert_eq!(
        person_temperament(&store, PersonId(2))
            .unwrap()
            .learned_offsets,
        [2, 0, 0, 0]
    );
    let before = store.clone();
    runtime
        .prepare_event::<u8>(
            &store,
            ExperienceEvent::Harm {
                target: PersonId(1),
            },
            &Unread,
        )
        .unwrap()
        .commit(&runtime, &mut store, |_, _, _| {
            panic!("unconfigured reaction must not read substrate")
        })
        .unwrap();
    assert_eq!(store, before);
}

#[test]
fn rejecting_native_mind_admission_rolls_back_the_trait_and_personality_together() {
    let runtime = Runtime::new(super::tests::definitions()).unwrap();
    let mut schema = Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Mind<u8>>("characters/mind", |mind| {
            mind.validate()?;
            if mind.personality.empathy > 700 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    runtime
        .register(
            &mut store,
            PersonId(1),
            Temperament::from(&Personality::industrious()),
        )
        .unwrap();
    store.insert_or_spawn(StableId(1), mind()).unwrap();
    let prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1000))
        .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        prepared.commit(&runtime, &mut store, |_, _, _| Ok([1000; 2])),
        Err(Error::Unavailable)
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn opaque_play_fold_preserves_receiving_fields_and_guards_the_single_native_mind_write() {
    let (runtime, mut store) = fixture();
    let prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1000))
        .unwrap();
    let play = crate::receivers::prepare_play::<u8>(
        &store,
        PersonId(1),
        game_core::ChildhoodInterest::Craft,
    )
    .unwrap();
    assert_eq!(play.gain(), 100);
    let folded = prepared
        .fold_minds(&store, play.into_mind_effects())
        .unwrap();
    let mut batch = Batch::new();
    let staged = folded
        .stage_into(&runtime, &store, &mut batch, &Source(1000))
        .unwrap();
    assert!(
        staged
            .validate(&runtime, &store, &Batch::new(), &Source(1000))
            .is_err()
    );
    staged
        .validate(&runtime, &store, &batch, &Source(1000))
        .unwrap();
    store.apply_batch(batch).unwrap();
    let current = crate::decisions::mind::<u8>(&store, PersonId(1)).unwrap();
    assert_eq!(current.interests[&game_core::ChildhoodInterest::Craft], 100);
    assert_eq!(
        (current.personality.discipline, current.personality.empathy),
        (901, 701)
    );
    assert_eq!(current.goal, Goal::Rest);
    assert_eq!(
        current.personality.risk_tolerance,
        mind().personality.risk_tolerance
    );

    let prepared = runtime
        .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1000))
        .unwrap();
    let mut batch = Batch::new();
    let staged = prepared
        .stage_into(&runtime, &store, &mut batch, &Source(1000))
        .unwrap();
    crate::decisions::mind_mut::<u8>(&mut store, PersonId(1))
        .unwrap()
        .retry_after = SimTime(7);
    assert_eq!(
        staged.validate(&runtime, &store, &batch, &Source(1000)),
        Err(Error::Unavailable)
    );
}

#[test]
fn staged_expression_rejects_late_writes_alleles_runtime_and_factual_source_changes() {
    for mutation in 0..4 {
        let (mut runtime, mut store) = fixture();
        store.insert_or_spawn(StableId(1), 12_u64).unwrap();
        let mut reads = ReadSet::new();
        reads.read::<u64>(&store, StableId(1));
        let prepared = runtime
            .prepare_learning::<u8>(&store, &[(PersonId(1), "care")], &Source(1000))
            .unwrap()
            .with_factual_reads(reads);
        let mut batch = Batch::new();
        let staged = prepared
            .stage_into(&runtime, &store, &mut batch, &Source(1000))
            .unwrap();
        let mut alleles = 1000;
        match mutation {
            0 => {
                batch.insert(StableId(1), mind());
            }
            1 => alleles = 0,
            2 => runtime.definitions.learned_limit += 1,
            3 => {
                batch.insert(StableId(1), 13_u64);
            }
            _ => unreachable!(),
        }
        assert!(
            staged
                .validate(&runtime, &store, &batch, &Source(alleles))
                .is_err()
        );
    }
}

#[test]
fn founder_expression_admits_baseline_and_native_mind_together_before_restore() {
    fn reject_expression(mind: &Mind<u8>) -> Result<(), Error> {
        mind.validate()?;
        if mind.personality.empathy > 500 {
            Err(Error::Unavailable)
        } else {
            Ok(())
        }
    }
    for reject in [true, false] {
        let runtime = Runtime::new(super::tests::definitions()).unwrap();
        let mut schema = Schema::new();
        register_components(&mut schema).unwrap();
        schema
            .register::<Mind<u8>>(
                "characters/mind",
                if reject {
                    reject_expression
                } else {
                    Mind::<u8>::validate
                },
            )
            .unwrap();
        let mut store = Store::with_schema(schema);
        store.insert_or_spawn(StableId(1), mind()).unwrap();
        let before = store.snapshot().unwrap();
        let prepared = runtime
            .prepare_founder_expression::<u8>(&store, PersonId(1), &Source(1000))
            .unwrap();
        let mut batch = Batch::new();
        let staged = prepared
            .stage_into(&runtime, &store, &mut batch, &Source(1000))
            .unwrap();
        staged
            .validate(&runtime, &store, &batch, &Source(1000))
            .unwrap();
        assert!(person_temperament(&store, PersonId(1)).is_none());
        if reject {
            assert!(store.apply_batch(batch).is_err());
            assert_eq!(store.snapshot().unwrap(), before);
        } else {
            store.apply_batch(batch).unwrap();
            assert_eq!(
                person_temperament(&store, PersonId(1))
                    .unwrap()
                    .baseline
                    .empathy,
                500
            );
            assert_eq!(
                crate::decisions::mind::<u8>(&store, PersonId(1))
                    .unwrap()
                    .personality
                    .empathy,
                700
            );
            let restored =
                Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
            assert_eq!(restored, store);
        }
    }
}
