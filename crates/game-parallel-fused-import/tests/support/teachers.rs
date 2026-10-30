use super::*;
use game_parallel::World;

#[test]
fn concentrated_births_inherit_roles_deterministically_without_copying_skills() {
    let world =
        World::from_toml(include_str!("../../../../content/parallel/campaign.toml")).unwrap();
    let skills = world.skills.unwrap();
    let births: Vec<_> = (100..10_100).map(|person| (person, [2, 1], 2)).collect();
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut next = skills.clone();
                enroll(&mut next, &births, 10, 12);
                next
            })
    };
    let result = run(1);
    assert_eq!(result, run(4));
    assert_eq!(result.teachers.len(), 20_002);
    assert!(
        result
            .teachers
            .iter()
            .filter(|policy| policy.person >= 100)
            .all(|policy| policy.eligible_at == 22)
    );
    assert!(result.records.is_empty());
    assert!(result.lessons.is_empty());
}
