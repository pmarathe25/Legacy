use crate::Runtime;
use game_parallel_families::{Families, FamilyRules, FamilyTotals, Genome, Member, Sex};
use game_parallel_family_model::{LifeInput, PhysicalView};
struct View {
    rows: Vec<LifeInput>,
}
impl PhysicalView for View {
    fn read(&self, id: u64) -> LifeInput {
        self.rows[self
            .rows
            .binary_search_by_key(&id, |person| person.id)
            .unwrap()]
    }
}
fn fixture(count: u64, sparse: bool) -> (Families, View) {
    let members: Vec<_> = (0..count)
        .map(|i| Member {
            id: if sparse { 10 + i * 10 } else { i + 1 },
            born: None,
            age: 2,
            lifespan_loss: 0,
            genome: Genome {
                sex: if i % 2 == 0 { Sex::Female } else { Sex::Male },
                alleles: [[1000, 1000]; 4],
            },
            parents: Default::default(),
            partner: None,
            pregnancy: None,
            next_conception: 0,
            alive: true,
            reproduction_allowed: true,
            reconsider_partnership: false,
        })
        .collect();
    let view = View {
        rows: members
            .iter()
            .map(|member| LifeInput {
                id: member.id,
                region: Some(1),
                health: 100,
                hunger: 0,
            })
            .collect(),
    };
    let families = Families {
        rules: FamilyRules {
            adult_age: 2,
            min_work_age: 0,
            fertile_until: 10,
            gestation: 2,
            birth_spacing: 2,
            lifespan_min: 12,
            lifespan_max: 14,
            conception_one_in: 1,
            minimum_health: 50,
            maximum_hunger: 8,
            newborn_health: 100,
            mutation_per_million: 10000,
            mutation_step: 10,
            enroll_births: true,
        },
        next_id: members.last().unwrap().id + 1,
        members,
        last_tick: 0,
        totals: FamilyTotals::default(),
        events: vec![],
    };
    (families, view)
}
fn run(workers: usize, sparse: bool) -> Families {
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .unwrap()
        .install(|| {
            let (mut expected, mut physical) = fixture(64, sparse);
            expected.members.reverse();
            let mut runtime = Runtime::compile_validated(&expected).unwrap();
            assert_eq!(runtime.export(), expected);
            for tick in 1..=25 {
                // The same identity changes locality without moving lifecycle storage.
                physical.rows[0].region = if [2, 3, 9].contains(&tick) {
                    None
                } else {
                    Some(1)
                };
                if tick == 4 {
                    physical.rows[1].health = 0;
                }
                if tick == 6 {
                    physical.rows[2].hunger = 100;
                }
                if tick == 8 {
                    physical.rows[2].hunger = 0;
                }
                let reference = expected.advance(&physical.rows, 42, tick).unwrap();
                let draft = runtime.prepare(&physical, 42, tick, None).unwrap();
                assert_eq!(
                    draft.deaths,
                    reference.deaths.iter().copied().collect::<Vec<_>>()
                );
                let receipts = |births: &[game_parallel_family_model::Birth]| {
                    births
                        .iter()
                        .map(|birth| (birth.id, birth.mother, birth.region, birth.health))
                        .collect::<Vec<_>>()
                };
                assert_eq!(receipts(&draft.births), receipts(&reference.births));
                for birth in &draft.births {
                    physical.rows.push(LifeInput {
                        id: birth.id,
                        region: birth.region,
                        health: birth.health,
                        hunger: 0,
                    });
                }
                for &id in &draft.deaths {
                    let index = physical
                        .rows
                        .binary_search_by_key(&id, |person| person.id)
                        .unwrap();
                    physical.rows[index].health = 0;
                }
                runtime.commit(draft).unwrap();
                expected = reference.families;
                assert_eq!(
                    runtime.export(),
                    expected,
                    "tick={tick} workers={workers} sparse={sparse}"
                );
                expected
                    .validate_committed(&physical.rows, tick, 100)
                    .unwrap();
                if tick == 11 {
                    let saved = serde_json::to_string(&runtime.export()).unwrap();
                    runtime =
                        Runtime::compile_validated(&serde_json::from_str(&saved).unwrap()).unwrap();
                }
            }
            assert!(expected.totals.births > 0 && expected.totals.deaths > 0);
            expected
        })
}
#[test]
fn column_lifecycle_matches_reference_births_travel_death_pedigree_and_reload() {
    for sparse in [false, true] {
        assert_eq!(run(1, sparse), run(4, sparse));
    }
}
#[test]
fn failed_late_preparation_and_superseded_drafts_preserve_committed_columns() {
    let (mut families, physical) = fixture(8, false);
    families.events = vec![game_parallel_families::FamilyEvent {
        tick: 999,
        kind: "arbitrary-imported-event".into(),
        people: vec![9, 8, 7, 6],
        region: Some(123),
    }];
    families.totals.partnerships = u64::MAX;
    let mut runtime = Runtime::compile_validated(&families).unwrap();
    assert_eq!(
        runtime.prepare(&physical, 42, 1, None).err(),
        Some("family counter overflow")
    );
    assert_eq!(runtime.export(), families);
    runtime.totals.partnerships = 0;
    let before = runtime.export();
    let discarded = runtime.prepare(&physical, 42, 1, None).unwrap();
    let next = runtime.prepare(&physical, 42, 1, None).unwrap();
    assert!(runtime.commit(discarded).is_err());
    assert_eq!(runtime.export(), before);
    runtime.commit(next).unwrap();
    let retained = runtime.receipts.clone().unwrap();
    let retained_events = retained.export();
    assert_eq!(
        runtime.export(),
        before.advance(&physical.rows, 42, 1).unwrap().families
    );
    let discarded = runtime.prepare(&physical, 42, 2, None).unwrap();
    drop(discarded);
    assert_eq!(retained.export(), retained_events);
    assert_eq!(runtime.events(), retained_events);
}
#[test]
fn lifespan_costs_and_foreign_drafts_are_transactional() {
    let (families, physical) = fixture(8, true);
    let mut runtime = Runtime::compile_validated(&families).unwrap();
    let mut other = Runtime::compile_validated(&families).unwrap();
    let foreign = other.prepare(&physical, 4, 1, None).unwrap();
    assert!(runtime.commit(foreign).is_err());
    let draft = runtime.prepare(&physical, 4, 1, Some((10, 100))).unwrap();
    let expected = families
        .advance_validated_with_penalty(&physical, 4, 1, Some((10, 100)))
        .unwrap();
    runtime.commit(draft).unwrap();
    assert_eq!(runtime.export(), expected.families);
}

