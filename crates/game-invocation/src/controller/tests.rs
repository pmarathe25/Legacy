use super::*;
use crate::*;
use std::collections::BTreeSet;
struct Fixture {
    personality: Personality,
    goal: Goal,
    dream: Aspiration,
    past: BTreeMap<InventoryId, StockKnowledge>,
    current: BTreeMap<InventoryId, StockKnowledge>,
    past_sites: BTreeMap<SiteId, SiteKnowledge>,
    current_sites: BTreeMap<SiteId, SiteKnowledge>,
    relationships: BTreeMap<PersonId, Relationship>,
    children: BTreeMap<PersonId, ChildSignal>,
    people: Vec<ControllerPerson>,
    routes: RouteMemory,
}
impl Fixture {
    fn new() -> Self {
        Self {
            personality: Personality::industrious(),
            goal: Goal::Practice,
            dream: Aspiration::Independence,
            past: BTreeMap::new(),
            current: BTreeMap::new(),
            past_sites: BTreeMap::new(),
            current_sites: BTreeMap::new(),
            relationships: BTreeMap::new(),
            children: BTreeMap::new(),
            people: Vec::new(),
            routes: BTreeMap::new(),
        }
    }
    fn input(&self) -> ControllerInput<'_, Box<dyn Iterator<Item = ControllerPerson> + '_>> {
        ControllerInput {
            actor: PersonId(42),
            changed: false,
            in_combat: false,
            independent: true,
            area: Some(AreaId(1)),
            activity: ControllerActivity::Other,
            hunger: 0,
            meal_relief: 120000,
            fatigue: 0,
            fatigue_limit: 20000,
            personality: &self.personality,
            past: ControllerMemory {
                goal: self.goal,
                dream: self.dream,
                stocks: &self.past,
                sites: &self.past_sites,
                relationships: &self.relationships,
            },
            current_stocks: &self.current,
            current_sites: &self.current_sites,
            child_signals: &self.children,
            local_people: Box::new(self.people.iter().copied()),
            current_routes: &self.routes,
            historical_routes: &[],
        }
    }
}
fn stock(id: u64, area: u64, good: Good, amount: u64) -> StockKnowledge {
    StockKnowledge {
        inventory: InventoryId(id),
        area: AreaId(area),
        good,
        quantity: Quantity(amount),
        accessible: true,
        observed_at: SimTime(0),
        learned_at: SimTime(0),
        source: KnowledgeSource::Observed(PersonId(42)),
    }
}
fn act(action: ControllerAction) -> ControllerDecision {
    ControllerDecision::Act {
        action,
        ready: true,
    }
}
#[test]
fn current_memory_overrides_same_past_id_and_equal_food_cost_uses_id_order() {
    let mut f = Fixture::new();
    f.past.insert(InventoryId(3), stock(3, 1, Good::Food, 1000));
    f.current.insert(InventoryId(3), stock(3, 1, Good::Food, 0));
    let mut input = f.input();
    input.hunger = 120000;
    assert_eq!(choose_controller_action(input), act(ControllerAction::Rest));
    f.past.insert(InventoryId(9), stock(9, 1, Good::Food, 1000));
    f.past.insert(InventoryId(4), stock(4, 1, Good::Food, 1000));
    let mut input = f.input();
    input.hunger = 120000;
    assert_eq!(
        choose_controller_action(input),
        act(ControllerAction::Eat {
            source: InventoryId(4)
        })
    );
}
#[test]
fn physical_control_and_protected_actions_precede_any_local_population_scan() {
    let f = Fixture::new();
    for (combat, independent) in [(true, true), (false, false)] {
        let mut input = f.input();
        input.in_combat = combat;
        input.independent = independent;
        input.local_people = Box::new(std::iter::from_fn(|| panic!("no local enumeration")));
        assert_eq!(choose_controller_action(input), ControllerDecision::Keep);
    }
    for activity in [
        ControllerActivity::Eat,
        ControllerActivity::Load,
        ControllerActivity::Care,
        ControllerActivity::Tell,
        ControllerActivity::Lesson,
        ControllerActivity::Memory,
    ] {
        let mut input = f.input();
        input.activity = activity;
        assert_eq!(choose_controller_action(input), ControllerDecision::Keep);
        let mut input = f.input();
        input.activity = activity;
        input.changed = true;
        assert_eq!(choose_controller_action(input), act(ControllerAction::Rest));
    }
    let mut f = Fixture::new();
    f.dream = Aspiration::Comfort;
    let mut input = f.input();
    input.area = None;
    input.changed = true;
    let decision = choose_controller_action(input);
    assert_eq!(decision, act(ControllerAction::Cancel));
    assert_eq!(decision.after_failure(), None);
}
#[test]
fn priority_is_hunger_then_rest_then_hostility_then_care_then_work() {
    let mut f = Fixture::new();
    f.past.insert(InventoryId(1), stock(1, 1, Good::Food, 1000));
    f.personality.aggression = 800;
    f.personality.risk_tolerance = 600;
    f.personality.empathy = 1000;
    f.dream = Aspiration::SupportOthers;
    f.relationships.insert(
        PersonId(2),
        Relationship {
            trust: -1,
            affection: 0,
        },
    );
    f.people.push(ControllerPerson {
        alive: true,
        person: PersonId(2),
        contact: true,
    });
    f.children.insert(
        PersonId(3),
        ChildSignal {
            preference: None,
            location: Location::AtArea(AreaId(1)),
            urgency: 1,
            hydration_urgency: 0,
            observed_at: SimTime(0),
        },
    );
    let mut input = f.input();
    input.hunger = 120000;
    input.fatigue = 20000;
    assert_eq!(
        choose_controller_action(input),
        act(ControllerAction::Eat {
            source: InventoryId(1)
        })
    );
    let mut input = f.input();
    input.fatigue = 10001;
    assert_eq!(choose_controller_action(input), act(ControllerAction::Rest));
    assert_eq!(
        choose_controller_action(f.input()),
        act(ControllerAction::Engage {
            target: PersonId(2)
        })
    );
    f.personality.aggression = 799;
    assert_eq!(
        choose_controller_action(f.input()),
        act(ControllerAction::Care {
            child: PersonId(3),
            source: InventoryId(1)
        })
    );
    f.personality.empathy = 499;
    assert_eq!(
        choose_controller_action(f.input()),
        act(ControllerAction::Rest)
    );
}
#[test]
fn the_first_hostile_intent_keeps_its_preference_when_physical_contact_fails() {
    let mut f = Fixture::new();
    f.personality.aggression = 800;
    f.personality.risk_tolerance = 600;
    for actor in [42, 2, 3] {
        f.relationships.insert(
            PersonId(actor),
            Relationship {
                trust: -1,
                affection: 0,
            },
        );
    }
    f.people = vec![
        ControllerPerson {
            alive: true,
            person: PersonId(42),
            contact: true,
        },
        ControllerPerson {
            alive: true,
            person: PersonId(2),
            contact: false,
        },
        ControllerPerson {
            alive: true,
            person: PersonId(3),
            contact: true,
        },
    ];
    let decision = choose_controller_action(f.input());
    assert_eq!(
        decision,
        ControllerDecision::Act {
            action: ControllerAction::Engage {
                target: PersonId(2)
            },
            ready: false
        }
    );
    assert_eq!(decision.after_failure(), Some(ControllerAction::Rest));
}
#[test]
fn work_uses_current_site_and_stock_rights_and_suppresses_unchanged_same_site() {
    let mut f = Fixture::new();
    f.past
        .insert(InventoryId(1), stock(1, 1, Good::Timber, 1000));
    f.past.insert(InventoryId(2), stock(2, 1, Good::Tool, 1000));
    let site = SiteKnowledge {
        site: SiteId(5),
        area: AreaId(1),
        input: InventoryId(1),
        tool: InventoryId(2),
        input_quantity: Quantity(1000),
        output: InventoryId(3),
        access: true,
    };
    f.past_sites.insert(site.site, site.clone());
    let mut input = f.input();
    input.activity = ControllerActivity::Work { site: SiteId(5) };
    assert_eq!(choose_controller_action(input), ControllerDecision::Keep);
    let mut input = f.input();
    input.activity = ControllerActivity::Work { site: SiteId(5) };
    input.changed = true;
    assert_eq!(
        choose_controller_action(input),
        act(ControllerAction::Work { site: SiteId(5) })
    );
    f.current_sites.insert(
        site.site,
        SiteKnowledge {
            access: false,
            ..site
        },
    );
    assert_eq!(
        choose_controller_action(f.input()),
        act(ControllerAction::Rest)
    );
}
#[test]
fn current_and_earlier_historical_routes_win_and_burden_changes_traits_exactly() {
    let current = BTreeMap::from([(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(10))]))]);
    let early = BTreeMap::from([(
        AreaId(1),
        BTreeMap::from([(AreaId(2), SimDuration(99)), (AreaId(3), SimDuration(20))]),
    )]);
    let late = BTreeMap::from([(AreaId(1), BTreeMap::from([(AreaId(3), SimDuration(99))]))]);
    let merged = recalled_routes(&current, [&early, &late]);
    assert_eq!(merged[&AreaId(1)][&AreaId(2)], SimDuration(10));
    assert_eq!(merged[&AreaId(1)][&AreaId(3)], SimDuration(20));
    let traits = controller_personality(
        &Personality::industrious(),
        &Burden {
            temporary_identity: 10,
            permanent_identity: 20,
            emotional_disruption: 1000,
            ..Burden::default()
        },
    );
    assert_eq!(
        (traits.discipline, traits.aggression, traits.patience),
        (750, 1000, 0)
    );
}
#[test]
fn accepted_control_rotates_lives_and_drives_character_memory_without_the_assembly() {
    let actor = PersonId(42);
    let lives = BTreeSet::from([PersonId(1), PersonId(2)]);
    let control = ControlState::new(Rules::fixture()).unwrap();
    let mut schema = game_ecs::Schema::new();
    register_components(&mut schema).unwrap();
    let mut store = game_ecs::Store::with_schema(schema);
    let mut facts = AdmissionFacts {
        actor,
        now: SimTime(0),
        current: Some(actor),
        alive: true,
        health: 100000,
        independent: true,
        learning: 100,
        required_learning: 100,
        fatigue: 0,
        fatigue_limit: 100000,
        archived: lives.clone(),
    };
    control
        .prepare_action(
            &store,
            actor,
            &ControlAction::Activate {
                lives: lives.clone(),
            },
            &facts,
            Some(WarningId(1)),
        )
        .unwrap()
        .commit(&control, &mut store)
        .unwrap();
    let mut warning_id = 2;
    while let Some(warning) = control.pending_consent(&store).cloned() {
        control
            .prepare_action(
                &store,
                actor,
                &ControlAction::AcceptWarning {
                    warning: warning.id,
                },
                &facts,
                Some(WarningId(warning_id)),
            )
            .unwrap()
            .commit(&control, &mut store)
            .unwrap();
        warning_id += 1;
    }
    burden_mut(&mut store, actor).unwrap().burden = control.rules.loss_threshold;
    control
        .prepare_review(&store, &facts, Some(WarningId(warning_id)))
        .unwrap()
        .commit(&control, &mut store)
        .unwrap();
    warning_id += 1;
    while let Some(warning) = control.pending_consent(&store).cloned() {
        control
            .prepare_action(
                &store,
                actor,
                &ControlAction::AcceptWarning {
                    warning: warning.id,
                },
                &facts,
                Some(WarningId(warning_id)),
            )
            .unwrap()
            .commit(&control, &mut store)
            .unwrap();
        warning_id += 1;
    }
    let mut survivor = Fixture::new();
    survivor.goal = Goal::Survival;
    survivor
        .past
        .insert(InventoryId(1), stock(1, 2, Good::Food, 1000));
    survivor
        .routes
        .insert(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(100))]));
    let mut comfortable = Fixture::new();
    comfortable.dream = Aspiration::Comfort;
    for (tick, expected_life, expected_action) in [
        (
            0,
            PersonId(1),
            ControllerAction::Move {
                destination: AreaId(2),
            },
        ),
        (1, PersonId(2), ControllerAction::Rest),
    ] {
        facts.now = SimTime(tick * control.rules.controller_quantum.0);
        let prepared = control
            .prepare_controller(&store, facts.now, true)
            .unwrap()
            .unwrap();
        let changed = prepared
            .effects
            .iter()
            .find_map(|effect| match effect {
                ControlEffect::ControllerSelected { changed, .. } => Some(*changed),
                _ => None,
            })
            .unwrap();
        prepared.commit(&control, &mut store).unwrap();
        let chosen = control.active(&store).unwrap().dominant.unwrap();
        assert_eq!(chosen, expected_life);
        let memory = if chosen == PersonId(1) {
            &survivor
        } else {
            &comfortable
        };
        let mut input = memory.input();
        input.changed = changed;
        input.hunger = 40000;
        assert_eq!(choose_controller_action(input), act(expected_action));
    }
}

