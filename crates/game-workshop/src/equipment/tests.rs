use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;

struct Facts {
    actor: Option<ToolActorFacts>,
    nearby: Vec<InventoryId>,
    allocations: ToolAllocations,
    allocation_reads: Cell<usize>,
}
impl ToolSelectionFacts for Facts {
    fn actor(&self, _: PersonId) -> Option<ToolActorFacts> {
        self.actor
    }
    fn nearby(&self, _: PersonId, _: AreaId) -> impl Iterator<Item = InventoryId> {
        self.nearby.iter().copied()
    }
    fn stock(&self, actor: PersonId, stock: &Inventory) -> ToolStockFacts {
        ToolStockFacts {
            accessible: stock.access.contains(&actor)
                || stock.location == InventoryLocation::Carried(actor),
            actual_area: Some(AreaId(1)),
        }
    }
    fn allocations(&self) -> Option<ToolAllocations> {
        self.allocation_reads.set(self.allocation_reads.get() + 1);
        Some(self.allocations.clone())
    }
}
fn setup() -> (Definitions, Store, Facts) {
    let definitions = Definitions::from_content(game_content::Document {
        id: "test/tool-choice".into(),
        version: 1,
        data: Catalog {
            default_recipe: RecipeId("meal".into()),
            recipes: BTreeMap::from([(
                RecipeId("meal".into()),
                Recipe {
                    input_good: Good::Timber,
                    input_quantity: Quantity(1_000),
                    output_good: Good::Food,
                    output_quantity: Quantity(1_000),
                    duration: MINUTE,
                },
            )]),
        },
    })
    .unwrap();
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    game_cargo::register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    insert_site(
        &mut store,
        Site {
            recipe: RecipeId("meal".into()),
            id: SiteId(10),
            area: AreaId(1),
            input: InventoryId(1),
            tool: InventoryId(2),
            output: InventoryId(3),
            access: BTreeSet::from([PersonId(1)]),
            last_granted: None,
        },
    )
    .unwrap();
    for (id, good, location) in [
        (1, Good::Timber, InventoryLocation::Area(AreaId(1))),
        (2, Good::Tool, InventoryLocation::Area(AreaId(1))),
        (3, Good::Food, InventoryLocation::Area(AreaId(1))),
        (4, Good::Tool, InventoryLocation::Carried(PersonId(1))),
        (5, Good::Tool, InventoryLocation::Area(AreaId(1))),
        (6, Good::Tool, InventoryLocation::Area(AreaId(1))),
    ] {
        game_cargo::insert_inventory(
            &mut store,
            Inventory {
                id: InventoryId(id),
                good,
                location,
                quantity: Quantity(1_000),
                capacity: Quantity(10_000),
                access: BTreeSet::from([PersonId(1)]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
    }
    let allocations = ToolAllocations::prepare(
        (2..=6)
            .filter(|id| *id != 3)
            .map(|id| game_cargo::ToolSlots {
                tool: InventoryId(id),
                quantity: Quantity(1_000),
                last_granted: None,
            }),
        [],
    )
    .unwrap();
    let facts = Facts {
        actor: Some(ToolActorFacts {
            alive: true,
            health: 100,
            stationary_area: Some(AreaId(1)),
        }),
        nearby: vec![
            InventoryId(4),
            InventoryId(5),
            InventoryId(6),
            InventoryId(2),
        ],
        allocations,
        allocation_reads: Cell::new(0),
    };
    (definitions, store, facts)
}

#[test]
fn resumed_and_declared_tools_precede_nearby_alternates_without_mutation() {
    let (definitions, mut store, facts) = setup();
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(2))
    );
    insert_job(
        &mut store,
        PersonId(1),
        Job {
            site: SiteId(10),
            remaining: 10,
            rate: SkillRate::default(),
            body_rate: SkillRate::default(),
            work_remainder: 0,
            tool: Some(InventoryId(6)),
        },
    )
    .unwrap();
    let before = store.snapshot().unwrap();
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(6))
    );
    assert_eq!(store.snapshot().unwrap(), before);
}

