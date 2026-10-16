use game_content::Store;
use game_openings::*;
use game_sim::*;
fn builder() -> Builder {
    let mut b = Builder::new();
    game_land::plugin(&mut b).unwrap();
    game_transport::plugin(&mut b).unwrap();
    game_objects::plugin(&mut b).unwrap();
    game_traits::plugin(&mut b).unwrap();
    game_cargo::plugin(&mut b).unwrap();
    game_property::plugin(&mut b).unwrap();
    game_environment::plugin(&mut b).unwrap();
    game_physiology::plugin(&mut b).unwrap();
    game_knowledge::plugin(&mut b).unwrap();
    game_skills::plugin(&mut b).unwrap();
    game_ecology::plugin(&mut b).unwrap();
    game_agriculture::plugin(&mut b).unwrap();
    game_workshop::plugin(&mut b).unwrap();
    game_health::plugin(&mut b).unwrap();
    game_relationships::plugin(&mut b).unwrap();
    game_agreements::plugin(&mut b).unwrap();
    game_institutions::plugin(&mut b).unwrap();
    game_justice::plugin(&mut b).unwrap();
    game_lifecycle::plugin(&mut b).unwrap();
    game_lineage::plugin(&mut b).unwrap();
    game_guardians::plugin(&mut b).unwrap();
    game_invocation::plugin(&mut b).unwrap();
    game_social_life::plugin(&mut b).unwrap();
    game_characters::plugin(&mut b).unwrap();
    game_logistics::plugin(&mut b).unwrap();
    game_projects::plugin(&mut b).unwrap();
    game_realm::plugin(&mut b).unwrap();
    game_conflict::plugin(&mut b).unwrap();
    game_confinement::plugin(&mut b).unwrap();
    game_missions::plugin(&mut b).unwrap();
    game_armies::plugin(&mut b).unwrap();
    game_pressure::plugin(&mut b).unwrap();
    game_communities::plugin(&mut b).unwrap();
    game_openings::plugin(&mut b).unwrap();
    b
}
#[test]
fn fixtures_initialize_through_owner_requests_without_the_application() {
    for scenario in scenarios(&mut Store::standard()).unwrap() {
        let mut h = builder().build(3, 1).unwrap();
        initialize(&mut h, &mut Store::standard(), &scenario.id, 3).unwrap();
        let definition = h.get::<Opening>().unwrap().definition.as_ref().unwrap();
        let expected = definition.actors().len() + usize::from(definition.reincarnation.is_some());
        assert_eq!(
            h.get::<game_physiology::Bodies>().unwrap().people.len(),
            expected
        );
        assert_eq!(h.get::<Opening>().unwrap().scenario, scenario.id);
        assert_eq!(scenario.fixture, scenario.id != "generated");
        let save = h.save().unwrap();
        assert_eq!(builder().load(&save, 2).unwrap().save().unwrap(), save);
    }
}
#[test]
fn unsupported_scenarios_are_rejected_before_initialization() {
    let mut h = builder().build(3, 1).unwrap();
    let before = h.save().unwrap();
    assert!(initialize(&mut h, &mut Store::standard(), "not-an-opening", 3).is_err());
    assert_eq!(before, h.save().unwrap());
}
#[test]
fn an_existing_opening_cannot_be_partially_reinitialized() {
    let mut h = builder().build(3, 1).unwrap();
    initialize(&mut h, &mut Store::standard(), "kernel-fixture", 3).unwrap();
    let before = h.save().unwrap();
    assert!(initialize(&mut h, &mut Store::standard(), "kernel-fixture", 3).is_err());
    assert_eq!(before, h.save().unwrap());
}
