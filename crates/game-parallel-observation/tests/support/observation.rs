use crate::{Location, guardian_search, ruler};
use game_parallel::{Knowledge, KnowledgeRecord, Mind, World};

fn world() -> World {
    World::from_toml(include_str!("../../../../content/parallel/guardians.toml")).unwrap()
}

#[test]
fn ruler_sees_only_personal_nutrient_reserves_and_default_baseline() {
    let mut world = world();
    world.nutrition = Some(game_parallel::Nutrition {
        rules: game_parallel::NutritionRules {
            interval: 5,
            capacity: [100; 6],
            initial: [20; 6],
            metabolism: [1; 6],
            minimum: [2; 6],
            deficiency_damage: 1,
            scalar_meal: [4; 6],
        },
        people: vec![game_parallel::NutritionPerson {
            person: 2,
            reserves: [7; 6],
        }],
    });
    assert_eq!(ruler(&world).unwrap().nutrition.unwrap().reserves, [20; 6]);
    world
        .nutrition
        .as_mut()
        .unwrap()
        .people
        .push(game_parallel::NutritionPerson {
            person: 1,
            reserves: [9; 6],
        });
    assert_eq!(ruler(&world).unwrap().nutrition.unwrap().reserves, [9; 6]);
}
#[test]
fn view_contains_local_information_and_only_personal_stale_reports() {
    let mut world = world();
    world.knowledge = Some(Knowledge {
        max_records: 4,
        minds: vec![
            Mind {
                person: 1,
                records: vec![KnowledgeRecord {
                    region: 2,
                    food: 17,
                    observed_at: 0,
                    witness: 5,
                    learned_from: Some(2),
                }],
            },
            Mind {
                person: 2,
                records: vec![KnowledgeRecord {
                    region: 2,
                    food: 99,
                    observed_at: 0,
                    witness: 2,
                    learned_from: None,
                }],
            },
        ],
    });
    let view = ruler(&world).unwrap();
    assert_eq!(
        view.location,
        Location::Local {
            region: 1,
            food: 100,
            people: vec![1, 2, 3, 4]
        }
    );
    assert_eq!(view.outgoing_routes.len(), 1);
    assert_eq!(
        (
            view.outgoing_routes[0].source,
            view.outgoing_routes[0].destination
        ),
        (1, 2)
    );
    assert_eq!(view.records.len(), 1);
    assert_eq!(view.records[0].food, 17);
    assert_eq!(view.records[0].learned_from, Some(2));
    world.regions[1].food = 500;
    assert_eq!(ruler(&world).unwrap(), view);
}
#[test]
fn guardian_search_is_explicit_read_only_and_ends_after_mentorship_begins() {
    let mut world = world();
    assert!(guardian_search(&world, 1).is_err());
    world.guardians.as_mut().unwrap().guardians[0].student = Some(1);
    let before = world.clone();
    let view = guardian_search(&world, 1).unwrap();
    assert_eq!(view.region, 2);
    assert_eq!(world, before);
    world.guardians.as_mut().unwrap().guardians[0].mentorship = 1;
    assert!(guardian_search(&world, 1).is_err());
}
#[test]
fn traveling_ruler_does_not_observe_the_destination() {
    let mut world = world();
    let person = world.regions[0].people.remove(0);
    world.travelers.push(game_parallel::Traveler {
        person,
        source: 1,
        destination: 2,
        ticks_remaining: 3,
    });
    assert!(ruler(&world).unwrap().outgoing_routes.is_empty());
    assert_eq!(
        ruler(&world).unwrap().location,
        Location::Traveling {
            source: 1,
            destination: 2,
            ticks_remaining: 3
        }
    );
}

#[test]
fn view_includes_only_the_rulers_disposition() {
    let mut world = world();
    let autonomy = World::from_toml(include_str!(
        "../../../../content/parallel/dispositions.toml"
    ))
    .unwrap()
    .autonomy
    .unwrap();
    world.autonomy = Some(autonomy);
    assert!(ruler(&world).unwrap().disposition.is_none());
    world.lineage.as_mut().unwrap().current = Some(2);
    let disposition = ruler(&world).unwrap().disposition.unwrap();
    assert_eq!(disposition.person, 2);
    assert_eq!(disposition.travel_caution, 6);
    assert_eq!(disposition.hunger_urgency, 2);
}