#[test]
fn canonical_controller_entry_joins_private_archives_and_restores_identical_turns() {
    use crate::memory_start::tests::{ACTOR, LIFE, fixture};
    let (runtime, mut whole, memory) = fixture();
    let second = PersonId(5);
    let mut comfortable = archive::<u8>(&whole, LIFE).unwrap().clone();
    comfortable.person = second;
    comfortable.mind.dream = Aspiration::Comfort;
    whole
        .insert_or_spawn(game_ecs::StableId(second.0), comfortable)
        .unwrap();
    let active = session_mut(&mut whole, ACTOR)
        .unwrap()
        .active
        .as_mut()
        .unwrap();
    active.lives.insert(second);
    active.episode = Some(Episode {
        actor: ACTOR,
        starts: SimTime(0),
        ends: SimTime(100_000),
    });
    let mut restored =
        game_ecs::Store::restore(whole.schema().clone(), whole.snapshot().unwrap()).unwrap();
    let facts = ControllerFacts {
        in_combat: false,
        independent: true,
        area: Some(AreaId(3)),
        activity: ControllerActivity::Other,
        hunger: 120_000,
        meal_relief: 120_000,
        fatigue: 0,
        fatigue_limit: 100_000,
    };
    for tick in 0..3 {
        let now = SimTime(tick * runtime.control.rules.controller_quantum.0);
        for store in [&mut whole, &mut restored] {
            let opening = store.snapshot().unwrap();
            let prepared = runtime
                .control
                .prepare_controller(store, now, true)
                .unwrap()
                .unwrap();
            let decision = runtime
                .controller_decision::<u8>(store, &prepared, facts, [])
                .unwrap();
            // Current stocks are empty; only the selected archive remembers this food.
            assert_eq!(
                decision,
                act(ControllerAction::Eat {
                    source: InventoryId(2)
                })
            );
            let admission = AdmissionFacts {
                actor: ACTOR,
                now,
                current: Some(ACTOR),
                alive: true,
                health: 100_000,
                independent: true,
                learning: 100,
                required_learning: 100,
                fatigue: 0,
                fatigue_limit: 100_000,
                archived: BTreeSet::from([LIFE, second]),
            };
            for _ in 0..3 {
                let view = runtime
                    .state_view::<u8>(store, &admission, &memory)
                    .unwrap();
                assert!(!view.player_control);
                assert_eq!(view.active_lives, admission.archived);
            }
            assert_eq!(store.snapshot().unwrap(), opening);
            prepared.commit(&runtime.control, store).unwrap();
            let subject = runtime.controller_mind::<u8>(store, ACTOR).unwrap();
            assert_eq!(
                subject.dream,
                if tick % 2 == 0 {
                    Aspiration::Independence
                } else {
                    Aspiration::Comfort
                }
            );
        }
        assert_eq!(whole, restored);
    }
    let prepared = runtime
        .control
        .prepare_controller(&whole, SimTime(90_000), true)
        .unwrap()
        .unwrap();
    session_mut(&mut whole, ACTOR)
        .unwrap()
        .active
        .as_mut()
        .unwrap()
        .next_controller = SimTime(100_000);
    let before = whole.clone();
    assert!(
        runtime
            .controller_decision::<u8>(&whole, &prepared, facts, [])
            .is_err()
    );
    assert_eq!(whole, before);
}

