use std::fmt::Write as _;
use std::path::PathBuf;

/// Includes candidate preparation, personal selection, revalidation and
/// physical arbitration against one shared stock and one shared local store.
#[test]
#[ignore = "mixed-workload performance measurement"]
fn concentrated_personal_choice_benchmark() {
    let food_pickups = std::env::var_os("LEGACY_CHOICE_FOOD_PICKUPS").is_some();
    let count: u64 = std::env::var("LEGACY_CHOICE_PEOPLE")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(5_000);
    assert!((1..=100_000).contains(&count));
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut text = String::from(
        r#"seed = 7
tick_millis = 900000
stock_capacity = 100000
food_relief_divisor = 20
[rules]
hunger_per_tick = 1
meal_threshold = 5
starvation_threshold = 50
starvation_damage = 1
max_health = 100
carrying_capacity = 10
max_work_hunger = 20
[[regions]]
id = 1
stock = { grain = 50000 }
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[people]]\nid = {id}\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false").unwrap();
    }
    text.push_str(
        r#"[work_sites]
sites = []
[construction]
[[construction.blueprints]]
id = 1
name = "Shelter"
kind = "shelter"
capacity = 2
kit = "grain"
quantity = 1
salvage = 1
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[construction.policies]]\nperson = {id}\nblueprint = 1\ntarget_sites = {count}\nwilling = true").unwrap();
    }
    text.push_str(
        r#"[local_transport]
[[local_transport.stores]]
id = 1
region = 1
name = "Granary"
capacity = 10000
"#,
    );
    for id in 1..=count {
        writeln!(text, "[[local_transport.policies]]\nperson = {id}\nregion = 1\nsource = {{ kind = \"stock\" }}\ndestination = {{ kind = \"store\", id = 1 }}\ngood = \"grain\"\nquantity = 1\nwilling = true").unwrap();
    }
    for id in 1..=count {
        let (build, carry) = if id % 2 == 0 { (0, 10) } else { (10, 0) };
        writeln!(text, "[[personal_choices]]\nid = {id}\nidle = 0\npreferences = [{{ domain = 1, enabled = true, bias = {build} }}, {{ domain = 2, enabled = true, bias = {carry} }}]").unwrap();
    }
    if food_pickups {
        text.push_str("[knowledge]\nmax_records = 4\nminds = []\n[[regions]]\nid = 2\nstock = {}\n[[routes]]\nsource = 1\ndestination = 2\nticks = 1\n[[routes]]\nsource = 2\ndestination = 1\nticks = 1\n[food_search]\nmin_hunger = 10\nmin_report_food = 1\nmax_report_age = 10\nretry_ticks = 1\npeople = []\n");
    }
    text = text
        .replace(
            "stock_capacity = 100000",
            &format!("stock_capacity = {}", count * 20),
        )
        .replace("grain = 50000", &format!("grain = {}", count * 10))
        .replace(
            "\ncapacity = 10000\n",
            &format!("\ncapacity = {}\n", count * 2),
        );
    let token = format!(
        "legacy-choice-benchmark-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let path = std::env::temp_dir().join(format!("{token}.toml"));
    std::fs::write(&path, text).unwrap();
    let mut snapshot = game_economy_cli::core_scenario(&path, &root.join("content"))
        .unwrap()
        .snapshot();
    if std::env::var_os("LEGACY_CHOICE_HUNGRY").is_some() {
        for person in &mut snapshot.core.people {
            person.hunger = 25;
        }
    }
    std::fs::remove_file(path).unwrap();
    let maintenance = std::env::var_os("LEGACY_CHOICE_MAINTENANCE").is_some();
    let work = std::env::var_os("LEGACY_CHOICE_WORK").is_some();
    let meals = std::env::var_os("LEGACY_CHOICE_MEALS").is_some();
    let tool_repairs = std::env::var_os("LEGACY_CHOICE_TOOL_REPAIRS").is_some();
    let treatments = std::env::var_os("LEGACY_CHOICE_SELF_TREATMENTS").is_some();
    assert!(
        usize::from(maintenance)
            + usize::from(work)
            + usize::from(meals)
            + usize::from(tool_repairs)
            + usize::from(food_pickups)
            + usize::from(treatments)
            <= 1,
        "choose one additional workload"
    );
    if treatments {
        use game_lots::{Container, Lot, Place};
        let medicine = snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.name == "medicine")
            .unwrap()
            .id;
        for container in &mut snapshot.core.physical.custody.containers {
            if let Place::Person(id) = container.place
                && id % 3 == 0
            {
                *container = Container::new(
                    Place::Person(id),
                    10,
                    vec![Lot {
                        item: None,
                        good: medicine,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
            }
        }
        snapshot.health = Some(game_economy::core_interval::CoreHealth {
            owner: game_disease::Disease {
                strains: vec![game_disease::Strain {
                    id: 1,
                    name: "fever".into(),
                    incubation: 0,
                    infectious_ticks: 5,
                    immunity_ticks: 1,
                    damage_per_tick: 1,
                    transmission_per_million: 0,
                    combat_transmission_per_million: 0,
                    birth_transmission_per_million: 0,
                }],
                treatments: vec![game_disease::Treatment {
                    strain: 1,
                    medicine,
                }],
                cases: vec![],
                events: vec![],
            },
            treatment: (1..=count)
                .filter(|id| id % 3 == 0)
                .map(|person| game_economy_treatment::Policy {
                    person,
                    medicine,
                    quantity: 1,
                    willing: true,
                    health_at_most: None,
                })
                .collect(),
            last_tick: None,
        });
        let profiles = std::sync::Arc::make_mut(
            snapshot
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        );
        for person in profiles.iter_mut().filter(|person| person.id % 3 == 0) {
            person.preferences.push(game_personal_choice::Preference {
                domain: 10,
                enabled: true,
                bias: 30,
                weight: 1000,
            });
        }
    }
    if food_pickups {
        for person in snapshot
            .core
            .people
            .iter_mut()
            .filter(|person| person.id % 3 == 0)
        {
            person.seek_meals = true;
        }
        let profiles = std::sync::Arc::make_mut(
            snapshot
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        );
        for person in profiles.iter_mut().filter(|person| person.id % 3 == 0) {
            person.preferences.push(game_personal_choice::Preference {
                domain: 9,
                enabled: true,
                bias: 30,
                weight: 1000,
            });
        }
    }
    if tool_repairs {
        use game_economy_work::tool_wear::{Condition, RepairChoice, Rule, ToolWear};
        use game_lots::{Container, Lot, Place};
        let good = |name: &str| {
            snapshot
                .core
                .physical
                .goods
                .iter()
                .find(|good| good.name == name)
                .unwrap()
                .id
        };
        let grain = good("grain");
        let tool = good("tool");
        for container in &mut snapshot.core.physical.custody.containers {
            if let Place::Person(id) = container.place
                && id % 3 == 0
            {
                *container = Container::new(
                    Place::Person(id),
                    10,
                    vec![
                        Lot {
                            item: None,
                            good: grain,
                            quantity: 1,
                            expires_at: None,
                        },
                        Lot {
                            item: Some(1_000_000 + id),
                            good: tool,
                            quantity: 1,
                            expires_at: None,
                        },
                    ],
                )
                .unwrap();
            }
        }
        snapshot.tool_wear = Some(ToolWear {
            next_item: 1_000_001 + count,
            rules: vec![Rule {
                good: tool,
                durability: 5,
                wear: 1,
                repair_good: grain,
                repair_quantity: 1,
            }],
            conditions: (1..=count)
                .filter(|id| id % 3 == 0)
                .map(|id| Condition {
                    item: 1_000_000 + id,
                    good: tool,
                    damage: 5,
                })
                .collect(),
            repairs: (1..=count)
                .filter(|id| id % 3 == 0)
                .map(|person| RepairChoice {
                    person,
                    item: 1_000_000 + person,
                    willing: true,
                })
                .collect(),
            events: vec![],
        });
        let profiles = std::sync::Arc::make_mut(
            snapshot
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        );
        for person in profiles.iter_mut().filter(|person| person.id % 3 == 0) {
            person.preferences.push(game_personal_choice::Preference {
                domain: 8,
                enabled: true,
                bias: 30,
                weight: 1000,
            });
        }
    }
    if meals {
        use game_lots::{Container, Lot, Place};
        let good = snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.name == "grain")
            .unwrap()
            .id;
        for person in snapshot
            .core
            .people
            .iter_mut()
            .filter(|person| person.id % 3 == 0)
        {
            person.seek_meals = true;
        }
        for container in &mut snapshot.core.physical.custody.containers {
            if let Place::Person(id) = container.place
                && id % 3 == 0
            {
                *container = Container::new(
                    Place::Person(id),
                    10,
                    vec![Lot {
                        item: None,
                        good,
                        quantity: 10,
                        expires_at: None,
                    }],
                )
                .unwrap();
            }
        }
        let profiles = std::sync::Arc::make_mut(
            snapshot
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        );
        for person in profiles.iter_mut().filter(|person| person.id % 3 == 0) {
            person.preferences.push(game_personal_choice::Preference {
                domain: 7,
                enabled: true,
                bias: 30,
                weight: 1000,
            });
        }
    }
    if maintenance {
        use game_economy_building::maintenance::{Condition, Maintenance, Policy, Rule};
        use game_economy_work::sites::Site;
        use game_lots::{Container, Lot, Place};
        let good = snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.name == "grain")
            .unwrap()
            .id;
        for container in &mut snapshot.core.physical.custody.containers {
            if let Place::Person(id) = container.place
                && id % 3 == 0
            {
                *container = Container::new(
                    Place::Person(id),
                    10,
                    vec![Lot {
                        item: None,
                        good,
                        quantity: 1,
                        expires_at: None,
                    }],
                )
                .unwrap();
            }
        }
        let sites = snapshot.sites.as_mut().unwrap();
        sites.facilities.next_id = 2;
        sites.facilities.sites.push(Site {
            id: 1,
            region: 1,
            kind: "mill".into(),
            capacity: 1,
        });
        sites.maintenance = Some(Maintenance {
            rules: vec![Rule {
                kind: "mill".into(),
                wear_limit: 10,
                wear_per_slot: 1,
                repair_good: good,
                repair_quantity: 1,
            }],
            conditions: vec![Condition { site: 1, wear: 5 }],
            policies: (1..=count)
                .filter(|id| id % 3 == 0)
                .map(|person| Policy {
                    person,
                    site: 1,
                    willing: true,
                    pressured_by: None,
                    pressure_threshold: None,
                    disregard_claims: false,
                })
                .collect(),
            events: vec![],
        });
        let profiles = std::sync::Arc::make_mut(
            snapshot
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        );
        for person in profiles.iter_mut().filter(|person| person.id % 3 == 0) {
            person.preferences.push(game_personal_choice::Preference {
                domain: 3,
                enabled: true,
                bias: 30,
                weight: 1000,
            });
        }
    }
    if work {
        let grain = snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.name == "grain")
            .unwrap()
            .id;
        let output = snapshot
            .core
            .physical
            .goods
            .iter()
            .find(|good| good.id != grain)
            .unwrap()
            .id;
        let recipe = snapshot
            .core
            .physical
            .recipes
            .iter()
            .map(|recipe| recipe.id)
            .max()
            .unwrap_or(0)
            + 1;
        snapshot
            .core
            .physical
            .recipes
            .push(game_production::Recipe {
                id: recipe,
                name: "choice benchmark work".into(),
                ticks: 2,
                inputs: vec![game_production::Amount {
                    good: grain,
                    quantity: 1,
                }],
                outputs: vec![game_production::Amount {
                    good: output,
                    quantity: 1,
                }],
            });
        let output_place = game_lots::Place::Stock {
            region: 1,
            good: output,
        };
        if !snapshot
            .core
            .physical
            .custody
            .containers
            .iter()
            .any(|container| container.place == output_place)
        {
            snapshot
                .core
                .physical
                .custody
                .containers
                .push(game_lots::Container::new(output_place, count * 20, vec![]).unwrap());
            snapshot
                .core
                .physical
                .custody
                .containers
                .sort_by_key(|container| container.place);
        }
        for person in snapshot
            .core
            .people
            .iter_mut()
            .filter(|person| person.id % 3 == 0)
        {
            person.willing_to_work = true;
        }
        snapshot.autowork = Some(game_autowork::Autowork {
            next_id: 1,
            policies: (1..=count)
                .filter(|id| id % 3 == 0)
                .map(|person| game_autowork::Policy {
                    person,
                    recipe,
                    enabled: true,
                    disregard_claims: false,
                })
                .collect(),
            requests: None,
        });
        let profiles = std::sync::Arc::make_mut(
            snapshot
                .schedule
                .as_mut()
                .unwrap()
                .personal_choices
                .as_mut()
                .unwrap(),
        );
        for person in profiles.iter_mut().filter(|person| person.id % 3 == 0) {
            person.preferences.push(game_personal_choice::Preference {
                domain: 4,
                enabled: true,
                bias: 30,
                weight: 1000,
            });
        }
    }
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        #[cfg(feature = "profile")]
        let before = game_diagnostics::snapshot();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut host = pool.install(|| {
                game_economy::core_interval::CoreHost::resume(snapshot.clone()).unwrap()
            });
            let start = std::time::Instant::now();
            let result = pool.install(|| host.advance_continuous(1, &[]).unwrap());
            times.push(start.elapsed());
            assert_eq!(result.failure, None);
            if maintenance && count >= 3 {
                let decisions = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::PersonalChoices(rows) => {
                            Some(rows)
                        }
                        _ => None,
                    })
                    .flatten()
                    .filter(|event| event.key.is_some_and(|key| key.domain == 3))
                    .count();
                assert_eq!(decisions as u64, count / 3);
            }
            if work && count >= 3 {
                let decisions = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::PersonalChoices(rows) => {
                            Some(rows)
                        }
                        _ => None,
                    })
                    .flatten()
                    .filter(|event| event.key.is_some_and(|key| key.domain == 4))
                    .count();
                assert_eq!(decisions as u64, count / 3);
            }
            if meals && count >= 3 {
                let decisions = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::Meals(rows) => Some(rows.len()),
                        _ => None,
                    })
                    .sum::<usize>();
                assert_eq!(decisions as u64, count / 3);
            }
            if tool_repairs && count >= 3 {
                let decisions = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::ToolRepairs(rows) => {
                            Some(rows.iter().filter(|row| row.accepted).count())
                        }
                        _ => None,
                    })
                    .sum::<usize>();
                assert_eq!(decisions as u64, count / 3);
            }
            if food_pickups && count >= 3 {
                let decisions = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::Provisions(rows) => {
                            Some(rows.iter().filter(|row| row.accepted).count())
                        }
                        _ => None,
                    })
                    .sum::<usize>();
                assert_eq!(decisions as u64, count / 3);
            }
            if treatments && count >= 3 {
                let decisions = result
                    .events
                    .iter()
                    .filter_map(|event| match event {
                        game_economy::core_interval::CoreRunEvent::Treatments(rows) => {
                            Some(rows.iter().filter(|row| row.accepted).count())
                        }
                        _ => None,
                    })
                    .sum::<usize>();
                assert_eq!(decisions as u64, count / 3);
            }
            let state = pool.install(|| host.snapshot());
            if let Some(expected) = &reference {
                assert_eq!(&state, expected);
            } else {
                reference = Some(state);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} people",
            times[1]
        );
        #[cfg(feature = "profile")]
        for (name, sample) in game_diagnostics::snapshot()
            .iter()
            .filter(|(name, _)| name.starts_with("choice."))
        {
            let earlier = before.get(name);
            let calls = sample.calls - earlier.map_or(0, |row| row.calls);
            let nanos = sample.nanoseconds - earlier.map_or(0, |row| row.nanoseconds);
            if calls > 0 {
                eprintln!(
                    "  {name}: {:.3} ms/call",
                    nanos as f64 / calls as f64 / 1_000_000.0
                );
            }
        }
    }
}
