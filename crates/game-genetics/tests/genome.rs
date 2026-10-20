use game_genetics::*;
use std::collections::BTreeMap;

#[test]
fn compact_genomes_match_ordered_maps_through_replacement_extensions_and_json() {
    let mut genome = Genome::default();
    let mut reference = BTreeMap::new();
    for i in 0..2048u64 {
        let key = format!("locus-{}", game_core::random(3, i, 1) % 127);
        let alleles = Alleles([i as u16, (i * 7) as u16]);
        assert_eq!(
            genome.insert(key.clone().into(), alleles),
            reference.insert(key, alleles)
        );
        if i % 64 == 0 {
            let encoded = serde_json::to_string(&genome).unwrap();
            assert_eq!(encoded, serde_json::to_string(&reference).unwrap());
            assert_eq!(serde_json::from_str::<Genome>(&encoded).unwrap(), genome);
            assert_eq!(genome.len(), reference.len());
            for (key, value) in &reference {
                assert_eq!(genome.get(key), Some(value));
            }
        }
    }
    assert!(!genome.is_empty());
    let rows = [
        ("z".into(), Alleles([1, 2])),
        ("a".into(), Alleles([3, 4])),
        ("z".into(), Alleles([5, 6])),
    ];
    let genome: Genome = rows.into_iter().collect();
    assert_eq!(genome["z"], Alleles([5, 6]));
    assert_eq!(
        genome.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(
        serde_json::from_str::<Genome>(r#"{"z":[1,2],"a":[3,4],"z":[5,6]}"#).unwrap(),
        genome
    );
}

#[test]
fn inheritance_and_mutation_stay_bounded_at_extremes_for_many_independent_families() {
    for seed in 0..32 {
        for value in [0, 1, 32767, 65534, 65535] {
            for mutation in [0, 1, 500, 65535] {
                let mother = Alleles([value, 65535 - value]);
                let father = Alleles([65535 - value, value]);
                for id in 1..33 {
                    let a = inherit(seed, game_core::PersonId(id), mother, father, mutation);
                    for (allele, parent) in a.0.into_iter().zip([mother, father]) {
                        assert!(parent.0.iter().any(|p| p.abs_diff(allele) <= mutation));
                    }
                    let m = mutate(seed, id, mother, mutation);
                    for (a, b) in m.0.into_iter().zip(mother.0) {
                        assert!(a.abs_diff(b) <= mutation);
                    }
                }
            }
        }
    }
}