#[test]
fn prepared_controller_review_guards_real_body_and_selected_memory_before_publication() {
    use crate::memory_start::tests::{ACTOR, LIFE, fixture};
    use game_ecs::{Batch, ReadSet, StableId, Store};
    struct Source<'a>(&'a Store);
    impl ControllerFactsSource for Source<'_> {
        fn facts(&self, actor: PersonId) -> Result<ControllerFacts, Error> {
            let body = game_physiology::body(self.0, actor).ok_or(Error::Unavailable)?;
            Ok(ControllerFacts {
                in_combat: false,
                independent: true,
                area: Some(AreaId(3)),
                activity: ControllerActivity::Other,
                hunger: body.vitals.hunger,
                meal_relief: 120_000,
                fatigue: body.vitals.fatigue,
                fatigue_limit: 100_000,
            })
        }
        fn protect(&self, store: &Store, actor: PersonId, reads: &mut ReadSet) {
            reads.read::<game_physiology::Body>(store, StableId(actor.0));
        }
    }
    let (runtime, mut store, _) = fixture();
    game_physiology::insert_body(
        &mut store,
        ACTOR,
        game_physiology::Body {
            vitals: Vitals {
                hunger: 120_000,
                fatigue: 0,
                health: 100_000,
                alive: true,
            },
            fatigue_remainder: 0,
            health_remainder: 0,
        },
    )
    .unwrap();
    session_mut(&mut store, ACTOR)
        .unwrap()
        .active
        .as_mut()
        .unwrap()
        .episode = Some(Episode {
        actor: ACTOR,
        starts: SimTime(0),
        ends: SimTime(100_000),
    });
    for attack in 0..4 {
        let control = runtime
            .control
            .prepare_controller(&store, SimTime(0), true)
            .unwrap()
            .unwrap();
        let review = runtime
            .prepare_controller_review::<u8>(&store, control, &Source(&store), [])
            .unwrap();
        assert_eq!(
            review.decision(),
            act(ControllerAction::Eat {
                source: InventoryId(2)
            })
        );
        let before = store.clone();
        let mut next_id = 100;
        let mut published = vec![];
        let mut batch = Batch::new();
        let receipt = review
            .stage_into(&runtime.control, &store, &Source(&store), &mut batch)
            .unwrap();
        match attack {
            0 => {
                batch.remove::<game_physiology::Body>(StableId(ACTOR.0));
            }
            1 => {
                batch.remove::<game_characters::decisions::Stocks>(StableId(ACTOR.0));
            }
            2 => {
                batch.remove::<PastLife<u8>>(StableId(LIFE.0));
            }
            _ => {
                batch.insert(
                    StableId(ACTOR.0),
                    game_characters::decisions::mind::<u8>(&store, ACTOR)
                        .unwrap()
                        .clone(),
                );
            }
        }
        let result = (|| -> Result<(), Error> {
            receipt.validate_batch(&runtime.control, &store, &batch)?;
            store.apply_batch(batch)?;
            next_id += 1;
            published.push(receipt.decision);
            Ok(())
        })();
        assert!(result.is_err());
        assert_eq!(store, before);
        assert_eq!(next_id, 100);
        assert!(published.is_empty());
    }
    let mut restored = Store::restore(store.schema().clone(), store.snapshot().unwrap()).unwrap();
    for candidate in [&mut store, &mut restored] {
        let control = runtime
            .control
            .prepare_controller(candidate, SimTime(0), true)
            .unwrap()
            .unwrap();
        let review = runtime
            .prepare_controller_review::<u8>(candidate, control, &Source(candidate), [])
            .unwrap();
        let mut batch = Batch::new();
        let receipt = review
            .stage_into(&runtime.control, candidate, &Source(candidate), &mut batch)
            .unwrap();
        receipt
            .validate_batch(&runtime.control, candidate, &batch)
            .unwrap();
        candidate.apply_batch(batch).unwrap();
        assert_eq!(
            runtime.control.active(candidate).unwrap().dominant,
            Some(LIFE)
        );
    }
    assert_eq!(store, restored);
}

#[test]
fn invocation_filters_dead_local_bodies_before_selecting_the_archived_hostility() {
    let mut f = Fixture::new();
    f.personality.aggression = 800;
    f.personality.risk_tolerance = 600;
    for actor in [PersonId(2), PersonId(3)] {
        f.relationships.insert(
            actor,
            Relationship {
                trust: -1,
                affection: 0,
            },
        );
    }
    f.people = vec![
        ControllerPerson {
            person: PersonId(2),
            alive: false,
            contact: true,
        },
        ControllerPerson {
            person: PersonId(3),
            alive: true,
            contact: true,
        },
    ];
    assert_eq!(
        choose_controller_action(f.input()),
        act(ControllerAction::Engage {
            target: PersonId(3)
        })
    );
}