#[test]
fn view_includes_own_cargo_without_disclosing_other_peoples_holdings() {
    use game_parallel::{Amount, Cargo, GoodsCargo, GoodsHolding, Holding};
    let mut world = world();
    world.cargo = Some(Cargo {
        capacity: 20,
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
        auto_pack_travel: false,
        holdings: vec![
            Holding { person: 1, food: 3 },
            Holding { person: 2, food: 9 },
        ],
    });
    world.goods_cargo = Some(GoodsCargo {
        capacity: 20,
        holdings: vec![
            GoodsHolding {
                person: 1,
                items: vec![Amount {
                    good: 1,
                    quantity: 2,
                }]
                .into(),
            },
            GoodsHolding {
                person: 2,
                items: vec![Amount {
                    good: 2,
                    quantity: 7,
                }]
                .into(),
            },
        ],
    });
    let view = ruler(&world).unwrap();
    assert_eq!(view.carried_food, 3);
    assert_eq!(
        view.carried_goods,
        vec![Amount {
            good: 1,
            quantity: 2
        }]
    );
}

#[test]
fn only_own_noticeable_illness_is_visible() {
    let mut world = world();
    let mut disease = World::from_toml(include_str!("../../../../content/parallel/disease.toml"))
        .unwrap()
        .disease
        .unwrap();
    disease.cases[0].person = 2;
    world.disease = Some(disease);
    assert!(!ruler(&world).unwrap().illness_symptoms);
    world.disease.as_mut().unwrap().cases[0].person = 1;
    assert!(!ruler(&world).unwrap().illness_symptoms);
    world.tick = 1;
    assert!(ruler(&world).unwrap().illness_symptoms);
    world.disease.as_mut().unwrap().strains[0].damage_per_tick = 0;
    assert!(!ruler(&world).unwrap().illness_symptoms);
}

#[test]
fn view_exposes_only_the_rulers_directed_opinions() {
    use game_parallel::{Opinion, OpinionRules, Opinions};
    let mut state = world();
    state.opinions = Some(Opinions {
        sensitivities: Vec::new(),
        rules: OpinionRules {
            minimum: -5,
            maximum: 5,
            initial: 0,
            fulfilled_gain: 1,
            default_loss: 1,
            theft_loss: 0,
            encounter_gain: 0,
            minimum_report_trust: None,
            affinity: None,
            newborn_obligation_development: None,
            fear_per_damage: 0,
            fear_decay_per_tick: 0,
            gratitude_gain: 0,
            obligation_relief: 0,
        },
        records: vec![
            Opinion {
                observer: 1,
                target: 2,
                trust: 3,
                fear: 0,
                obligation: 0,
                affinity: 0,
            },
            Opinion {
                observer: 2,
                target: 1,
                trust: -4,
                fear: 0,
                obligation: 0,
                affinity: 0,
            },
        ],
        events: vec![],
    });
    let view = ruler(&state).unwrap();
    assert_eq!(
        view.opinions,
        vec![Opinion {
            observer: 1,
            target: 2,
            trust: 3,
            fear: 0,
            obligation: 0,
            affinity: 0,
        }]
    );
}

