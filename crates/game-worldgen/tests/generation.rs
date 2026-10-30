use game_worldgen::*;
#[path = "support/catalog.rs"]
mod catalog;
#[test]
fn seeds_generate_only_physical_and_individual_initial_conditions() {
    let settings = load(&mut game_content::Store::standard()).unwrap();
    let catalog = game_traits::load(&mut game_content::Store::standard()).unwrap();
    let first = generate(&settings, 17, &catalog).unwrap();
    assert_eq!(first, generate(&settings, 17, &catalog).unwrap());
    assert_ne!(first, generate(&settings, 18, &catalog).unwrap());
    assert!(first.institutions.is_empty());
    assert!(first.bonds.is_empty());
    assert!(first.plots.is_empty());
    assert_eq!(
        first.actors().len(),
        settings.population + settings.guardians
    );
    assert_eq!(first.links.len(), settings.areas - 1);
    assert!(first.people.iter().all(|p| p.count == 1));
    assert!(
        first
            .inventories
            .iter()
            .all(|i| i.area.is_none() && i.access == [i.owner.clone()])
    );
    let mut invalid = settings.clone();
    invalid.population = 2;
    assert!(generate(&invalid, 17, &catalog).is_err());
    invalid = settings;
    invalid.areas = invalid.population;
    assert!(generate(&invalid, 17, &catalog).is_err());
}

#[test]
fn generated_individuals_use_new_authored_traits_and_their_actual_ranges() {
    let settings = load(&mut game_content::Store::standard()).unwrap();
    let mut catalog = game_traits::load(&mut game_content::Store::standard()).unwrap();
    catalog.definitions.insert(
        "curiosity".into(),
        game_traits::Trait {
            name: "Curiosity".into(),
            minimum: -20,
            maximum: -10,
            default: -15,
            founder_spread: 65535,
            weights: Default::default(),
        },
    );
    let generated = generate(&settings, 1, &catalog).unwrap();
    assert!(
        generated
            .people
            .iter()
            .all(|p| (-20..=-10).contains(&p.traits["curiosity"]))
    );
    catalog.definitions.get_mut("curiosity").unwrap().maximum = -20;
    assert!(generate(&settings, 1, &catalog).is_err());
}

#[test]
fn headless_physical_conditions_can_omit_immortal_guardians() {
    let mut settings = load(&mut game_content::Store::standard()).unwrap();
    settings.guardians = 0;
    let catalog = game_traits::load(&mut game_content::Store::standard()).unwrap();
    let generated = generate(&settings, 7, &catalog).unwrap();
    assert_eq!(generated.people.len(), settings.population);
    assert!(
        generated
            .people
            .iter()
            .all(|p| p.kind == game_physiology::Kind::Mortal)
    );
}
