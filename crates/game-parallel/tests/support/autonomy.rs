use game_parallel::{AutonomyOutcome, KnowledgeRecord, World};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/autonomy.toml")).unwrap()
}
fn remember_two_places(w: &mut World, local: u32, remote: u32) {
    w.social = None;
    w.knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|m| m.person == 2)
        .unwrap()
        .records = vec![
        KnowledgeRecord {
            region: 1,
            food: local,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        },
        KnowledgeRecord {
            region: 2,
            food: remote,
            observed_at: 0,
            witness: 3,
            learned_from: Some(1),
        },
    ];
}

#[test]
fn person_chooses_using_stale_personal_memory_not_true_remote_stock() {
    let mut w = world();
    remember_two_places(&mut w, 0, 50);
    w.regions[1].food = 0;
    w.step().unwrap();
    let event = &w.autonomy.as_ref().unwrap().events[0];
    assert_eq!(
        (event.person, event.source, event.destination, event.outcome),
        (2, 1, 2, AutonomyOutcome::Started)
    );
    assert!(
        w.travelers
            .iter()
            .any(|t| t.person.id == 2 && t.destination == 2)
    );
    w.validate().unwrap();
    let saved = serde_json::to_string(&w).unwrap();
    let mut resumed = World::from_json(&saved).unwrap();
    w.step().unwrap();
    resumed.step().unwrap();
    assert_eq!(w, resumed);
}

#[test]
fn report_becomes_usable_only_on_a_later_review() {
    let mut w = world();
    w.knowledge.as_mut().unwrap().minds[0].records[0].food = 30;
    w.regions[0].people[1].hunger = 3;
    w.autonomy.as_mut().unwrap().rules.min_hunger = 0;
    w.step().unwrap();
    assert!(w.autonomy.as_ref().unwrap().events.is_empty());
    assert!(
        w.knowledge.as_ref().unwrap().minds[1]
            .records
            .iter()
            .any(|r| r.region == 2 && r.learned_from == Some(1))
    );
    w.social.as_mut().unwrap().participants[0].willing = false;
    w.step().unwrap();
    assert_eq!(
        w.autonomy.as_ref().unwrap().events[0].outcome,
        AutonomyOutcome::Started
    );
    assert_eq!(w.travelers[0].person.id, 2);
}

