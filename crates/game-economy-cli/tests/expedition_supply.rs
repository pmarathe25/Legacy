use game_economy::core_interval::{CoreHost, CoreRunEvent, ExpeditionEventKind};
use game_lots::Place;

#[test]
fn a_courier_may_fetch_real_supplies_or_leave_without_them() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let template =
        std::fs::read_to_string(root.join("content/scenarios/economy-core-player.toml")).unwrap();
    let mut source = template
        .split_once("[hauling]")
        .unwrap()
        .0
        .replace("seed = 7", "seed = 7\ndecision_period_millis = 100")
        .replace("controlled = true", "controlled = false")
        .replace("grain = 10, timber = 5", "grain = 10");
    source.push_str(
        r#"
[[people]]
id = 2
region = 2
health = 100
hunger = 0
willing_to_work = false
seek_meals = false

[[local_transport.stores]]
id = 1
region = 1
name = "Timber store"
capacity = 10
goods = { timber = 4 }

[local_transport.walking.elapsed]
unit_millis = 100

[[local_transport.walking.routes]]
region = 1
from = { kind = "stock" }
to = { kind = "store", id = 1 }
ticks = 2

[[local_transport.walking.routes]]
region = 1
from = { kind = "store", id = 1 }
to = { kind = "stock" }
ticks = 2
"#,
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-expedition-supply-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, &source).unwrap();
    let imported = game_economy_cli::core_scenario(&path, &root.join("content")).unwrap();
    let timber = imported
        .snapshot()
        .core
        .physical
        .goods
        .into_iter()
        .find(|good| good.name == "timber")
        .unwrap()
        .id;
    source.push_str(&format!(r#"
[missions]
next_id = 2

[[missions.entries]]
id = 1
person = 1
origin = 1
destination = 2
visit_ticks = 1
phase = "ready"
objective = {{ kind = "deliver", person = 2, good = {timber}, quantity = 3 }}
delivery_preference = {{ willing = true, reserve = 1, acquisition = {{ willing = true, known_store = 1 }} }}
"#));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let opening = loaded.unwrap().snapshot();
    let run = |workers, steps: &[u64], willing| {
        rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut saved = opening.clone();
        saved.expeditions.as_mut().unwrap().itineraries[0].delivery_preference.as_mut().unwrap().acquisition.as_mut().unwrap().willing = willing;
        let mut host = CoreHost::resume(saved).unwrap();
        let mut events = Vec::new();
        for &step in steps {
            let result = host.advance_continuous(step, &[]).unwrap();
            assert_eq!(result.failure, None);
            events.extend(result.events.into_iter().filter_map(|event| match event {
                CoreRunEvent::Expeditions(events) => Some(events),
                _ => None,
            }).flatten());
            host = CoreHost::resume(serde_json::from_str(&serde_json::to_string(&host.snapshot()).unwrap()).unwrap()).unwrap();
        }
        let saved = host.snapshot();
        let quantity = |place| saved.core.physical.custody.containers.iter()
            .find(|row| row.place == place).unwrap().lots.iter()
            .filter(|lot| lot.good == timber).map(|lot| lot.quantity).sum::<u64>();
        let fetched = events.iter().find(|event| matches!(event.kind,
            ExpeditionEventKind::SuppliesAcquired { good, quantity: 4, from: Place::Inventory(1) } if good == timber));
        if willing {
            let fetched = fetched.expect("courier acquired stock from the store");
            assert_eq!(fetched.at_millis, 200);
            assert_eq!(quantity(Place::Inventory(1)), 0);
            assert_eq!(quantity(Place::Person(1)), 1);
            assert_eq!(quantity(Place::Person(2)), 3);
            assert!(events.iter().any(|event| event.kind == ExpeditionEventKind::ObjectiveMet));
        } else {
            assert!(fetched.is_none());
            assert_eq!(quantity(Place::Inventory(1)), 4);
            assert_eq!(quantity(Place::Person(2)), 0);
            assert!(events.iter().any(|event| event.kind == ExpeditionEventKind::ReachedDestination));
        }
        (events, saved)
    })
    };
    assert_eq!(run(1, &[1750], true), run(4, &[181, 319, 900, 350], true));
    run(2, &[1750], false);
}
