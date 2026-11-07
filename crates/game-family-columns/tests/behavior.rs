use game_family_columns::Store;
use game_family_model::Member;
use game_genetics::{Genome, Sex};

fn member(id: u64, parents: &[u64]) -> Member {
    Member {
        id,
        born: None,
        age: 3,
        lifespan_loss: 0,
        genome: Genome {
            sex: Sex::Female,
            alleles: [[1000, 1000]; 4],
        },
        parents: parents.iter().copied().collect(),
        partner: None,
        pregnancy: None,
        next_conception: 0,
        alive: true,
        reproduction_allowed: false,
        reconsider_partnership: false,
    }
}

#[test]
fn sorted_and_reordered_families_preserve_order_and_ancestry_across_workers() {
    for workers in [1, 4] {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let sorted = vec![
                    member(10, &[]),
                    member(20, &[]),
                    member(30, &[10, 20]),
                    member(50, &[10, 20]),
                ];
                for input in [sorted.clone(), sorted.iter().rev().cloned().collect()] {
                    let store = Store::compile(&input).unwrap();
                    assert_eq!(store.export(), input);
                    let first = store.locate(30).unwrap();
                    let second = store.locate(50).unwrap();
                    assert!(store.metadata[first].related(&store.metadata[second]));
                }
                assert!(Store::compile(&[member(1, &[]), member(1, &[])]).is_err());
                assert!(Store::compile(&[member(3, &[1, 2])]).is_err());
            });
    }
}
