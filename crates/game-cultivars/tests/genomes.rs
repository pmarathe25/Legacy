use game_core::*;
use game_cultivars::*;
use std::sync::Arc;
#[test]
fn saved_genomes_share_storage_without_merging_lineages_or_mutable_values() {
    let genome: Genome = [("cold".into(), game_genetics::Alleles([100, 200]))].into();
    let first = Strain {
        id: 1,
        species: GoodId(2),
        mother: None,
        father: None,
        generation: 0,
        genome: Arc::new(genome),
    };
    let mut child = first.clone();
    child.id = 2;
    child.mother = Some(1);
    child.father = Some(1);
    child.generation = 1;
    let encoded = serde_json::to_string(&[first, child]).unwrap();
    let mut loaded: Vec<Strain> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(serde_json::to_string(&loaded).unwrap(), encoded);
    assert!(Arc::ptr_eq(&loaded[0].genome, &loaded[1].genome));
    assert_ne!(loaded[0].id, loaded[1].id);
    assert_eq!(loaded[1].mother, Some(1));
    Arc::make_mut(&mut loaded[0].genome).insert("cold".into(), game_genetics::Alleles([300, 400]));
    assert_eq!(loaded[1].genome["cold"].0, [100, 200]);
    let reloaded: Vec<Strain> = serde_json::from_str(&encoded).unwrap();
    assert!(Arc::ptr_eq(&loaded[1].genome, &reloaded[0].genome));
}
