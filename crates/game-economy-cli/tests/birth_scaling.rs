use game_economy::core_interval::CoreHost;
use game_lots::{Container, Place};
use std::path::PathBuf;

#[test]
#[ignore = "composed newborn enrollment scaling"]
fn newborn_choices_with_physical_births_benchmark() {
    let count: u64 = std::env::var("LEGACY_BIRTH_COUNT")
        .ok()
        .map(|value| value.parse().expect("invalid birth count"))
        .unwrap_or(5_000);
    assert!((1..=100_000).contains(&count));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut snapshot = game_economy_cli::core_scenario(
        &root.join("content/scenarios/economy-core-birth-choices.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    let male = snapshot.core.people[0].clone();
    let female = snapshot.core.people[1].clone();
    snapshot.core.people = (0..count)
        .flat_map(|index| {
            let mut a = male.clone();
            let mut b = female.clone();
            a.id = index * 2 + 1;
            b.id = index * 2 + 2;
            [a, b]
        })
        .collect();
    let family = snapshot.family.as_mut().unwrap();
    let father = family.members[0].clone();
    let mother = family.members[1].clone();
    family.members = (0..count)
        .flat_map(|index| {
            let mut a = father.clone();
            let mut b = mother.clone();
            a.id = index * 2 + 1;
            b.id = index * 2 + 2;
            b.pregnancy.as_mut().unwrap().father = a.id;
            [a, b]
        })
        .collect();
    family.next_id = count * 2 + 1;
    snapshot
        .core
        .physical
        .custody
        .containers
        .retain(|row| !matches!(row.place, Place::Person(_)));
    snapshot
        .core
        .physical
        .custody
        .containers
        .extend((1..=count * 2).map(|id| Container::new(Place::Person(id), 100, vec![]).unwrap()));
    snapshot
        .core
        .physical
        .custody
        .containers
        .sort_by_key(|row| row.place);
    let baseline = std::env::var_os("LEGACY_BIRTH_BASELINE").is_some();
    if baseline {
        snapshot.schedule.as_mut().unwrap().birth_choices = None;
    }
    let mut initial = CoreHost::resume(snapshot).unwrap();
    assert_eq!(
        initial.advance_continuous(3_599_999, &[]).unwrap().failure,
        None
    );
    let opening = initial.snapshot();
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut host = pool.install(|| CoreHost::resume(opening.clone()).unwrap());
            let start = std::time::Instant::now();
            let result = pool.install(|| host.advance_continuous(1, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            assert_eq!(result.boundaries, 1);
            let state = pool.install(|| host.snapshot());
            assert_eq!(state.core.people.len() as u64, count * 3);
            let choices = state
                .schedule
                .as_ref()
                .unwrap()
                .personal_choices
                .as_ref()
                .unwrap();
            assert_eq!(choices.len() as u64, if baseline { 0 } else { count });
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} physical births ({})",
            times[1],
            if baseline {
                "baseline"
            } else {
                "personal choices"
            }
        );
    }
}
