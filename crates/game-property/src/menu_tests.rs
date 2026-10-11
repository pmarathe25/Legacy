use crate::*;
use game_core::*;
use game_ecs::Store;
use std::collections::{BTreeMap, BTreeSet};
fn fixture() -> (Runtime, Store) {
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    (Runtime::default(), Store::with_schema(schema))
}
fn actor() -> ActorFacts {
    ActorFacts {
        actor: PersonId(1),
        now: SimTime(0),
        area: Some(AreaId(3)),
        capable: true,
        exertion_ready: true,
    }
}
fn stock(id: u64, allowed: bool) -> StockFacts {
    StockFacts {
        id: InventoryId(id),
        area: Some(AreaId(3)),
        good: Good::Food,
        quantity: Quantity(1_000),
        room: Quantity(4_000),
        carried_by: None,
        actor_allowed: allowed,
        custodian: Some(PersonId(2)),
    }
}
fn contact() -> ContactFacts {
    ContactFacts {
        actor: PersonId(2),
        area: Some(AreaId(3)),
        present: true,
        capable: true,
        available: true,
        autonomous: true,
    }
}
fn menu<'a>(stocks: &'a [StockFacts], contacts: &'a [ContactFacts]) -> MenuFacts<'a> {
    MenuFacts {
        actor: actor(),
        quantity: Quantity(1_000),
        cargo: Some(StockFacts {
            id: InventoryId(10),
            carried_by: Some(PersonId(1)),
            actor_allowed: true,
            ..stock(10, true)
        }),
        can_receive_quantity: true,
        stocks,
        contacts,
        recipient_access: &[],
    }
}
#[test]
fn local_stock_and_owner_taking_rules_produce_bounded_property_intents() {
    let (runtime, store) = fixture();
    let stocks = [stock(2, false)];
    let contacts = [contact()];
    let actions = runtime.available_actions(&store, menu(&stocks, &contacts));
    assert_eq!(
        actions,
        vec![
            PropertyAction::Take {
                source: InventoryId(2),
                destination: InventoryId(10),
                quantity: Quantity(1_000)
            },
            PropertyAction::RequestAccess {
                inventory: InventoryId(2)
            }
        ]
    );
    let mut facts = menu(&stocks, &contacts);
    facts.can_receive_quantity = false;
    assert_eq!(
        runtime.available_actions(&store, facts),
        vec![PropertyAction::RequestAccess {
            inventory: InventoryId(2)
        }]
    );
    let mut facts = menu(&stocks, &contacts);
    facts.actor.capable = false;
    assert!(runtime.available_actions(&store, facts).is_empty());
}
#[test]
fn claiming_opinions_and_recipient_access_stay_personal_in_owner_menu() {
    let (runtime, mut store) = fixture();
    let view = ClaimView {
        id: ClaimId(20),
        claimant: PersonId(2),
        inventory: InventoryId(2),
        rights: BTreeSet::from([ClaimRight::Transfer]),
        evidence: Vec::new(),
        recognized_by: BTreeSet::new(),
        opposed_by: BTreeSet::new(),
    };
    insert_knowledge(
        &mut store,
        PersonId(1),
        Knowledge {
            claims: BTreeMap::from([(view.id, view)]),
            ..Knowledge::default()
        },
    )
    .unwrap();
    let mut owned = stock(2, true);
    owned.custodian = Some(PersonId(1));
    let contacts = [contact()];
    let stocks = [owned];
    let access = [(InventoryId(2), PersonId(2))];
    let mut facts = menu(&stocks, &contacts);
    facts.recipient_access = &access;
    assert_eq!(
        runtime.available_actions(&store, facts),
        vec![
            PropertyAction::Recognize { claim: ClaimId(20) },
            PropertyAction::Contest { claim: ClaimId(20) },
            PropertyAction::Share {
                inventory: InventoryId(2),
                recipient: PersonId(2),
                allowed: false
            }
        ]
    );
    assert!(known_claims(&store, PersonId(2)).is_none());
}
