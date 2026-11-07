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
    let sample_count: usize = std::env::var("LEGACY_SAMPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(3);
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
    let assessments = taking_inquiries && std::env::var_os("LEGACY_INQUIRY_ASSESSMENTS").is_some();
    let history_stress = std::env::var_os("LEGACY_SOCIAL_HISTORY_STRESS").is_some();
    let inquiry_pursuit =
        taking_inquiries && std::env::var_os("LEGACY_DISABLE_INQUIRY_PURSUIT").is_none();
    assert!(people > 4 && ticks > 0 && sample_count > 0 && reproduction <= 100);
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
    if history_stress {
        for (original, replacement) in [
            ("lifespan_min = 50", "lifespan_min = 1000"),
            ("lifespan_max = 70", "lifespan_max = 1000"),
            ("hunger_per_tick = 1", "hunger_per_tick = 0"),
            ("exposure_per_thousand = 1000", "exposure_per_thousand = 0"),
            ("exposure_per_thousand = 2000", "exposure_per_thousand = 0"),
            ("exhaustion_damage = 1", "exhaustion_damage = 0"),
        ] {
            assert!(
                scenario_source.contains(original),
                "history stress setting missing: {original}"
            );
            scenario_source = scenario_source.replacen(original, replacement, 1);
        }
        let purpose = scenario_source
            .find("[purpose.rules]")
            .expect("purpose rules");
        let damage = purpose
            + scenario_source[purpose..]
                .find("damage = 1")
                .expect("purpose damage");
        scenario_source.replace_range(damage..damage + "damage = 1".len(), "damage = 0");
    }
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
                person: actor, target, good: bread, quantity: 1, willing: true, interval: 1, recover_incapacitated: false,
                escalate: false, pressured_by: None, pressure_threshold: None,
                affinity_at_least: None, affinity_at_most: None, assessment_at_least: None, assessment_at_most: None,
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
            let mut complaint_preferences = groups.iter().map(|&(_, _, recipient)| ComplaintPreference {
                person: recipient, willing: true, pressure_threshold: None,
            }).collect::<Vec<_>>();
            complaint_preferences.sort_unstable_by_key(|preference| preference.person);
            let mut assessment_preferences = if assessments { groups.iter().map(|&(_, _, recipient)| game_economy_taking::AssessmentPreference {
                person: recipient, matching_unassessed: Some((recipient % 7) as i32 - 3), ..Default::default()
            }).collect::<Vec<_>>() } else { Vec::new() };
            assessment_preferences.sort_unstable_by_key(|preference| preference.person);
            let mut pursuing = if inquiry_pursuit { groups.iter().flat_map(|&(_, author, recipient)| [author, recipient]).collect::<Vec<_>>() } else { Vec::new() };
            pursuing.sort_unstable();
            initial.taking = Some(Taking { policies: policies.into(), responses: responses.into(), witnesses: witnesses.into(), report_policies: report_policies.into(), complaint_preferences: complaint_preferences.into(), assessment_preferences: assessment_preferences.into(), inquiry_pursuit: pursuing.into(), ..Default::default() });
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
                }, created_at: now, expires_at, decision: ComplaintDecision::Pending, decided_at: None, previous_case: None, answer: None })
            }).collect::<Vec<_>>();
            assert!(!complaints.is_empty(), "setup must produce actual witnessed takings");
            taking.complaints = complaints.into();
            taking.policies_mut().iter_mut().for_each(|policy| policy.willing = false);
        }
        let mut samples = Vec::new();
        let mut expected = None;
        let mut final_population = 0;
        let mut living_people = 0;
        let mut started_jobs = 0;
        let mut delivered_jobs = 0;
        let mut remembered_encounters = 0;
        let mut encounter_statements = 0;
        let mut inquiry_requests = 0;
        let mut inquiry_decisions = 0;
        let mut inquiry_accepted = 0;
        let mut inquiry_declined = 0;
        let mut inquiry_expired = 0;
        let mut inquiry_answers = 0;
        let mut inquiry_assessments = 0;
        let mut social_relationships = 0;
        let mut directed_opinions = 0;
        let mut taking_reports = 0;
        for round in 0..=sample_count {
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
            living_people = state.people.iter().filter(|person| person.health > 0).count();
            social_relationships = state.dynamics.as_ref().and_then(|d| d.social.as_ref()).map_or(0, |s| s.relationships.len());
            directed_opinions = state.opinions.as_ref().map_or(0, |owner| owner.records.len());
            taking_reports = state.taking.as_ref().map_or(0, |owner| owner.reports.len());
            if encounter_reports {
                let minds = &state.knowledge.as_ref().unwrap().minds;
                remembered_encounters = minds.iter().map(|mind| mind.people_seen.len()).sum::<usize>();
                encounter_statements = minds.iter().map(|mind| mind.people_reports.len()).sum::<usize>();
            }
            if taking_inquiries {
                let complaints = &state.taking.as_ref().unwrap().complaints;
                inquiry_requests = complaints.len();
                inquiry_decisions = complaints.iter().filter(|row| row.decision != game_economy_taking::ComplaintDecision::Pending).count();
                inquiry_accepted = complaints.iter().filter(|row| row.decision == game_economy_taking::ComplaintDecision::Accepted).count();
                inquiry_declined = complaints.iter().filter(|row| row.decision == game_economy_taking::ComplaintDecision::Declined).count();
                inquiry_expired = complaints.iter().filter(|row| row.decision == game_economy_taking::ComplaintDecision::Expired).count();
                inquiry_answers = complaints.iter().filter(|row| row.answer.is_some()).count();
                inquiry_assessments = complaints.iter().filter(|row| row.answer.as_ref().is_some_and(|answer| answer.assessment.is_some())).count();
            }
            let bytes = serde_json::to_vec(&state).unwrap();
            let hash = bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3));
            if let Some(expected) = expected { assert_eq!(hash, expected, "repeated runs must match"); } else { expected = Some(hash); }
            if round > 0 { samples.push(elapsed); }
        }
        samples.sort_by(f64::total_cmp);
        let middle = samples.len() / 2;
        let median = if samples.len() % 2 == 0 { (samples[middle - 1] + samples[middle]) / 2.0 } else { samples[middle] };
        println!("workers={workers} initial_people={people} final_people={final_population} living_people={living_people} history_stress={history_stress} reproduction_percent={reproduction} grain_per_person={grain_per_person} ticks={ticks} samples={sample_count} started_jobs={started_jobs} delivered_jobs={delivered_jobs} social_relationships={social_relationships} directed_opinions={directed_opinions} taking_reports={taking_reports} remembered_encounters={remembered_encounters} encounter_statements={encounter_statements} inquiry_pursuit={inquiry_pursuit} inquiry_requests={inquiry_requests} inquiry_decisions={inquiry_decisions} inquiry_accepted={inquiry_accepted} inquiry_declined={inquiry_declined} inquiry_expired={inquiry_expired} inquiry_answers={inquiry_answers} inquiry_assessments={inquiry_assessments} elapsed_min={:.6} elapsed_median={median:.6} elapsed_max={:.6} state_fingerprint={:016x}", samples[0], samples[samples.len()-1], expected.unwrap());
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
