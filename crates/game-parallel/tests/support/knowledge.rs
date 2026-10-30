use game_parallel::{KnowledgeRecord, Move, World};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/knowledge.toml")).unwrap()
}
fn records(w: &World, person: u64) -> &[KnowledgeRecord] {
    &w.knowledge
        .as_ref()
        .unwrap()
        .minds
        .iter()
        .find(|m| m.person == person)
        .unwrap()
        .records
}

#[test]
fn local_observation_and_report_preserve_source_and_staleness() {
    let mut w = world();
    w.regions[1].food = 40;
    w.step().unwrap();
    let local = records(&w, 2).iter().find(|r| r.region == 1).unwrap();
    assert_eq!(
        (
            local.food,
            local.observed_at,
            local.witness,
            local.learned_from
        ),
        (10, 0, 2, None)
    );
    let reported = records(&w, 2).iter().find(|r| r.region == 2).unwrap();
    assert_eq!(
        (
            reported.food,
            reported.observed_at,
            reported.witness,
            reported.learned_from
        ),
        (4, 0, 3, Some(1))
    );
    assert!(records(&w, 3).iter().all(|r| r.region != 1));
    w = World::from_json(&serde_json::to_string(&w).unwrap()).unwrap();
    w.move_people(&[Move {
        person: 2,
        destination: 2,
    }])
    .unwrap();
    w.step().unwrap();
    w.step().unwrap();
    assert_eq!(
        records(&w, 2).iter().find(|r| r.region == 2).unwrap().food,
        4
    );
    let observed_food = w.regions[1].food;
    let observed_at = w.tick;
    w.step().unwrap();
    let direct = records(&w, 2).iter().find(|r| r.region == 2).unwrap();
    assert_eq!(
        (
            direct.food,
            direct.observed_at,
            direct.witness,
            direct.learned_from
        ),
        (observed_food, observed_at, 2, None)
    );
    w.validate().unwrap();
}

#[test]
fn no_encounter_means_no_remote_report() {
    let mut w = world();
    w.social.as_mut().unwrap().participants[1].willing = false;
    w.step().unwrap();
    assert!(records(&w, 2).iter().all(|r| r.region != 2));
    assert_eq!(records(&w, 2)[0].region, 1);
}

#[test]
fn small_memory_retains_direct_local_observation() {
    let mut w = world();
    w.knowledge.as_mut().unwrap().max_records = 1;
    w.step().unwrap();
    assert_eq!(records(&w, 2).len(), 1);
    assert_eq!(records(&w, 2)[0].region, 1);
    assert_eq!(records(&w, 2)[0].learned_from, None);
}

#[test]
fn invalid_knowledge_and_failed_family_phase_leave_everything_unchanged() {
    let mut w = world();
    w.knowledge.as_mut().unwrap().minds[0].records[0].observed_at = 1;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.social = world().social;
    w.knowledge = world().knowledge;
    w.families.as_mut().unwrap().members[7].age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn personal_knowledge_is_deterministic_across_workers_and_layout() {
    let mut outcomes = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = world();
        if workers > 1 {
            w.regions.reverse();
            for r in &mut w.regions {
                r.people.reverse();
            }
            w.knowledge.as_mut().unwrap().minds.reverse();
        }
        pool.install(|| {
            for _ in 0..5 {
                w.step().unwrap();
            }
        });
        w.regions.sort_unstable_by_key(|r| r.id);
        for r in &mut w.regions {
            r.people.sort_unstable_by_key(|p| p.id);
        }
        w.knowledge
            .as_mut()
            .unwrap()
            .minds
            .sort_unstable_by_key(|m| m.person);
        outcomes.push(w);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}
