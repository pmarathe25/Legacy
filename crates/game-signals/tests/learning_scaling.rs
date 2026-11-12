use game_signals::{Catalog, Event, GiftEvidence, Mind, Named, Signals};

#[test]
#[ignore = "personal signal learning scaling"]
fn signal_learning_scaling() {
    let count: u64 = std::env::var("LEGACY_SIGNAL_LEARN_COUNT")
        .ok()
        .map(|value| value.parse().expect("invalid population"))
        .unwrap_or(100_000);
    let concentrated = std::env::var_os("LEGACY_SIGNAL_LEARN_CONCENTRATED").is_some();
    let template: Mind = serde_json::from_value(serde_json::json!({
        "person":1, "idle":0, "attention":1000,
        "learning_rules":[{"good":7,"meaning":2,"enabled":true,"value":10}]
    }))
    .unwrap();
    let minds = if concentrated {
        vec![template.clone()]
    } else {
        (1..=count)
            .map(|id| {
                let mut mind = template.clone();
                mind.person = id;
                mind
            })
            .collect()
    };
    let mut events: Vec<_> = (1..=count)
        .map(|id| Event {
            tick: 1,
            actor: count + id,
            observer: if concentrated { 1 } else { id },
            form: 1,
            noticed: true,
            interpreted: None,
        })
        .collect();
    events.sort_unstable_by_key(|event| (event.observer, event.actor));
    let gifts: Vec<_> = events
        .iter()
        .map(|event| GiftEvidence {
            actor: event.actor,
            observer: event.observer,
            good: 7,
        })
        .collect();
    let state = Signals {
        catalog: Catalog {
            forms: vec![Named {
                id: 1,
                name: "cry".into(),
            }],
            meanings: vec![Named {
                id: 2,
                name: "comfort".into(),
            }],
        },
        birth_profile: None,
        minds,
        events,
        learning_events: vec![],
    };
    let mut reference = None;
    for workers in [1, 2, 4, 6] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut times = Vec::new();
        for _ in 0..3 {
            let mut learned = pool.install(|| state.clone());
            let start = std::time::Instant::now();
            pool.install(|| {
                let draft = learned.prepare_gift_learning(&gifts, 2).unwrap().unwrap();
                draft.apply_validated(&mut learned);
            });
            times.push(start.elapsed());
            assert_eq!(
                learned.learning_events.len() as u64,
                if concentrated { 1 } else { count }
            );
            if let Some(expected) = &reference {
                assert_eq!(&learned, expected);
            } else {
                reference = Some(learned);
            }
        }
        times.sort_unstable();
        eprintln!(
            "{workers} workers: {:?} median for {count} {} inferences",
            times[1],
            if concentrated {
                "concentrated"
            } else {
                "distributed"
            }
        );
    }
}