#[test]
fn births_cross_partial_block_boundaries_and_failed_birth_ids_do_not_publish() {
    let (mut expected, mut physical) = fixture(8194, false);
    let mut runtime = Runtime::compile_validated(&expected).unwrap();
    for tick in 1..=5 {
        let reference = expected.advance(&physical.rows, 71, tick).unwrap();
        let draft = runtime.prepare(&physical, 71, tick, None).unwrap();
        for birth in &draft.births {
            physical.rows.push(LifeInput {
                id: birth.id,
                region: birth.region,
                health: birth.health,
                hunger: 0,
            });
        }
        runtime.commit(draft).unwrap();
        expected = reference.families;
        assert_eq!(runtime.export(), expected);
    }
    assert!(runtime.store.metadata.len() > 8194 + game_parallel_family_columns::BLOCK);
    for block in &runtime.store.blocks[..runtime.store.blocks.len() - 1] {
        assert_eq!(block.current.age.len(), game_parallel_family_columns::BLOCK);
    }
    let (mut families, physical) = fixture(8, false);
    families.next_id = u64::MAX;
    let mut runtime = Runtime::compile_validated(&families).unwrap();
    for tick in 1..=2 {
        let draft = runtime.prepare(&physical, 42, tick, None).unwrap();
        runtime.commit(draft).unwrap();
    }
    let before = runtime.export();
    assert_eq!(
        runtime.prepare(&physical, 42, 3, None).err(),
        Some("birth identity overflow")
    );
    assert_eq!(runtime.export(), before);
}

#[test]
fn independent_reconsideration_matches_the_reference_lifecycle() {
    let (mut families, physical) = fixture(8, false);
    for member in &mut families.members {
        member.reproduction_allowed = false;
    }
    families.members[0].partner = Some(2);
    families.members[0].reproduction_allowed = true;
    families.members[0].reconsider_partnership = true;
    families.members[1].partner = Some(1);
    let expected = families.advance(&physical.rows, 42, 1).unwrap().families;
    let mut runtime = Runtime::compile_validated(&families).unwrap();
    let draft = runtime.prepare(&physical, 42, 1, None).unwrap();
    runtime.commit(draft).unwrap();
    assert_eq!(runtime.export(), expected);
    assert!(expected.members[0].partner.is_none());
    assert!(expected.members[1].partner.is_none());
}
