use game_economy::core_interval::{
    CoreHost, CoreRunEvent, ExpeditionEvent, ExpeditionEventKind, ExpeditionStage,
};
use game_economy_player::interval::Player;
use std::path::Path;

fn scene(quantum: u64, case: &str, closed: bool) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut text =
        std::fs::read_to_string(root.join("content/scenarios/economy-expedition-rendezvous.toml"))
            .unwrap()
            .replace("tick_millis = 3001", &format!("tick_millis = {quantum}"));
    if closed {
        text.push_str("\n[[route_closures]]\nsource = 1\ndestination = 2\nstart = 15\nend = 25\nunit_millis = 100\n");
    }
    let path = std::env::temp_dir().join(format!(
        "legacy-rendezvous-{}-{case}-{quantum}.toml",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    loaded.unwrap()
}

fn advance(host: &mut CoreHost, millis: u64) -> Vec<ExpeditionEvent> {
    let run = host.advance_continuous(millis, &[]).unwrap();
    assert_eq!(run.failure, None);
    run.events
        .into_iter()
        .filter_map(|event| match event {
            CoreRunEvent::Expeditions(events) => Some(events),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn independent_wait_and_timeout_decisions_are_stable_across_workers_and_elapsed_partitions() {
    let run = |workers, quantum, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = scene(quantum, "determinism", false);
                let mut events = Vec::new();
                for &chunk in chunks {
                    events.extend(advance(&mut host, chunk));
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(host.expedition(4).unwrap().stage, ExpeditionStage::Ready);
                assert_eq!(host.expedition(4).unwrap().visit_millis, 10000);
                events
            })
    };
    let events = run(1, 701, &[4301]);
    assert_eq!(events, run(6, 3001, &[19, 944, 1089, 2249]));
    let outbound: Vec<_> = events
        .iter()
        .filter(|row| row.kind == ExpeditionEventKind::OutboundLeg)
        .map(|row| (row.person, row.at_millis))
        .collect();
    assert_eq!(outbound, [(1, 2000), (2, 2000), (3, 3000)]);
    let together: Vec<_> = events
        .iter()
        .filter(|row| {
            matches!(
                row.kind,
                ExpeditionEventKind::CompanionsDeparted {
                    group: 7,
                    companions: 1
                }
            )
        })
        .map(|row| (row.person, row.at_millis))
        .collect();
    assert_eq!(together, [(1, 2000), (2, 2000)]);
}

#[test]
fn a_player_can_join_or_change_their_own_rendezvous() {
    let mut host = scene(3001, "controlled", false);
    let mut player = Player::new(&mut host, 4).unwrap();
    player
        .set_expedition_rendezvous(Some((7, 1, 5000)))
        .unwrap();
    player.set_expedition_rendezvous(None).unwrap();
    assert!(
        player
            .view()
            .unwrap()
            .expedition
            .unwrap()
            .rendezvous
            .is_none()
    );
    player
        .set_expedition_rendezvous(Some((7, 2, 4500)))
        .unwrap();
    let events = advance(&mut host, 2001);
    let group: Vec<_> = events
        .iter()
        .filter(|row| {
            matches!(
                row.kind,
                ExpeditionEventKind::CompanionsDeparted {
                    group: 7,
                    companions: 2
                }
            )
        })
        .map(|row| (row.person, row.at_millis))
        .collect();
    assert_eq!(group, [(1, 2000), (2, 2000), (4, 2000)]);
    assert_eq!(host.expedition(3).unwrap().stage, ExpeditionStage::Ready);
    assert_eq!(host.expedition(4).unwrap().stage, ExpeditionStage::Outbound);
}

#[test]
fn the_archive_counts_joint_departure_only_after_actual_physical_admission() {
    let mut host = scene(3001, "closed", true);
    let events = advance(&mut host, 2001);
    assert_eq!(
        events
            .iter()
            .filter(|row| row.kind == ExpeditionEventKind::RouteUnavailable)
            .count(),
        2
    );
    assert!(
        !events
            .iter()
            .any(|row| matches!(row.kind, ExpeditionEventKind::CompanionsDeparted { .. }))
    );
    assert_eq!(host.expedition(1).unwrap().stage, ExpeditionStage::Ready);
    assert_eq!(host.expedition(2).unwrap().stage, ExpeditionStage::Ready);
    let events = advance(&mut host, 1000);
    assert_eq!(
        events
            .iter()
            .filter(|row| matches!(
                row.kind,
                ExpeditionEventKind::CompanionsDeparted {
                    group: 7,
                    companions: 2
                }
            ))
            .count(),
        3
    );
}
