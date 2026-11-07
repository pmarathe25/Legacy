//! Synthetic selection benchmark; setup and result destruction are not timed.
use game_economy_person::{Person, travel::Colocation};
use game_economy_taking::{
    Complaint, ComplaintAccount, ComplaintAnswer, ComplaintDecision, Context, Outcome, Policy,
    Report, ReportBelief, Taking,
};

#[test]
#[ignore = "manual assessment-conditioned selection benchmark"]
fn assessment_selection_benchmark() {
    let count = std::env::var("LEGACY_PEOPLE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(100_000);
    let workers = std::env::var("LEGACY_WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let conditional = std::env::var_os("LEGACY_ASSESSMENT_CONDITION").is_some();
    let people: Vec<_> = (1..=count * 2)
        .map(|id| Person {
            id,
            region: Some(1),
            health: 100,
            hunger: 0,
            immortal: None,
            willing_to_work: false,
            seek_meals: false,
            controlled: false,
        })
        .collect();
    let policies: Vec<_> = (1..=count)
        .map(|target| Policy {
            person: count + target,
            target,
            good: 1,
            quantity: 1,
            willing: true,
            interval: 1,
            recover_incapacitated: false,
            escalate: false,
            pressured_by: None,
            pressure_threshold: None,
            affinity_at_least: None,
            affinity_at_most: None,
            assessment_at_least: None,
            assessment_at_most: conditional.then_some(-3),
        })
        .collect();
    let complaints: Vec<_> = (1..=count)
        .map(|target| {
            let recipient = count + target;
            Complaint {
                author: target,
                recipient,
                witness: target,
                account: ComplaintAccount {
                    actor: target,
                    target: recipient,
                    good: 1,
                    quantity: 1,
                    observed_at: 1,
                    outcome: Outcome::Transferred,
                    source: None,
                },
                created_at: 2,
                expires_at: 8,
                decision: ComplaintDecision::Accepted,
                decided_at: Some(3),
                answer: Some(ComplaintAnswer {
                    report: Report {
                        speaker: target,
                        recipient,
                        actor: target,
                        target: recipient,
                        good: 1,
                        quantity: 1,
                        observed_at: 1,
                        received_at: 4,
                        claimed_outcome: Outcome::Transferred,
                        belief: ReportBelief::Unassessed,
                        source_speaker: None,
                    },
                    matches_account: true,
                    assessment: Some(-4),
                    precedent: None,
                }),
            }
        })
        .collect();
    let owner = Taking {
        policies: policies.into(),
        complaints: complaints.into(),
        ..Default::default()
    };
    owner
        .validate(&(1..=count * 2).collect(), &[1].into_iter().collect(), 4)
        .unwrap();
    let locations = Colocation::new(None);
    rayon::ThreadPoolBuilder::new().num_threads(workers).build().unwrap().install(|| {
        let mut samples = Vec::new();
        for round in 0..6 {
            let now = std::time::Instant::now();
            let plan = Context { owner: &owner, people: &people, opinions: None, locations: &locations,
                busy: &[], blocked: &[], can_touch: &|_, _| true, max_hunger: 100, manual: None,
                action_base: 0, seed: 7, tick: 5 }.prepare().unwrap();
            let elapsed = now.elapsed().as_secs_f64();
            assert_eq!(plan.actions.len(), count as usize);
            if round > 0 { samples.push(elapsed); }
        }
        samples.sort_by(f64::total_cmp);
        println!("workers={workers} actors={count} people={} conditional={conditional} elapsed_min={:.6} elapsed_median={:.6} elapsed_max={:.6}", count * 2, samples[0], samples[2], samples[4]);
    });
}
