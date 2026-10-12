use crate::institutions::*;
use crate::policy_fixtures::{case as case_fixture, labor, refs, seed_institution, world};
use crate::storage::*;
use game_core::*;
use game_ecs::{Batch, StableId, Store};
use std::collections::{BTreeMap, BTreeSet};
#[test]
fn society_uses_native_rows_and_sparse_learning_preserves_unrelated_foreign_data() {
    let (runtime, _) = world();
    // A second owner shares the same entity, and must survive Society's effects.
    #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct Foreign {
        n: u64,
    }
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    schema
        .register::<Foreign>("test/foreign", |_| Ok(()))
        .unwrap();
    let mut store = Store::with_schema(schema);
    store
        .insert_or_spawn(StableId(1), Foreign { n: 99 })
        .unwrap();
    insert_concept(&mut store, ConceptId(21), Concept { rule: labor() }).unwrap();
    let mut batch = Batch::new();
    runtime
        .prepare_case_learning(
            &store,
            case_fixture(
                30,
                1,
                SocialCaseKind::Cooperation,
                SocialOutcome::Contributed,
            ),
            Some(ConceptId(21)),
            &BTreeMap::from([(PersonId(1), 500)]),
        )
        .unwrap()
        .stage_into(&store, &mut batch)
        .unwrap();
    store.apply_batch(batch).unwrap();
    assert_eq!(store.get::<Foreign>(StableId(1)), Some(&Foreign { n: 99 }));
    assert_eq!(
        knowledge(&store, PersonId(1)).unwrap().values[&ConceptId(21)].weight,
        500
    );
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.columns.contains_key("society/knowledge"));
    let encoded = serde_json::to_vec(&snapshot).unwrap();
    let restored = Store::restore(
        store.schema().clone(),
        serde_json::from_slice(&encoded).unwrap(),
    )
    .unwrap();
    assert_eq!(store, restored);
    assert_eq!(
        serde_json::to_value(runtime).unwrap(),
        serde_json::json!({"last_event":0})
    );
}
#[test]
fn stale_observer_learning_and_invalid_batched_row_leave_store_identical() {
    let (runtime, mut store) = world();
    insert_knowledge(&mut store, PersonId(1), Knowledge::default()).unwrap();
    let plan = runtime
        .prepare_case_learning(
            &store,
            case_fixture(
                30,
                1,
                SocialCaseKind::Cooperation,
                SocialOutcome::Contributed,
            ),
            None,
            &BTreeMap::from([(PersonId(1), 500)]),
        )
        .unwrap();
    knowledge_mut(&mut store, PersonId(1))
        .unwrap()
        .cases
        .insert(CaseId(99));
    let before = store.snapshot().unwrap();
    assert!(plan.stage_into(&store, &mut Batch::new()).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
    let mut batch = Batch::new();
    batch.insert(
        StableId(30),
        case_fixture(
            30,
            1,
            SocialCaseKind::Cooperation,
            SocialOutcome::Contributed,
        ),
    );
    batch.insert(
        StableId(31),
        SocialCaseView {
            subject: PersonId(0),
            ..case_fixture(31, 1, SocialCaseKind::Default, SocialOutcome::Defaulted)
        },
    );
    assert!(store.apply_batch(batch).is_err());
    assert_eq!(store.snapshot().unwrap(), before);
}
#[test]
fn contextual_validation_checks_real_rows_and_retains_historical_replaced_concepts() {
    let (runtime, mut store) = world();
    let mut i = crate::policy_fixtures::institution(10, vec![labor()], InstitutionStatus::Active);
    i.members = BTreeSet::from([PersonId(1), PersonId(2)]);
    seed_institution(&mut store, i);
    insert_concept(&mut store, ConceptId(21), Concept { rule: labor() }).unwrap();
    institution_mut(&mut store, InstitutionId(10))
        .unwrap()
        .applied
        .insert(
            ConceptId(21),
            BTreeMap::from([(PersonId(1), AgreementId(40))]),
        );
    let references = refs();
    let areas = BTreeSet::from([AreaId(3)]);
    let mut agreements = game_agreements::State::default();
    agreements.agreements.insert(
        AgreementId(40),
        game_agreements::Agreement {
            id: AgreementId(40),
            proposer: PersonId(1),
            parties: BTreeSet::from([PersonId(1), PersonId(2)]),
            accepted: BTreeSet::new(),
            terms: vec![],
            procedure: None,
            obligations: vec![],
            status: game_agreements::AgreementStatus::Proposed,
            proposed_at: SimTime(0),
            revision: 0,
        },
    );
    let facts = crate::save_validation::ValidationFacts {
        at: SimTime(10),
        next_id: 2000,
        next_event: 1,
        areas: &areas,
        references: &references,
        agreements: &agreements,
    };
    runtime.validate(&store, &facts).unwrap();
    insert_hearing(&mut store, CaseId(999), Hearing { shared: true }).unwrap();
    assert!(runtime.validate(&store, &facts).is_err());
}