#[test]
fn stale_or_missing_information_does_not_create_departure() {
    let mut w = world();
    remember_two_places(&mut w, 0, 50);
    w.tick = 2;
    w.autonomy.as_mut().unwrap().rules.max_observation_age = 0;
    w.step().unwrap();
    assert!(w.autonomy.as_ref().unwrap().events.is_empty());
    assert!(w.travelers.is_empty());
    assert_eq!(w.autonomy.as_ref().unwrap().actors[0].next_review, 4);
    let mut w = world();
    w.knowledge = None;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn actor_who_dies_during_tick_does_not_depart() {
    let mut w = world();
    remember_two_places(&mut w, 0, 50);
    w.regions[0].people[1].health = 1;
    w.rules.starvation_threshold = 1;
    w.rules.starvation_damage = 1;
    w.rules.food_growth = 0;
    w.regions[0].food = 0;
    w.step().unwrap();
    assert_eq!(
        w.autonomy.as_ref().unwrap().events[0].outcome,
        AutonomyOutcome::Unavailable
    );
    assert!(w.travelers.is_empty());
}

#[test]
fn failing_family_or_review_overflow_does_not_publish_decisions() {
    let initial = world();
    let mut w =
        World::from_toml(include_str!("../../../../content/parallel/families.toml")).unwrap();
    w.social = initial.social.clone();
    w.knowledge = initial.knowledge.clone();
    w.autonomy = initial.autonomy.clone();
    remember_two_places(&mut w, 0, 50);
    w.families.as_mut().unwrap().members[7].age = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
    let mut w = initial;
    w.social = None;
    w.tick = 1;
    w.autonomy.as_mut().unwrap().rules.review_interval = u64::MAX;
    let before = w.clone();
    assert!(w.step().is_err());
    assert_eq!(w, before);
}

#[test]
fn large_knowledge_driven_departures_are_worker_independent() {
    use game_parallel::{AutonomousActor, Mind, Person};
    let mut base = world();
    base.social = None;
    base.rules.food_growth = 0;
    base.regions[0].people = (1..=5000)
        .rev()
        .map(|id| Person {
            id,
            hunger: 1,
            health: 100,
        })
        .collect();
    base.regions[1].people.clear();
    base.knowledge.as_mut().unwrap().minds = (1..=5000)
        .map(|person| Mind {
            person,
            records: vec![
                KnowledgeRecord {
                    region: 1,
                    food: 0,
                    observed_at: 0,
                    witness: person,
                    learned_from: None,
                },
                KnowledgeRecord {
                    region: 2,
                    food: 100,
                    observed_at: 0,
                    witness: person,
                    learned_from: None,
                },
            ],
        })
        .collect();
    base.autonomy.as_mut().unwrap().actors = (1..=5000)
        .map(|person| AutonomousActor {
            person,
            enabled: true,
            next_review: 0,
            travel_caution: 0,
            hunger_urgency: 0,
        })
        .collect();
    base.validate().unwrap();
    let mut outcomes = Vec::new();
    for workers in [1, 4] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let mut w = base.clone();
        pool.install(|| w.step().unwrap());
        assert_eq!(w.travelers.len(), 5000);
        assert_eq!(w.autonomy.as_ref().unwrap().events.len(), 5000);
        assert!(
            w.autonomy
                .as_ref()
                .unwrap()
                .events
                .iter()
                .all(|e| e.outcome == AutonomyOutcome::Started)
        );
        w.validate().unwrap();
        outcomes.push(w);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn policy_rejection_is_recorded_without_partial_departure() {
    use game_parallel::Cargo;
    let mut w = world();
    remember_two_places(&mut w, 0, 50);
    w.cargo = Some(Cargo {
        capacity: 0,
        auto_eat_travel: false,
        auto_eat_local: false,
        personal_food_access: false,
        acquire_when_hungry: vec![],
        food_aid: vec![],
        food_stores: vec![],
        food_store_claims: vec![],
        food_store_choices: vec![],
        food_depositors: vec![],
        food_store_blueprints: vec![],
        food_store_builders: vec![],
        inherit_store_building: false,
        food_sharing: vec![],
        inherit_food_sharing: false,
        food_requests: vec![],
        next_food_request: 0,
        food_askers: vec![],
        auto_seek_age: None,
        food_enrollments: vec![],
        auto_pack_travel: true,
        holdings: Vec::new(),
    });
    w.step().unwrap();
    let event = &w.autonomy.as_ref().unwrap().events[0];
    assert_eq!(event.outcome, AutonomyOutcome::Rejected);
    assert!(event.reason.is_some());
    assert!(w.travelers.is_empty());
    assert!(w.regions[0].people.iter().any(|p| p.id == 2));
}

#[test]
fn autonomous_departure_uses_provision_policy_and_carries_food() {
    use game_parallel::Cargo;
    let mut w = world();
    remember_two_places(&mut w, 0, 50);
    w.rules.food_growth = 0;
    w.regions[0].food = 10;
    w.cargo = Some(Cargo {
        capacity: 10,
        auto_eat_travel: true,
        auto_eat_local: false,
        personal_food_access: false,
        acquire_when_hungry: vec![],
        food_aid: vec![],
        food_stores: vec![],
        food_store_claims: vec![],
        food_store_choices: vec![],
        food_depositors: vec![],
        food_store_blueprints: vec![],
        food_store_builders: vec![],
        inherit_store_building: false,
        food_sharing: vec![],
        inherit_food_sharing: false,
        food_requests: vec![],
        next_food_request: 0,
        food_askers: vec![],
        auto_seek_age: None,
        food_enrollments: vec![],
        auto_pack_travel: true,
        holdings: Vec::new(),
    });
    w.step().unwrap();
    assert_eq!(
        w.autonomy.as_ref().unwrap().events[0].outcome,
        AutonomyOutcome::Started
    );
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 4);
    w.step().unwrap();
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 2);
    w.step().unwrap();
    assert!(w.regions[1].people.iter().any(|p| p.id == 2));
    assert_eq!(w.cargo.as_ref().unwrap().holdings[0].food, 2);
    w.validate().unwrap();
}

#[test]
fn individual_dispositions_change_travel_decisions_without_changing_knowledge() {
    let mut cautious = world();
    remember_two_places(&mut cautious, 0, 50);
    cautious.autonomy.as_mut().unwrap().actors[0].travel_caution = 60;
    let mut adventurous = cautious.clone();
    adventurous.autonomy.as_mut().unwrap().actors[0].travel_caution = 0;
    cautious.step().unwrap();
    adventurous.step().unwrap();
    assert!(cautious.travelers.is_empty());
    assert_eq!(adventurous.travelers[0].person.id, 2);
    assert_eq!(
        World::from_json(&serde_json::to_string(&cautious).unwrap()).unwrap(),
        cautious
    );
}

#[test]
fn hunger_urgency_personalizes_when_an_actor_starts_seeking_food() {
    let mut patient = world();
    remember_two_places(&mut patient, 0, 50);
    patient.autonomy.as_mut().unwrap().rules.min_hunger = 8;
    let mut urgent = patient.clone();
    urgent.autonomy.as_mut().unwrap().actors[0].hunger_urgency = 1;
    patient.step().unwrap();
    urgent.step().unwrap();
    assert!(patient.travelers.is_empty());
    assert_eq!(urgent.travelers[0].person.id, 2);
}

#[path = "social.rs"]
mod social;

#[path = "agriculture.rs"]
mod agriculture;
#[path = "cargo.rs"]
mod cargo;

#[path = "knowledge.rs"]
mod knowledge;
#[path = "lineage.rs"]
mod lineage;

#[path = "goods_cargo.rs"]
mod goods_cargo;
#[path = "market.rs"]
mod market;

#[path = "agreements.rs"]
mod agreements;
#[path = "autowork.rs"]
mod autowork;
#[path = "production.rs"]
mod production;

#[path = "families.rs"]
mod families;
#[path = "simulation.rs"]
mod simulation;
