use std::path::PathBuf;

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let people: u64 = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10_000);
    let ticks: usize = std::env::var("LEGACY_TICKS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);
    let reproduction: u8 = std::env::var("LEGACY_REPRODUCTION")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(80);
    let grain_per_person: u64 = std::env::var("LEGACY_GRAIN_PER_PERSON")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(20);
    let encounter_reports = std::env::var_os("LEGACY_ENCOUNTER_REPORTS").is_some();
    let taking_inquiries = std::env::var_os("LEGACY_TAKING_INQUIRIES").is_some();
    assert!(people > 4 && ticks > 0 && reproduction <= 100);
    let profile =
        std::fs::read_to_string(root.join("content/scenarios/campaign-population.toml")).unwrap();
    assert!(profile.contains("count = 128"));
    let profile = profile.replacen("count = 128", &format!("count = {}", people - 4), 1);
    assert!(profile.contains("reproduction_percent = 80"));
    let profile = profile.replacen(
        "reproduction_percent = 80",
        &format!("reproduction_percent = {reproduction}"),
        1,
    );
    let path = std::env::temp_dir().join(format!(
        "legacy-mixed-benchmark-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, profile).unwrap();
    let mut scenario_source =
        std::fs::read_to_string(root.join("content/scenarios/economy-campaign.toml")).unwrap();
    if grain_per_person > 0 {
        let grain = people
            .checked_mul(grain_per_person)
            .expect("grain quantity overflow");
        assert!(scenario_source.contains("stock_capacity = 1000"));
        assert!(scenario_source.contains("grain = 100, grain-seed"));
        assert!(scenario_source.contains("stock = { bread = 20 }"));
        scenario_source = scenario_source
            .replacen(
                "stock_capacity = 1000",
                &format!("stock_capacity = {}", grain.max(1000)),
                1,
            )
            .replacen(
                "grain = 100, grain-seed",
                &format!("grain = {grain}, grain-seed"),
                1,
            )
            .replacen(
                "stock = { bread = 20 }",
                &format!("stock = {{ bread = 20, grain = {grain} }}"),
                1,
            );
    }
    let scenario_path =
        std::env::temp_dir().join(format!("legacy-mixed-scenario-{}.toml", std::process::id()));
    std::fs::write(&scenario_path, scenario_source).unwrap();
    rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut initial = game_economy_cli::scenario_with_options(
            &scenario_path, &root.join("content"), Some(42), Some(&path),
        ).unwrap().snapshot();
        assert_eq!(initial.people.len(), people as usize);
        if encounter_reports {
            let knowledge = initial.knowledge.as_mut().expect("campaign knowledge");
            let existing: std::collections::HashSet<_> = knowledge.minds.iter().map(|mind| mind.person).collect();
            for person in &initial.people {
                if !existing.contains(&person.id) {
                    knowledge.minds.push(game_economy_knowledge::Mind {
                        person: person.id,
                        remember_people: true,
                        people_seen: Vec::new(),
                        people_reporting: Some(game_economy::Reporting { willing: true, pressure_threshold: None }),
                        people_reports: Vec::new(),
                        reporting: None,
                        misreport: None,
                        records: Vec::new(),
                    });
                }
            }
            for mind in &mut knowledge.minds {
                mind.remember_people = true;
                mind.people_reporting = Some(game_economy::Reporting { willing: true, pressure_threshold: None });
            }
            knowledge.minds.sort_unstable_by_key(|mind| mind.person);
        }
        if taking_inquiries {
            use game_economy_taking::{Complaint, ComplaintAccount, ComplaintDecision, ComplaintPreference, Policy, ReportPolicy, Response, Taking, Witness};
            let bread = initial.physical.goods.iter().find(|good| good.name == "bread").unwrap().id;
            let mut groups = Vec::new();
            for region in [1, 2] {
                let ids: Vec<_> = initial.people.iter().filter(|p| p.id >= 1000 && p.region == Some(region)).map(|p| p.id).collect();
                groups.extend(ids.as_chunks::<3>().0.iter().map(|&[a, b, c]| (a, b, c)));
            }
            let policies = groups.iter().map(|&(actor, target, _)| Policy {
                person: actor, target, good: bread, quantity: 1, willing: true, interval: 1,
                escalate: false, pressured_by: None, pressure_threshold: None,
                affinity_at_least: None, affinity_at_most: None,
            }).collect::<Vec<_>>();
            let responses = groups.iter().map(|&(_, target, _)| Response {
                person: target, notices: true, resist: false,
                resist_if_fear_at_least: None, resist_if_affinity_at_least: None, resist_if_affinity_at_most: None,
            }).collect::<Vec<_>>();
            let witnesses = groups.iter().map(|&(actor, target, _)| Witness {
                person: target, actor, notices: true, ..Default::default()
            }).collect::<Vec<_>>();
            let report_policies = groups.iter().map(|&(actor, target, recipient)| ReportPolicy {
                person: target, recipient, actor, willing: true, pressure_threshold: None,
                claimed_outcome: None, source: Default::default(), heard_from: None, details: None,
            }).collect::<Vec<_>>();
            let complaint_preferences = groups.iter().map(|&(_, _, recipient)| ComplaintPreference {
                person: recipient, willing: true, pressure_threshold: None,
            }).collect::<Vec<_>>();
            initial.taking = Some(Taking { policies: policies.into(), responses: responses.into(), witnesses: witnesses.into(), report_policies: report_policies.into(), complaint_preferences: complaint_preferences.into(), ..Default::default() });
            let mut staged = game_economy::Economy::compile(initial).unwrap();
            staged.advance().unwrap();
            initial = staged.snapshot();
            let now = initial.physical.custody.tick;
            let expires_at = now.checked_add(ticks as u64 + 2).unwrap();
            let taking = initial.taking.as_mut().unwrap();
            let observed: std::collections::HashMap<_, _> = taking.witnesses.iter().filter_map(|w| w.last_seen.map(|seen| ((w.actor, w.person), seen))).collect();
            let complaints = groups.iter().filter_map(|&(actor, author, recipient)| {
                let seen = observed.get(&(actor, author))?;
                Some(Complaint { author, recipient, witness: author, account: ComplaintAccount {
                    actor, target: seen.target, good: seen.good, quantity: seen.quantity,
                    observed_at: seen.tick, outcome: seen.outcome, source: None,
                }, created_at: now, expires_at, decision: ComplaintDecision::Pending, decided_at: None, answer: None })
            }).collect::<Vec<_>>();
            assert!(!complaints.is_empty(), "setup must produce actual witnessed takings");
            taking.complaints = complaints.into();
            taking.policies_mut().iter_mut().for_each(|policy| policy.willing = false);
        }
        let mut samples = Vec::new();
        let mut expected = None;
        let mut final_population = 0;
        let mut started_jobs = 0;
        let mut delivered_jobs = 0;
        let mut remembered_encounters = 0;
        let mut encounter_statements = 0;
        let mut inquiry_requests = 0;
        let mut inquiry_decisions = 0;
        let mut inquiry_answers = 0;
        for round in 0..4 {
            let mut world = game_economy::Economy::compile(initial.clone()).unwrap();
            let started = std::time::Instant::now();
            started_jobs = 0;
            delivered_jobs = 0;
            for _ in 0..ticks {
                let events = world.advance().unwrap();
                started_jobs += events.started.len();
                delivered_jobs += events.delivered.len();
            }
            let elapsed = started.elapsed().as_secs_f64();
            let state = world.snapshot();
            final_population = state.people.len();
            if encounter_reports {
                let minds = &state.knowledge.as_ref().unwrap().minds;
                remembered_encounters = minds.iter().map(|mind| mind.people_seen.len()).sum::<usize>();
                encounter_statements = minds.iter().map(|mind| mind.people_reports.len()).sum::<usize>();
            }
            if taking_inquiries {
                let complaints = &state.taking.as_ref().unwrap().complaints;
                inquiry_requests = complaints.len();
                inquiry_decisions = complaints.iter().filter(|row| row.decision != game_economy_taking::ComplaintDecision::Pending).count();
                inquiry_answers = complaints.iter().filter(|row| row.answer.is_some()).count();
            }
            let bytes = serde_json::to_vec(&state).unwrap();
            let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3));
            if let Some(expected) = expected { assert_eq!(hash, expected, "repeated runs must match"); } else { expected = Some(hash); }
            if round > 0 { samples.push(elapsed); }
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} initial_people={people} final_people={final_population} reproduction_percent={reproduction} grain_per_person={grain_per_person} ticks={ticks} started_jobs={started_jobs} delivered_jobs={delivered_jobs} remembered_encounters={remembered_encounters} encounter_statements={encounter_statements} inquiry_requests={inquiry_requests} inquiry_decisions={inquiry_decisions} inquiry_answers={inquiry_answers} elapsed_min={:.6} elapsed_median={:.6} elapsed_max={:.6} state_fingerprint={:016x}", samples[0], samples[1], samples[2], expected.unwrap());
        #[cfg(feature = "profile")]
        if std::env::var_os("LEGACY_PROFILE").is_some() {
            for (name, measurement) in game_diagnostics::snapshot() {
                println!("{name}: {:.3} ms/call", measurement.nanoseconds as f64 / measurement.calls as f64 / 1_000_000.0);
            }
        }
    });
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(scenario_path).unwrap();
}
