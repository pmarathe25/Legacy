use game_core::PersonId;
use game_relationships::{Relationship, Social};
use game_sim::Resource;

#[test]
fn changed_validation_checks_new_and_updated_links_and_values() {
    let mut previous = Social::default();
    for id in 1..100 {
        previous
            .relationships
            .entry(PersonId(id))
            .or_default()
            .insert(PersonId(id + 100), Relationship::default());
        previous
            .values
            .entry(PersonId(id))
            .or_default()
            .insert("kindness".into(), 1);
    }
    previous.validate().unwrap();
    previous.validate_changes(&previous).unwrap();
    let mut changed = previous.clone();
    changed
        .relationships
        .get_mut(&PersonId(50))
        .unwrap()
        .get_mut(&PersonId(150))
        .unwrap()
        .trust = 101;
    assert!(changed.validate_changes(&previous).is_err());
    let mut changed = previous.clone();
    changed
        .relationships
        .entry(PersonId(200))
        .or_default()
        .insert(PersonId(200), Relationship::default());
    assert!(changed.validate_changes(&previous).is_err());
    let mut changed = previous.clone();
    changed
        .values
        .get_mut(&PersonId(50))
        .unwrap()
        .insert("kindness".into(), 101);
    assert!(changed.validate_changes(&previous).is_err());
    let mut changed = previous.clone();
    changed
        .values
        .entry(PersonId(200))
        .or_default()
        .insert(" ".into(), 0);
    assert!(changed.validate_changes(&previous).is_err());
    // Valid changes and deletion remain admissible, and snapshots stay independent.
    let mut changed = previous.clone();
    changed.relationships.remove(&PersonId(50));
    changed
        .values
        .get_mut(&PersonId(50))
        .unwrap()
        .insert("kindness".into(), -100);
    changed.validate_changes(&previous).unwrap();
    assert_eq!(previous.values[&PersonId(50)]["kindness"], 1);
}
