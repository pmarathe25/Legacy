use game_parallel::{Pregnancy, World};
use game_parallel_fused_physical::Kernel;

#[test]
fn either_person_can_leave_and_a_changed_intention_does_not_cancel_a_pregnancy() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let family = world.families.as_mut().unwrap();
    for member in &mut family.members {
        member.reproduction_allowed = false;
    }
    family.members[0].partner = Some(2);
    family.members[0].pregnancy = Some(Pregnancy { father: 2, due: 3 });
    family.members[1].partner = Some(1);
    family.members[1].reproduction_allowed = true;
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut kernel = Kernel::compile(world.clone()).unwrap();
                let draft = kernel.prepare().unwrap();
                kernel.set_reproduction(1, false).unwrap();
                assert!(kernel.commit(draft).is_err());
                kernel.leave_partnership(2).unwrap();
                let result = kernel.export();
                let members = &result.families.as_ref().unwrap().members;
                assert_eq!(members[0].partner, None);
                assert_eq!(members[1].partner, None);
                assert_eq!(members[0].pregnancy.as_ref().unwrap().father, 2);
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                let mut kernel = Kernel::compile(restored).unwrap();
                for _ in 0..3 {
                    kernel.step().unwrap();
                }
                let result = kernel.export();
                assert_eq!(result.families.as_ref().unwrap().totals.births, 1);
                assert!(
                    result
                        .families
                        .as_ref()
                        .unwrap()
                        .members
                        .iter()
                        .any(|member| member.parents.as_slice() == [1, 2])
                );
                result
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn opting_out_is_independent_and_invalid_edits_are_atomic() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let mut kernel = Kernel::compile(world).unwrap();
    let before = kernel.export();
    assert!(kernel.leave_partnership(1).is_err());
    assert!(kernel.set_reproduction(999, false).is_err());
    assert_eq!(kernel.export(), before);
    kernel.set_reproduction(1, false).unwrap();
    for _ in 0..4 {
        kernel.step().unwrap();
    }
    let result = kernel.export();
    let member = result
        .families
        .unwrap()
        .members
        .into_iter()
        .find(|member| member.id == 1)
        .unwrap();
    assert!(!member.reproduction_allowed);
    assert!(member.partner.is_none());
    assert!(member.pregnancy.is_none());
}

#[test]
fn a_person_may_reconsider_an_incompatible_partner_without_their_permission() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    let family = world.families.as_mut().unwrap();
    for member in &mut family.members {
        member.reproduction_allowed = false;
    }
    family.members[0].partner = Some(2);
    family.members[0].reproduction_allowed = true;
    family.members[1].partner = Some(1);
    let run = |workers, reconsider: bool, controlled: bool| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut state = world.clone();
                if controlled {
                    state.lineage = World::from_toml(include_str!(
                        "../../../../content/parallel/campaign.toml"
                    ))
                    .unwrap()
                    .lineage;
                }
                let mut kernel = Kernel::compile(state).unwrap();
                kernel
                    .set_partnership_reconsideration(1, reconsider)
                    .unwrap();
                kernel.step().unwrap();
                let result = kernel.export();
                let family = result.families.as_ref().unwrap();
                let expected = if reconsider && !controlled {
                    None
                } else {
                    Some(2)
                };
                assert_eq!(
                    family
                        .members
                        .iter()
                        .find(|row| row.id == 1)
                        .unwrap()
                        .partner,
                    expected
                );
                assert_eq!(
                    family
                        .members
                        .iter()
                        .find(|row| row.id == 2)
                        .unwrap()
                        .partner,
                    expected.map(|_| 1)
                );
                let restored = World::from_json(&serde_json::to_string(&result).unwrap()).unwrap();
                assert_eq!(Kernel::compile(restored).unwrap().export(), result);
                result
            })
    };
    for reconsider in [false, true] {
        for controlled in [false, true] {
            assert_eq!(
                run(1, reconsider, controlled),
                run(4, reconsider, controlled)
            );
        }
    }
}