#[test]
fn view_includes_only_rulers_own_goals() {
    use game_parallel::{Goal, Goals};
    let mut state = world();
    state.goals = Some(Goals {
        retry_interval: 3,
        plan_inputs: false,
        people: vec![
            Goal {
                person: 1,
                recipe: 1,
                priority: 2,
                target: 3,
                progress: 1,
                next_review: 0,
                enabled: true,
            },
            Goal {
                person: 2,
                recipe: 2,
                priority: 3,
                target: 1,
                progress: 0,
                next_review: 0,
                enabled: true,
            },
        ],
        events: vec![],
    });
    let view = ruler(&state).unwrap();
    assert_eq!(view.goals.len(), 1);
    assert_eq!(view.goals[0].person, 1);
}
#[test]
fn view_exposes_only_own_purpose_need() {
    use game_parallel::{Purpose, PurposePerson, PurposeRules};
    let mut state = world();
    state.purpose = Some(Purpose {
        rules: PurposeRules {
            max: 20,
            per_tick: 1,
            blocked_increase: 2,
            completion_relief: 6,
            damage_threshold: 8,
            damage: 2,
        },
        people: vec![
            PurposePerson {
                person: 1,
                deprivation: 4,
                eligible_at: 0,
            },
            PurposePerson {
                person: 2,
                deprivation: 12,
                eligible_at: 0,
            },
        ],
    });
    assert_eq!(ruler(&state).unwrap().purpose.unwrap().deprivation, 4);
}
#[test]
fn view_exposes_only_care_events_involving_the_ruler() {
    use game_parallel::{Caregiving, CaregivingEvent, CaregivingRules};
    let mut state = world();
    state.caregiving = Some(Caregiving {
        inherit_births: false,
        rules: CaregivingRules {
            dependent_until: 5,
            max_caregiver_hunger: 10,
            neglect_damage: 2,
            feed_from_cargo: false,
            fetch_local_food: false,
            pressure_threshold: None,
        },
        policies: vec![],
        events: vec![
            CaregivingEvent {
                tick: 0,
                child: 2,
                caregiver: Some(1),
            },
            CaregivingEvent {
                tick: 0,
                child: 3,
                caregiver: Some(2),
            },
        ],
    });
    let view = ruler(&state).unwrap();
    assert_eq!(view.caregiving.len(), 1);
    assert_eq!(view.caregiving[0].child, 2);
}
#[test]
fn view_exposes_only_the_rulers_own_aspirations() {
    use game_parallel::{Dream, Dreams};
    let mut state = world();
    let dream = |person| Dream {
        person,
        recipe: 1,
        priority: 1,
        batch: 1,
        review_interval: 3,
        next_review: 0,
        enabled: true,
    };
    state.dreams = Some(Dreams {
        inherit_births: false,
        learning: vec![],
        people: vec![dream(1), dream(2)],
        events: vec![],
    });
    let view = ruler(&state).unwrap();
    assert_eq!(view.dreams.len(), 1);
    assert_eq!(view.dreams[0].person, 1);
}
#[test]
fn view_only_exposes_the_rulers_own_expeditions() {
    use game_parallel::{Mission, MissionPhase, Missions};
    let mut state = world();
    let mission = |id, person| Mission {
        id,
        person,
        party: None,
        party_opt_in: false,
        party_compliant: false,
        objective: None,
        objective_met: false,
        origin: 1,
        destination: 2,
        visit_ticks: 1,
        remaining: 0,
        phase: MissionPhase::Ready,
    };
    state.missions = Some(Missions {
        next_id: 3,
        pressure_threshold: None,
        entries: vec![mission(1, 1), mission(2, 2)],
        events: vec![],
    });
    let view = ruler(&state).unwrap();
    assert_eq!(view.missions.len(), 1);
    assert_eq!(view.missions[0].person, 1);
}

#[test]
fn personality_view_exposes_only_the_current_ruler() {
    let mut world = world();
    world.personality = World::from_toml(include_str!(
        "../../../../content/parallel/personality.toml"
    ))
    .unwrap()
    .personality;
    let first = ruler(&world).unwrap().personality.unwrap();
    assert_eq!(
        (first.person, first.sociability, first.diligence),
        (1, 0, 0)
    );
    world.lineage.as_mut().unwrap().current = Some(2);
    let second = ruler(&world).unwrap().personality.unwrap();
    assert_eq!(
        (second.person, second.sociability, second.diligence),
        (2, 1000, 1000)
    );
    world.lineage.as_mut().unwrap().current = Some(4);
    assert!(ruler(&world).unwrap().personality.is_none());
}

#[test]
fn ruler_view_exposes_the_current_calendar_season() {
    let mut world = world();
    world.environment = World::from_toml(include_str!("../../../../content/parallel/seasons.toml"))
        .unwrap()
        .environment;
    assert_eq!(ruler(&world).unwrap().season.as_deref(), Some("warm"));
    world.tick = 3;
    assert_eq!(ruler(&world).unwrap().season.as_deref(), Some("cold"));
}

#[test]
fn civic_and_property_views_respect_membership_and_personal_access() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/civic.toml")).unwrap();
    let view = ruler(&world).unwrap();
    assert_eq!(view.institutions.len(), 1);
    assert_eq!(view.institutions[0].body.id, 1);
    assert!(view.property_claims.is_empty());
    world.property.as_mut().unwrap().claims[0].delegates.push(1);
    let view = ruler(&world).unwrap();
    assert_eq!(view.property_claims[0].owner, 2);
    let person = world.regions[0].people.remove(0);
    world.travelers.push(game_parallel::Traveler {
        person,
        source: 1,
        destination: 2,
        ticks_remaining: 1,
    });
    let view = ruler(&world).unwrap();
    assert!(view.institutions.is_empty());
    assert_eq!(view.property_claims.len(), 1);
}

#[test]
fn own_skill_rate_includes_lessons_without_exposing_other_people() {
    let mut world =
        World::from_toml(include_str!("../../../../content/parallel/teaching.toml")).unwrap();
    world
        .skills
        .as_mut()
        .unwrap()
        .lessons
        .push(game_parallel::SkillLesson {
            teacher: 3,
            student: 1,
            recipe: 1,
            tick: 0,
            source_life: None,
        });
    let view = ruler(&world).unwrap();
    assert_eq!(view.skill_rates.len(), 1);
    assert_eq!(
        (view.skill_rates[0].recipe, view.skill_rates[0].rate),
        (1, 2)
    );
    assert_eq!(view.lessons.len(), 1);
}
