use game_economy::core_interval::{CoreHost, CoreRunEvent};
use game_economy_player::interval::Player;
use game_lots::Place;
use std::path::Path;

fn soil_world(quantum: u64, controlled: bool) -> CoreHost {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text =
        std::fs::read_to_string(root.join("content/scenarios/economy-soil-improvement.toml"))
            .unwrap()
            .replace(
                "tick_millis = 900000",
                &format!("tick_millis = {quantum}\ndecision_period_millis = 250"),
            )
            .replace("recovery_per_fallow = 10", "recovery_per_fallow = 0")
            .replace("willing_to_work = true", "willing_to_work = false")
            .replace(
                "[soil_amendment]",
                "[soil_amendment]\nelapsed_actions = true",
            );
    let path = std::env::temp_dir().join(format!(
        "legacy-soil-amendment-{}-{quantum}-{controlled}.toml",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    let loaded = game_economy_cli::core_scenario(&path, &root.join("content"));
    std::fs::remove_file(path).unwrap();
    let mut snapshot = loaded.unwrap().snapshot();
    let farm = snapshot.agriculture.as_mut().unwrap();
    farm.requests = None;
    let mut other_field = farm.fields[0].clone();
    other_field.id = 2;
    farm.fields.push(other_field);
    let soil = farm.soil.as_mut().unwrap();
    soil.fields.push(game_economy_agriculture::soil::FieldSoil {
        field: 2,
        fertility: 50,
    });
    let treatment = soil.amendment.as_mut().unwrap();
    let mut other_choice = treatment.choices[0].clone();
    other_choice.person = 2;
    other_choice.field = 2;
    other_choice.willing = false;
    treatment.choices.push(other_choice);
    snapshot.core.people[1].controlled = controlled;
    CoreHost::resume(snapshot).unwrap()
}

fn substrate_units(host: &CoreHost) -> u64 {
    let state = host.snapshot();
    let good = state
        .agriculture
        .as_ref()
        .unwrap()
        .soil
        .as_ref()
        .unwrap()
        .amendment
        .as_ref()
        .unwrap()
        .good;
    state
        .core
        .physical
        .custody
        .containers
        .iter()
        .find(|row| row.place == Place::Stock { region: 1, good })
        .unwrap()
        .used
}

#[test]
fn choice_and_material_custody_control_amendment_at_the_decision_instant() {
    let run = |workers, quantum, steps: &[u64]| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut host = soil_world(quantum, false);
                let mut accepted = Vec::new();
                for &duration in steps {
                    let run = host.advance_continuous(duration, &[]).unwrap();
                    assert_eq!(run.failure, None);
                    for event in run.events {
                        if let CoreRunEvent::SoilAmendment { at_millis, events } = event {
                            for event in events {
                                assert!(event.accepted);
                                accepted.push((at_millis, event.person, event.field));
                            }
                        }
                    }
                    host = CoreHost::resume(
                        serde_json::from_slice(&serde_json::to_vec(&host.snapshot()).unwrap())
                            .unwrap(),
                    )
                    .unwrap();
                }
                let soil = host.snapshot().agriculture.unwrap().soil.unwrap();
                let fertility = soil
                    .fields
                    .into_iter()
                    .map(|field| field.fertility)
                    .collect::<Vec<_>>();
                (accepted, fertility, substrate_units(&host))
            })
    };
    let result = run(1, 900, &[550]);
    assert_eq!(result, run(6, 173, &[73, 100, 7, 71, 249, 50]));
    assert_eq!(result, (vec![(0, 1, 1), (250, 1, 1)], vec![80, 50], 0));
}

#[test]
fn the_player_can_choose_to_treat_a_field_without_starting_other_people() {
    let mut host = soil_world(910, true);
    {
        let mut player = Player::new(&mut host, 2).unwrap();
        assert!(player.amend(2).unwrap());
        let view = player.view().unwrap();
        assert_eq!(view.accessible_fields[0].fertility, Some(50));
        assert_eq!(view.accessible_fields[1].fertility, Some(65));
    }
    assert_eq!(substrate_units(&host), 10);
}
