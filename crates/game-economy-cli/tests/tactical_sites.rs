use game_economy::core_interval::{CoreHost, CoreRunEvent, TacticalChoice};
use std::path::Path;

#[test]
fn a_target_gets_active_protection_only_when_they_choose_to_defend() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let template = std::fs::read_to_string(
        root.join("content/scenarios/economy-combat-collapse.toml"),
    ).unwrap().replace("\r\n", "\n");
    let run = |workers, policy: &str, chunks: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let input = template
                    .replace("tick_millis = 3600000", "tick_millis = 1000\ncombat_round_millis = 101")
                    .replace("[combat]\ndamage = 1", &format!("[combat]\ndamage = 6\n{policy}"));
                let path = std::env::temp_dir().join(format!("legacy-personal-guard-{}.toml", std::process::id()));
                std::fs::write(&path, input).unwrap();
                let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
                std::fs::remove_file(path).unwrap();
                let mut host = loaded.unwrap();
                let before = host.snapshot().core.people[1].health;
                host.start_controlled_engagement(1, 2).unwrap();
                for &advance in chunks {
                    assert_eq!(host.advance_continuous(advance, &[]).unwrap().failure, None);
                    let save = serde_json::to_vec(&host.snapshot()).unwrap();
                    host = CoreHost::resume(serde_json::from_slice(&save).unwrap()).unwrap();
                }
                before - host.snapshot().core.people[1].health
            })
    };
    assert_eq!(run(1, "", &[101]), 6);
    assert_eq!(run(6, "reactions = [{ person = 2, response = \"wait\" }]", &[29, 72]), 6);
    assert_eq!(run(6, "reactions = [{ person = 2, response = \"defend\" }]", &[19, 82]), 3);
}

#[test]
fn tactical_damage_collapses_a_real_workshop_and_interrupts_its_work() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let template =
        std::fs::read_to_string(root.join("content/scenarios/economy-combat-collapse.toml"))
            .unwrap();
    let source = template
        .replace("tick_millis = 3600000", "tick_millis = 1000\ncombat_round_millis = 101\nelapsed_spoilage = true")
        .replace("[rules]", "[storage_climate]\nambient_celsius = 20\naging_rates = [{ from_celsius = -50, per_thousand = 500 }, { from_celsius = 12, per_thousand = 1000 }]\nplaces = [{ place = { kind = \"inventory\", id = 1 }, celsius = 4 }]\n\n[rules]")
        .replace("goods = { timber = 5 }", "goods = { timber = 5, bread = 2 }\nstructure = { site = 1, celsius = 4, intact = true }")
        .replace("reed-shelter-kit = 1, bread = 20", "reed-shelter-kit = 1, bread = 20, timber = 10")
        .replace("wear_limit = 1", "wear_limit = 2")
        .replace("position = { x = 0, y = 1 }", "position = { x = 5, y = 4 }")
        .replace("[family]", "[[people]]\nid = 3\nregion = 1\nhealth = 100\nhunger = 0\nwilling_to_work = false\nseek_meals = false\ngoods = { timber = 10 }\n\n[family]")
        .replace("next_id = 3", "next_id = 4")
        .replace("[campaign.lineage]", "[[family.members]]\nid = 3\nage = 18\ngenome = { sex = \"Male\", alleles = [[1000,1000], [200,800], [300,700], [400,600]] }\n\n[campaign.lineage]");
    let path = std::env::temp_dir().join(format!(
        "legacy-elapsed-tactical-sites-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, source).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let mut opening = loaded.unwrap().snapshot();
    opening.core.people[2].controlled = true;
    let store = game_lots::Place::Inventory(1);
    let chilled_bread = |world: &game_economy::core_interval::CoreHostSnapshot| {
        world.core.physical.custody.containers.iter()
            .find(|holder| holder.place == store).unwrap().lots.iter()
            .find(|lot| lot.expires_at.is_some()).unwrap().clone()
    };
    let cold_expiry = chilled_bread(&opening).expires_at.unwrap();
    let recipe = opening
        .core
        .physical
        .recipes
        .iter()
        .find(|row| row.name == "fixture-tool")
        .unwrap()
        .id;
    let run = |workers, steps: &[(u64, bool)]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = CoreHost::resume(opening.clone()).unwrap();
                let (job, started) = host.start_controlled_work(3, recipe, 1).unwrap();
                assert!(started);
                host.start_controlled_engagement(1, 2).unwrap();
                assert_eq!(
                    host.player_view(1).unwrap().tactics.unwrap().visible_sites[0].0,
                    1
                );
                host.choose_controlled_tactics(1, TacticalChoice::AttackSite { site: 1 })
                    .unwrap();
                let mut strikes = Vec::new();
                for &(time, check_damage) in steps {
                    let result = host.advance_continuous(time, &[]).unwrap();
                    assert_eq!(result.failure, None);
                    for event in result.events {
                        if let CoreRunEvent::Tactical(result) = event {
                            for hit in result.sites {
                                strikes.push((hit.at_millis, hit.actor, hit.harm, hit.collapsed));
                                if hit.collapsed {
                                    assert_eq!(result.interrupted_jobs, [job]);
                                } else {
                                    assert!(result.interrupted_jobs.is_empty());
                                }
                            }
                        }
                    }
                    let saved = host.snapshot();
                    if check_damage {
                        let sites = saved.sites.as_ref().unwrap();
                        assert_eq!(sites.maintenance.as_ref().unwrap().conditions[0].wear, 1);
                        assert_eq!(sites.facilities.bindings[0].job, job);
                        assert_eq!(chilled_bread(&saved).expires_at, Some(cold_expiry));
                        assert!(saved.local_transport.as_ref().unwrap().stores[0].structure.unwrap().intact);
                    }
                    host = CoreHost::resume(
                        serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap(),
                    )
                    .unwrap();
                }
                assert_eq!(strikes, [(101, 1, 1, false), (202, 1, 1, true)]);
                let end = host.snapshot();
                assert_eq!(chilled_bread(&end).quantity, 2);
                assert_eq!(chilled_bread(&end).expires_at, Some(202 + (cold_expiry - 202).div_ceil(2)));
                assert!(!end.local_transport.as_ref().unwrap().stores[0].structure.unwrap().intact);
                assert!(end.sites.as_ref().unwrap().facilities.sites.is_empty());
                assert!(
                    end.sites
                        .as_ref()
                        .unwrap()
                        .maintenance
                        .as_ref()
                        .unwrap()
                        .conditions
                        .is_empty()
                );
                assert!(
                    end.personal_conflict
                        .as_ref()
                        .unwrap()
                        .tactical
                        .as_ref()
                        .unwrap()
                        .maps[0]
                        .obstacles
                        .is_empty()
                );
                assert_eq!(
                    end.core.physical.work[0].phase,
                    game_lot_runtime::Phase::Returning
                );
                assert!(
                    host.choose_controlled_tactics(1, TacticalChoice::AttackSite { site: 1 })
                        .is_err()
                );
                (strikes, end)
            })
    };
    assert_eq!(
        run(1, &[(101, true), (101, false)]),
        run(4, &[(12, false), (89, true), (101, false)])
    );
}
