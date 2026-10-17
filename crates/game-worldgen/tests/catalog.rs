use std::collections::BTreeSet;
#[test]
fn regional_catalogs_have_physical_sources_and_complete_production_chains() {
    let mut store = game_content::Store::standard();
    let settings = game_worldgen::load(&mut store).unwrap();
    let objects = game_objects::load(&mut store).unwrap();
    let crops = game_agriculture::load(&mut store, &objects).unwrap();
    let recipes = game_workshop::load(&mut store, &objects).unwrap();
    let mut available = BTreeSet::from([objects.resolve("grain").unwrap()]);
    for b in &settings.biomes {
        game_environment::load(&mut store, &b.climate).unwrap();
        available.insert(objects.resolve(&b.food).unwrap());
        available.insert(objects.resolve(&b.material).unwrap());
        for r in &b.resources {
            available.insert(objects.resolve(&r.object).unwrap());
            assert!(r.renew_per_person <= r.stock_per_person);
        }
    }
    loop {
        let count = available.len();
        for r in recipes.recipes.values() {
            if r.inputs.keys().all(|i| available.contains(i)) {
                available.insert(r.output);
            }
        }
        for o in objects.items.values() {
            if available.contains(&o.id) {
                for output in o.tags.iter().filter_map(|t| t.strip_prefix("matures:")) {
                    available.insert(objects.resolve(output).unwrap());
                }
            }
        }
        for c in crops.items.values() {
            if available.contains(&c.seed) {
                available.insert(c.output);
            }
        }
        if available.len() == count {
            break;
        }
    }
    for (key, o) in &objects.items {
        // This one object belongs to a low-level combat/workshop test fixture.
        if key != "training-tool" {
            assert!(
                available.contains(&o.id),
                "{key} has no physical source or executable production chain"
            );
        }
    }
}
#[test]
fn each_biome_generates_all_its_independent_resource_patches() {
    let mut settings = game_worldgen::load(&mut game_content::Store::standard()).unwrap();
    let traits = game_traits::load(&mut game_content::Store::standard()).unwrap();
    settings.population = 4;
    settings.areas = 1;
    for biome in settings.biomes.clone() {
        settings.biomes = vec![biome.clone()];
        let world = game_worldgen::generate(&settings, 3, &traits).unwrap();
        assert_eq!(world.patches.len(), 2 + biome.resources.len());
        for r in &biome.resources {
            assert!(
                world
                    .patches
                    .iter()
                    .any(|p| p.object == r.object && p.renew == 6 * r.renew_per_person)
            );
        }
        assert!(world.plots.is_empty());
        assert!(world.institutions.is_empty());
    }
    settings.biomes[0].resources[0].stock_per_person = u64::MAX;
    assert!(settings.validate().is_err());
}

#[test]
fn authored_food_abundance_changes_physical_productivity_without_social_structure() {
    let mut settings = game_worldgen::load(&mut game_content::Store::standard()).unwrap();
    let traits = game_traits::load(&mut game_content::Store::standard()).unwrap();
    settings.population = 32;
    settings.areas = 4;
    settings.biomes.truncate(1);
    assert!(
        serde_json::to_value(&settings).unwrap()["biomes"][0]
            .get("food_renew_per_person")
            .is_none()
    );
    let default = game_worldgen::generate(&settings, 4, &traits).unwrap();
    assert_eq!(default.patches[0].stock, 10 * 600);
    assert_eq!(default.patches[0].renew, 10 * 400);
    settings.biomes[0].food_stock_per_person = Some(600);
    settings.biomes[0].food_renew_per_person = Some(400);
    assert_eq!(
        default,
        game_worldgen::generate(&settings, 4, &traits).unwrap()
    );
    settings.biomes[0].food_stock_per_person = Some(60);
    settings.biomes[0].food_renew_per_person = Some(3);
    let limited = game_worldgen::generate(&settings, 4, &traits).unwrap();
    assert_eq!(limited.patches[0].stock, 10 * 60);
    assert_eq!(limited.patches[0].renew, 10 * 3);
    assert_eq!(default.people, limited.people);
    assert!(limited.institutions.is_empty());
    settings.biomes[0].food_renew_per_person = Some(61);
    assert!(settings.validate().is_err());
}