#[test]
fn cargo_allocation_keeps_other_workers_and_chooses_a_carried_alternate() {
    let (definitions, mut store, mut facts) = setup();
    facts.allocations = ToolAllocations::prepare(
        (2..=6)
            .filter(|id| *id != 3)
            .map(|id| game_cargo::ToolSlots {
                tool: InventoryId(id),
                quantity: Quantity(1_000),
                last_granted: None,
            }),
        [game_cargo::ToolUse {
            actor: PersonId(2),
            tool: InventoryId(2),
        }],
    )
    .unwrap();
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(4))
    );
    game_cargo::inventory_mut(&mut store, InventoryId(2))
        .unwrap()
        .access
        .clear();
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(4))
    );
}

#[test]
fn consuming_a_tool_as_input_cannot_spend_the_last_unit_needed_to_work() {
    let (definitions, mut store, mut facts) = setup();
    let mut catalog = definitions.catalog.clone();
    catalog
        .recipes
        .get_mut(&RecipeId("meal".into()))
        .unwrap()
        .input_good = Good::Tool;
    let definitions = Definitions::from_content(game_content::Document {
        id: "test/tool-input".into(),
        version: 1,
        data: catalog,
    })
    .unwrap();
    site_mut(&mut store, SiteId(10)).unwrap().input = InventoryId(2);
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(4))
    );
    game_cargo::inventory_mut(&mut store, InventoryId(2))
        .unwrap()
        .quantity = Quantity(2_000);
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(2))
    );
    facts.allocations = ToolAllocations::prepare(
        [game_cargo::ToolSlots {
            tool: InventoryId(2),
            quantity: Quantity(2_000),
            last_granted: None,
        }],
        [game_cargo::ToolUse {
            actor: PersonId(2),
            tool: InventoryId(2),
        }],
    )
    .unwrap();
    assert_eq!(
        definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
        Some(InventoryId(4))
    );
}

#[test]
fn unavailable_actor_or_wrong_site_area_never_requests_allocations() {
    let (definitions, store, mut facts) = setup();
    for actor in [
        None,
        Some(ToolActorFacts {
            alive: false,
            health: 100,
            stationary_area: Some(AreaId(1)),
        }),
        Some(ToolActorFacts {
            alive: true,
            health: 0,
            stationary_area: Some(AreaId(1)),
        }),
        Some(ToolActorFacts {
            alive: true,
            health: 100,
            stationary_area: None,
        }),
        Some(ToolActorFacts {
            alive: true,
            health: 100,
            stationary_area: Some(AreaId(2)),
        }),
    ] {
        facts.actor = actor;
        assert_eq!(
            definitions.select_work_tool(&store, PersonId(1), SiteId(10), &facts),
            None
        );
    }
    assert_eq!(facts.allocation_reads.get(), 0);
}

#[test]
fn visible_work_options_require_only_contact_and_site_permission() {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = Store::with_schema(schema);
    // These declarations have no input/tool/output stocks or workers yet.
    // Discovery stays permissive; admitting real work is a separate operation.
    for (id, area, access) in [
        (30, 1, BTreeSet::from([PersonId(1)])),
        (10, 1, BTreeSet::from([PersonId(1)])),
        (20, 2, BTreeSet::from([PersonId(1)])),
        (40, 1, BTreeSet::from([PersonId(2)])),
    ] {
        insert_site(
            &mut store,
            Site {
                id: SiteId(id),
                area: AreaId(area),
                recipe: RecipeId("meal".into()),
                input: InventoryId(100),
                tool: InventoryId(101),
                output: InventoryId(102),
                access,
                last_granted: None,
            },
        )
        .unwrap();
    }
    let opening = store.snapshot().unwrap();
    assert_eq!(
        local_work_sites(&store, PersonId(1), Some(AreaId(1))),
        vec![SiteId(10), SiteId(30)]
    );
    assert_eq!(
        local_work_sites(&store, PersonId(1), Some(AreaId(2))),
        vec![SiteId(20)]
    );
    assert_eq!(
        local_work_sites(&store, PersonId(2), Some(AreaId(1))),
        vec![SiteId(40)]
    );
    assert!(local_work_sites(&store, PersonId(3), Some(AreaId(1))).is_empty());
    assert!(local_work_sites(&store, PersonId(1), None).is_empty());
    assert_eq!(store.snapshot().unwrap(), opening);
}
