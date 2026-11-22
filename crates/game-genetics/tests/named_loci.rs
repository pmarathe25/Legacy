use game_genetics::{Genome, Inheritance, Locus, NamedAlleles, Sex, TraitCatalog};
use std::sync::Arc;

fn founder(sex: Sex, variants: Option<Vec<NamedAlleles>>) -> Genome {
    Genome {
        sex,
        alleles: [[500; 2]; 4],
        mate_desire: [500; 2],
        named_loci: variants.map(Arc::new),
    }
}

#[test]
fn configured_loci_follow_parental_inheritance_and_keep_unchanged_defaults_sparse() {
    let catalog = TraitCatalog {
        loci: vec![
            Locus {
                id: 7,
                name: "exploration".into(),
                default_alleles: [400; 2],
            },
            Locus {
                id: 9,
                name: "courtship".into(),
                default_alleles: [500; 2],
            },
        ],
        mate_desire_locus: Some(9),
    };
    assert!(catalog.validate());
    let mother = founder(
        Sex::Female,
        Some(vec![NamedAlleles {
            locus: 9,
            alleles: [800; 2],
        }]),
    );
    let father = founder(Sex::Male, None);
    assert!(catalog.validate_variants(&mother));
    let reproduction = Inheritance {
        seed: 4,
        mother: 1,
        time: 35,
        mutation_per_million: 0,
        mutation_step: 0,
    };
    let child = reproduction.child(&mother, &father, Some(&catalog));
    assert_eq!(catalog.expression(&child, 7), Some(400));
    assert_eq!(catalog.mate_desire(&child), 650);
    assert_eq!(
        &**child.named_loci.as_ref().unwrap(),
        &[NamedAlleles {
            locus: 9,
            alleles: [800, 500]
        }]
    );
    assert_eq!(reproduction.child(&mother, &father, Some(&catalog)), child);
    assert!(
        reproduction
            .child(&father, &father, Some(&catalog))
            .named_loci
            .is_none()
    );
    let guaranteed_mutation = Inheritance {
        mutation_per_million: 1_000_000,
        mutation_step: 45,
        ..reproduction
    };
    let mutated = guaranteed_mutation.child(&father, &father, Some(&catalog));
    assert!(mutated.named_loci.as_ref().unwrap().iter().all(|row| {
        let default = catalog
            .loci
            .iter()
            .find(|locus| locus.id == row.locus)
            .unwrap()
            .default_alleles[0];
        row.alleles
            .iter()
            .all(|&value| value.abs_diff(default) == 45)
    }));
    assert!(!catalog.validate_variants(&founder(
        Sex::Male,
        Some(vec![NamedAlleles {
            locus: 99,
            alleles: [500; 2]
        }])
    )));
}
