use super::*;
use game_ecs::{Schema, StableId};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum OpaqueAction {
    Issued { token: String, nonce: u128 },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Foreign(u64);

fn schema() -> Schema {
    let mut schema = Schema::new();
    register_minds::<OpaqueAction>(&mut schema).unwrap();
    schema
}
fn row() -> Mind<OpaqueAction> {
    let mut row: Mind<OpaqueAction> =
        serde_json::from_value(serde_json::to_value(super::tests::mind()).unwrap()).unwrap();
    row.plan = Some(Plan {
        action: OpaqueAction::Issued {
            token: "owner-defined-command".into(),
            nonce: u128::MAX - 3,
        },
        key: IntentionKey::Relief {
            need: NeedRef::Company,
            beneficiary: PersonId(1),
        },
        motives: vec![Motive::Relieve {
            need: NeedRef::Company,
            beneficiary: PersonId(1),
        }],
        prospects: Prospects::default(),
        shape: ActionShape::Other,
        delivery: None,
    });
    row
}
fn runtime() -> Runtime {
    Runtime {
        concepts: BTreeMap::from([(ConceptId(6), Good::Timber)]),
        work_concept: ConceptId(6),
        last_learning_event: 0,
        switching_margin: 100,
    }
}

#[test]
fn personal_minds_and_values_share_entities_without_sharing_learning_or_foreign_rows() {
    let mut schema = schema();
    schema
        .register::<Foreign>("other/value", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema);
    store.insert_or_spawn(StableId(1), Foreign(71)).unwrap();
    insert_mind(&mut store, PersonId(1), row()).unwrap();
    let mut other = row();
    other.personality = Personality::restful();
    other.dream = Aspiration::Comfort;
    other.values.insert(
        ConceptId(6),
        Attachment {
            weight: 700,
            experiences: 15,
        },
    );
    insert_mind(&mut store, PersonId(2), other.clone()).unwrap();
    let runtime = runtime();
    let before = runtime.clone();
    for _ in 0..2 {
        runtime.learn_production::<OpaqueAction>(&mut store, PersonId(1));
    }
    let learned = &mind::<OpaqueAction>(&store, PersonId(1)).unwrap().values[&ConceptId(6)];
    assert_eq!(
        learned,
        &Attachment {
            weight: 200,
            experiences: 2
        }
    );
    assert_eq!(mind::<OpaqueAction>(&store, PersonId(2)), Some(&other));
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(71)));
    assert_eq!(runtime, before);
    assert_eq!(minds::<OpaqueAction>(&store).count(), 2);
    let before = store.clone();
    runtime.learn_production::<OpaqueAction>(&mut store, PersonId(99));
    assert_eq!(store, before);
}

#[test]
fn mind_schema_registration_and_invalid_insertions_are_inert() {
    let mut schema = schema();
    assert_eq!(schema.keys().collect::<Vec<_>>(), vec!["characters/mind"]);
    assert!(schema.is_registered::<Mind<OpaqueAction>>());
    assert!(!schema.is_registered::<Mind<u8>>());
    assert!(register_minds::<OpaqueAction>(&mut schema).is_err());
    assert!(register_minds::<u8>(&mut schema).is_err());
    let mut store = Store::with_schema(schema);
    insert_mind(&mut store, PersonId(1), row()).unwrap();
    let before = store.clone();
    for actor in [PersonId(1), PersonId(2)] {
        let mut invalid = row();
        invalid.personality.empathy = 1_001;
        assert!(insert_mind(&mut store, actor, invalid).is_err());
        let mut invalid = row();
        invalid.interests.insert(ChildhoodInterest::Craft, 1_001);
        assert!(insert_mind(&mut store, actor, invalid).is_err());
        let mut invalid = row();
        invalid.values.insert(
            ConceptId(6),
            Attachment {
                weight: -1,
                experiences: 0,
            },
        );
        assert!(insert_mind(&mut store, actor, invalid).is_err());
    }
    assert!(insert_mind(&mut store, PersonId(0), row()).is_err());
    assert_eq!(store, before);
    assert!(mind::<u8>(&store, PersonId(1)).is_none());
}

