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

#[test]
fn concrete_customs_are_selected_by_content_and_resolve_to_the_original_native_bundle() {
    let definitions = definitions();
    let (_, camp) = definitions.opening_plan("settlement-founder").unwrap();
    let (_, office) = definitions.opening_plan("established-ruler").unwrap();
    let residents = (101..116).map(PersonId).collect::<Vec<_>>();
    let areas = BTreeMap::from([
        ("centre".into(), AreaId(1)),
        ("fields".into(), AreaId(2)),
        ("wood".into(), AreaId(3)),
    ]);
    let stocks = BTreeMap::from([("food".into(), InventoryId(201))]);
    let camp = camp.definition.customs["camp-provisioning"]
        .resolve(&residents, &areas, &stocks, InventoryId(202), SimTime(10))
        .unwrap();
    let office = office.definition.customs["supported-office"]
        .resolve(&residents, &areas, &stocks, InventoryId(202), SimTime(10))
        .unwrap();
    assert_eq!(
        camp.charter.purpose,
        "Willowbank camp provisioning association"
    );
    assert_eq!(camp.charter.offices[0].holder, PersonId(105));
    assert_eq!(office.charter.offices[0].holder, PersonId(101));
    assert_eq!(
        camp.charter.founders,
        BTreeSet::from([
            PersonId(101),
            PersonId(105),
            PersonId(106),
            PersonId(107),
            PersonId(108)
        ])
    );
    assert_eq!(camp.source_access, BTreeSet::from([PersonId(101)]));
    assert_eq!(camp.charter.procedure.minimum_support, 3);
    assert_eq!(camp.fulfil_step, SimDuration(60_000));
    assert!(matches!(
        camp.initial_delivery,
        Some(game_agreements::AgreementTerm::Delivery {
            debtor: PersonId(101),
            creditor: PersonId(105),
            source: InventoryId(201),
            destination: InventoryId(202),
            quantity: Quantity(1000),
            deadline: SimTime(86_400_010)
        })
    ));
}
#[test]
fn modifying_an_existing_custom_instance_uses_toml_and_survives_saved_definitions() {
    let source = include_str!("../../../content/openings/temperate.toml")
        .replacen(
            "purpose = \"Willowbank camp provisioning association\"",
            "purpose = \"New ford provisioning association\"",
            1,
        )
        .replacen(
            "quantity=1000, deadline_after=86400000",
            "quantity=2000, deadline_after=172800000",
            1,
        );
    let definitions = Definitions::from_content(game_content::parse(&source).unwrap()).unwrap();
    let bytes = serde_json::to_vec(&definitions).unwrap();
    let restored: Definitions = serde_json::from_slice(&bytes).unwrap();
    restored.validate().unwrap();
    let (opening, plan) = restored.opening_plan("settlement-founder").unwrap();
    let custom = &plan.definition.customs[&opening.starting_custom];
    assert_eq!(custom.purpose, "New ford provisioning association");
    assert_eq!(custom.contributions[0].quantity, Quantity(2000));
    assert_eq!(
        custom.contributions[0].deadline_after,
        SimDuration(172_800_000)
    );
}
#[test]
fn invalid_authored_custom_references_rights_capacity_offices_and_delivery_are_rejected() {
    for edit in 0..9 {
        let mut definitions = definitions();
        let custom = definitions
            .catalog
            .settlements
            .get_mut("ordinary")
            .unwrap()
            .customs
            .get_mut("camp-provisioning")
            .unwrap();
        match edit {
            0 => custom.claimant = 999,
            1 => custom.source = "missing-stock".into(),
            2 => custom.claim_rights.clear(),
            3 => custom.fund.capacity = Quantity(1),
            4 => custom.procedure.minimum_support = 65,
            5 => custom.offices[0].holder = 14,
            6 => custom.access_rules[0].authority = 99,
            7 => custom.contributions[0].deadline_after = SimDuration(0),
            _ => custom.fulfilled_contribution = Some(99),
        }
        assert!(definitions.catalog.validate().is_err(), "edit {edit}");
    }
    let mut definitions = definitions();
    definitions
        .catalog
        .openings
        .get_mut("settlement-founder")
        .unwrap()
        .starting_custom = "missing-custom".into();
    assert!(definitions.catalog.validate().is_err());
    let malformed = include_str!("../../../content/openings/temperate.toml").replacen(
        "fulfilled_contribution = 0",
        "fulfilled_contribution = 99",
        1,
    );
    assert!(Definitions::from_content(game_content::parse(&malformed).unwrap()).is_err());
}
