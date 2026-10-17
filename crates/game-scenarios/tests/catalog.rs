use game_content::Store;
use game_scenarios::*;
#[test]
fn scenario_discovery_and_symbol_validation_are_independent_of_the_application() {
    let mut store = Store::fixtures();
    let scenarios = load(&mut store).unwrap();
    assert!(scenarios.contains_key("foraging-fixture"));
    let mut bad = scenarios["kernel-fixture"].data.clone();
    bad.inventories[0].owner = "unknown".into();
    assert!(bad.validate().is_err());
    let data = &scenarios["kernel-fixture"].data;
    assert_eq!(data.select(&["citizen".into()]).unwrap().len(), 12);
    assert_eq!(data.select(&["*".into()]).unwrap().len(), 15);
}
