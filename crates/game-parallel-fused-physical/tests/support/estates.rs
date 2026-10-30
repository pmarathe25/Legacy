use game_parallel::{Person, PropertyInheritance, World};
use game_parallel_fused_physical::{Kernel, PropertyCommand};
fn world() -> World {
    let mut state =
        World::from_toml(include_str!("../../../../content/parallel/theft.toml")).unwrap();
    state.autowork = None;
    state.families.as_mut().unwrap().members[1].age = 59;
    state.property.as_mut().unwrap().inheritance = Some(PropertyInheritance {
        minimum_age: 12,
        prefer_partner: true,
    });
    state
}
#[test]
fn living_partner_inherits_actual_stock_title_and_old_permissions_and_offers_end() {
    let mut state = world();
    state.families.as_mut().unwrap().members[0].partner = Some(2);
    state.families.as_mut().unwrap().members[1].partner = Some(1);
    state.property.as_mut().unwrap().claims[0].delegates = vec![3];
    let mut kernel = Kernel::compile(state).unwrap();
    kernel
        .property_command(PropertyCommand::Offer {
            owner: 2,
            recipient: 3,
            region: 1,
            good: 1,
        })
        .unwrap();
    kernel.step().unwrap();
    let result = kernel.export();
    assert!(
        !result
            .families
            .as_ref()
            .unwrap()
            .members
            .iter()
            .find(|row| row.id == 2)
            .unwrap()
            .alive
    );
    let property = result.property.as_ref().unwrap();
    assert_eq!(property.claims[0].owner, 1);
    assert!(property.claims[0].delegates.is_empty());
    assert!(property.offers.is_empty());
    assert_eq!(
        (property.estates[0].previous_owner, property.estates[0].heir),
        (2, 1)
    );
    assert_eq!(
        result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        9
    );
    assert_eq!(
        game_parallel_observation::ruler(&result)
            .unwrap()
            .estates
            .len(),
        1
    );
    assert_eq!(
        Kernel::compile(World::from_json(&serde_json::to_string(&result).unwrap()).unwrap())
            .unwrap()
            .export(),
        result
    );
    kernel.step().unwrap();
    assert_eq!(kernel.export().property.unwrap().estates.len(), 1);
}
#[test]
fn estate_waits_until_a_child_meets_the_authored_age_and_is_worker_invariant() {
    let mut state = world();
    state.property.as_mut().unwrap().inheritance = Some(PropertyInheritance {
        minimum_age: 14,
        prefer_partner: false,
    });
    state.regions[0].people.push(Person {
        id: 9,
        hunger: 0,
        health: 100,
    });
    let family = state.families.as_mut().unwrap();
    let mut child = family.members[0].clone();
    child.id = 9;
    child.born = None;
    child.age = 11;
    child.parents = vec![1, 2].into();
    child.partner = None;
    child.pregnancy = None;
    child.reproduction_allowed = false;
    family.members.push(child);
    family.next_id = 10;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(state.clone()).unwrap();
                kernel.step().unwrap();
                let waiting = kernel.export();
                assert_eq!(waiting.property.as_ref().unwrap().claims[0].owner, 2);
                assert!(waiting.property.as_ref().unwrap().estates.is_empty());
                kernel = Kernel::compile(
                    World::from_json(&serde_json::to_string(&waiting).unwrap()).unwrap(),
                )
                .unwrap();
                kernel.step().unwrap();
                assert_eq!(kernel.export().property.unwrap().claims[0].owner, 2);
                kernel.step().unwrap();
                kernel.export()
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.property.as_ref().unwrap().claims[0].owner, 9);
    assert_eq!(result.property.as_ref().unwrap().estates[0].heir, 9);
    result.validate().unwrap();
}
#[test]
fn inheritance_is_rejected_without_families_and_failed_ticks_do_not_transfer_title() {
    let mut invalid = world();
    invalid.families = None;
    invalid.lineage = None;
    invalid.guardians = None;
    assert!(invalid.validate().is_err());
    let mut reference = world();
    let before = reference.clone();
    assert!(reference.step().is_err());
    assert_eq!(reference, before);
    assert!(game_parallel_blocks::Engine::compile(reference).is_err());
    let mut failing = world();
    failing.families.as_mut().unwrap().members[2].age = u64::MAX;
    let mut kernel = Kernel::compile(failing).unwrap();
    let before = kernel.export();
    assert!(kernel.step().is_err());
    assert_eq!(kernel.export(), before);
}

#[test]
fn accepting_a_stock_title_cancels_only_competing_offers() {
    let mut state = world();
    state
        .property
        .as_mut()
        .unwrap()
        .claims
        .push(game_parallel::PropertyClaim {
            region: 2,
            good: 1,
            owner: 5,
            delegates: vec![],
        });
    state.property.as_mut().unwrap().claims[0].delegates.push(4);
    let mut kernel = Kernel::compile(state).unwrap();
    let offer = |owner, recipient, region| PropertyCommand::Offer {
        owner,
        recipient,
        region,
        good: 1,
    };
    let first = kernel.property_command(offer(2, 3, 1)).unwrap().unwrap();
    let competing = kernel.property_command(offer(2, 4, 1)).unwrap().unwrap();
    let unrelated = kernel.property_command(offer(5, 6, 2)).unwrap().unwrap();
    let draft = kernel.prepare().unwrap();
    kernel
        .property_command(PropertyCommand::Decide {
            id: first,
            actor: 3,
            accept: true,
        })
        .unwrap();
    assert!(kernel.commit(draft).is_err());
    let result = kernel.export();
    let property = result.property.as_ref().unwrap();
    assert_eq!(property.claims[0].owner, 3);
    assert!(property.claims[0].delegates.is_empty());
    assert_eq!(
        property
            .offers
            .iter()
            .map(|offer| offer.id)
            .collect::<Vec<_>>(),
        vec![unrelated]
    );
    assert!(!property.offers.iter().any(|offer| offer.id == competing));
    assert_eq!(
        result.production.as_ref().unwrap().inventories[0].stocks[0].quantity,
        9
    );
    result.validate().unwrap();
}
