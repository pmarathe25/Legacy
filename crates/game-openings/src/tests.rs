use super::*;
fn definitions() -> Definitions {
    Definitions::from_content(
        game_content::parse(include_str!("../../../content/openings/temperate.toml")).unwrap(),
    )
    .unwrap()
}
#[test]
fn authored_plans_control_names_stocks_and_population_scaling() {
    let mut defs = definitions();
    let (_, plan) = defs.opening_plan("settlement-founder").unwrap();
    assert_eq!(plan.definition.areas[0].name, "Willowbank");
    assert_eq!(
        plan.stocks
            .iter()
            .find(|stock| stock.definition.key == "food")
            .unwrap()
            .quantity,
        Quantity(3_000_000)
    );
    let document = Document {
        id: defs.content.id.clone(),
        version: defs.content.version,
        data: defs.catalog.clone(),
    };
    let mut changed = document;
    changed.data.settlements.get_mut("ordinary").unwrap().areas[0].name = "New ford".into();
    changed.data.settlements.get_mut("ordinary").unwrap().stocks[0].quantity = Quantity(2_000_000);
    defs = Definitions::from_content(changed).unwrap();
    let (_, plan) = defs.opening_plan("settlement-founder").unwrap();
    assert_eq!(plan.definition.areas[0].name, "New ford");
    assert_eq!(plan.stocks[0].quantity, Quantity(2_000_000));
    let plan = defs.catalog.settlements["populated"].plan(128).unwrap();
    assert_eq!(plan.stocks[0].quantity, Quantity(128 * 180_000));
    assert_eq!(plan.stocks[0].capacity, Quantity(128 * 720_000));
}
#[test]
fn resolved_definitions_roundtrip_without_a_source_and_detect_tampering() {
    let defs = definitions();
    let saved = serde_json::to_vec(&defs).unwrap();
    let restored: Definitions = serde_json::from_slice(&saved).unwrap();
    restored.validate().unwrap();
    assert_eq!(defs, restored);
    let mut corrupt = restored;
    corrupt
        .catalog
        .settlements
        .get_mut("ordinary")
        .unwrap()
        .stocks[0]
        .quantity = Quantity(1);
    assert!(corrupt.validate().is_err());
}
#[test]
fn unknown_refs_duplicate_keys_bad_capacity_and_selectors_are_rejected() {
    for change in 0..4 {
        let mut defs = definitions();
        let settlement = defs.catalog.settlements.get_mut("ordinary").unwrap();
        match change {
            0 => {
                settlement.stocks[0].placement = Placement::Area {
                    area: "missing".into(),
                }
            }
            1 => settlement.stocks.push(settlement.stocks[0].clone()),
            2 => settlement.stocks[0].capacity = Quantity(1),
            _ => settlement.workshops[0].sponsor = 99,
        }
        assert!(defs.catalog.validate().is_err());
    }
}
#[test]
fn random_openings_keep_the_same_seeded_order_and_accept_added_existing_mechanics() {
    let defs = definitions();
    for seed in 0_u64..100 {
        let mixed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        assert_eq!(
            defs.catalog.choose(seed).unwrap(),
            defs.catalog.random_order[((mixed >> 32) % 3) as usize]
        );
    }
    let mut catalog = defs.catalog;
    catalog.openings.insert(
        "new-ford".into(),
        catalog.openings["settlement-founder"].clone(),
    );
    catalog.random_order.push("new-ford".into());
    catalog.validate().unwrap();
    assert!((0..100).any(|seed| catalog.choose(seed).unwrap() == "new-ford"));
}

#[test]
fn historical_stock_is_authored_and_invalid_established_bindings_are_rejected() {
    let mut defs = definitions();
    let ordinary = &mut defs
        .catalog
        .settlements
        .get_mut("ordinary")
        .unwrap()
        .workshops[0];
    assert_eq!(
        ordinary.historical_materials.as_ref().unwrap().quantity,
        Quantity(5_000)
    );
    ordinary.historical_materials.as_mut().unwrap().quantity = Quantity(101_000);
    assert!(defs.catalog.validate().is_err());
    defs.catalog
        .settlements
        .get_mut("ordinary")
        .unwrap()
        .workshops[0]
        .historical_materials = None;
    assert!(defs.catalog.validate().is_err());
}