#[test]
fn intrinsic_mind_validation_preserves_opaque_actions_and_leaves_catalogs_and_clocks_to_composition()
 {
    let mut row = row();
    row.knowledge.insert(
        InventoryId(17),
        StockKnowledge {
            inventory: InventoryId(17),
            area: AreaId(23),
            good: Good::Water,
            quantity: Quantity(500),
            accessible: false,
            observed_at: SimTime(999),
            learned_at: SimTime(1_000),
            source: KnowledgeSource::Recorded(RecordId(33)),
        },
    );
    let mut store = Store::with_schema(schema());
    insert_mind(&mut store, PersonId(1), row.clone()).unwrap();
    assert_eq!(mind::<OpaqueAction>(&store, PersonId(1)), Some(&row));
    let mut mismatched = row;
    mismatched
        .knowledge
        .get_mut(&InventoryId(17))
        .unwrap()
        .inventory = InventoryId(18);
    assert!(mismatched.validate().is_err());
    let mut runtime = runtime();
    runtime.concepts.insert(runtime.work_concept, Good::Water);
    validate_runtime(&runtime, 1).unwrap();
    let mutations: [fn(&mut Runtime); 3] = [
        |runtime| runtime.last_learning_event = 1,
        |runtime| runtime.switching_margin = -1,
        |runtime| runtime.concepts.clear(),
    ];
    for mutate in mutations {
        let mut invalid = runtime.clone();
        mutate(&mut invalid);
        assert!(matches!(
            validate_runtime(&invalid, 1),
            Err(Error::InvalidSave(_))
        ));
    }
    let mut json = serde_json::to_value(runtime).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("minds".into(), serde_json::json!({}));
    assert!(serde_json::from_value::<Runtime>(json).is_err());
}

#[test]
fn opaque_actions_roundtrip_without_native_handles_or_loss_of_integer_precision() {
    let mut schema = schema();
    schema
        .register::<Foreign>("other/value", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema.clone());
    store.insert_or_spawn(StableId(1), Foreign(71)).unwrap();
    insert_mind(&mut store, PersonId(1), row()).unwrap();
    let json = serde_json::to_string(&store.snapshot().unwrap()).unwrap();
    assert!(json.contains("340282366920938463463374607431768211452"));
    let restored = Store::restore(schema, serde_json::from_str(&json).unwrap()).unwrap();
    assert_eq!(restored, store);
    assert!(
        remove_mind::<OpaqueAction>(&mut store, PersonId(1))
            .unwrap()
            .is_some()
    );
    assert!(store.contains(StableId(1)));
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign(71)));
    assert!(
        remove_mind::<OpaqueAction>(&mut store, PersonId(99))
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_stricter_mind_schema_cannot_publish_partial_replacements_or_entities() {
    let mut schema = Schema::new();
    schema
        .register::<Mind<OpaqueAction>>("characters/mind", |row| {
            row.validate()?;
            if row.personality.empathy >= 800 {
                Err(Error::Unavailable)
            } else {
                Ok(())
            }
        })
        .unwrap();
    let mut store = Store::with_schema(schema);
    insert_mind(&mut store, PersonId(1), row()).unwrap();
    let before = store.clone();
    for actor in [PersonId(1), PersonId(2)] {
        let mut invalid = row();
        invalid.personality.empathy = 800;
        assert_eq!(
            insert_mind(&mut store, actor, invalid),
            Err(Error::Unavailable)
        );
        assert_eq!(store, before);
    }
    mind_mut::<OpaqueAction>(&mut store, PersonId(1))
        .unwrap()
        .personality
        .empathy = 1_001;
    assert!(Store::restore(store.schema().clone(), store.snapshot().unwrap()).is_err());
}

#[test]
fn sparse_trust_reaction_preserves_other_mind_fields_and_rejects_stale_or_foreign_footprints() {
    let mut store = Store::with_schema(schema());
    insert_mind(&mut store, PersonId(1), row()).unwrap();
    let effects = || {
        [
            TrustDelta {
                observer: PersonId(1),
                subject: PersonId(2),
                delta: 50,
            },
            TrustDelta {
                observer: PersonId(1),
                subject: PersonId(2),
                delta: 70,
            },
        ]
    };
    let change = PreparedTrustReactions::prepare::<OpaqueAction>(&store, effects()).unwrap();
    mind_mut::<OpaqueAction>(&mut store, PersonId(1))
        .unwrap()
        .retry_after = SimTime(88);
    change.commit::<OpaqueAction>(&mut store).unwrap();
    let mind = mind::<OpaqueAction>(&store, PersonId(1)).unwrap();
    assert_eq!(mind.retry_after, SimTime(88));
    assert_eq!(mind.relationships[&PersonId(2)].trust, 120);
    let change = PreparedTrustReactions::prepare::<OpaqueAction>(&store, effects()).unwrap();
    let mut batch = game_ecs::Batch::new();
    batch.remove::<Mind<OpaqueAction>>(StableId(1));
    assert!(change.check_batch::<OpaqueAction>(&store, &batch).is_err());
    mind_mut::<OpaqueAction>(&mut store, PersonId(1))
        .unwrap()
        .relationships
        .get_mut(&PersonId(2))
        .unwrap()
        .affection = 1;
    let before = store.clone();
    assert!(change.commit::<OpaqueAction>(&mut store).is_err());
    assert_eq!(store, before);
}
