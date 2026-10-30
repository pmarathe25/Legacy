use super::{load, save, scenario};
use std::path::PathBuf;

#[test]
fn authored_scenario_advances_and_resumes_identically_across_worker_counts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let first = world.advance().unwrap();
                assert_eq!(first.started.len(), 1);
                assert_eq!(world.snapshot().goals.as_ref().unwrap().people.len(), 1);
                let state = world.snapshot();
                let field = &state.agriculture.as_ref().unwrap().fields[0];
                assert_eq!(field.phase, game_parallel_economy::CropPhase::Growing);
                assert_eq!(field.remaining, 24);
                let seed = state
                    .physical
                    .goods
                    .iter()
                    .find(|g| g.name == "grain-seed")
                    .unwrap()
                    .id;
                let stock = state
                    .physical
                    .custody
                    .containers
                    .iter()
                    .find(|row| {
                        row.place
                            == game_parallel_lots::Place::Stock {
                                region: 1,
                                good: seed,
                            }
                    })
                    .unwrap();
                assert_eq!(stock.used, 20);
                let path = std::env::temp_dir().join(format!(
                    "legacy-economy-{}-{workers}.json",
                    std::process::id()
                ));
                save(&world, &path).unwrap();
                let mut restored = load(&path).unwrap();
                std::fs::remove_file(path).unwrap();
                for _ in 0..4 {
                    world.advance().unwrap();
                    restored.advance().unwrap();
                }
                assert_eq!(world.snapshot(), restored.snapshot());
                world.snapshot()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn authored_campaign_starts_a_controlled_ruler_and_a_real_family() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut world = scenario(
                    &root.join("content/parallel/economy-campaign.toml"),
                    &root.join("content"),
                )
                .unwrap();
                let state = world.snapshot();
                assert!(state.people.iter().find(|p| p.id == 1).unwrap().controlled);
                assert_eq!(state.campaign.as_ref().unwrap().lineage.current, Some(1));
                for _ in 0..3 {
                    world.advance().unwrap();
                }
                let saved = world.snapshot();
                assert_eq!(saved.family.as_ref().unwrap().totals.births, 1);
                assert!(
                    saved
                        .food_search
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == 3 && p.enabled)
                );
                assert!(
                    saved
                        .gathering
                        .as_ref()
                        .unwrap()
                        .policies
                        .iter()
                        .any(|p| p.person == 3 && p.pressured_by.is_none())
                );
                let newborn_purpose = saved
                    .purpose
                    .as_ref()
                    .unwrap()
                    .people
                    .iter()
                    .find(|p| p.person == 3)
                    .unwrap();
                assert_eq!(newborn_purpose.deprivation, 0);
                assert!(newborn_purpose.eligible_at > saved.physical.custody.tick);
                assert!(
                    saved
                        .dynamics
                        .as_ref()
                        .unwrap()
                        .personality
                        .as_ref()
                        .unwrap()
                        .people
                        .iter()
                        .any(|p| p.person == 3)
                );
                assert!(
                    saved
                        .knowledge
                        .as_ref()
                        .unwrap()
                        .minds
                        .iter()
                        .any(|mind| mind.person == 3)
                );
                assert!(
                    saved
                        .physical
                        .custody
                        .containers
                        .iter()
                        .any(|c| c.place == game_parallel_lots::Place::Person(3))
                );
                game_parallel_economy::Economy::compile(saved.clone()).unwrap();
                saved
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn ruler_view_excludes_remote_people_stock_and_other_minds() {
    use game_parallel_economy_knowledge::Record;
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut baseline = world.snapshot();
    baseline
        .people
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .region = Some(2);
    let mut hidden = baseline.clone();
    hidden.giving.as_mut().unwrap().learning[0].experiences = 2;
    hidden
        .opinions
        .as_mut()
        .unwrap()
        .records
        .push(game_parallel_economy::Opinion {
            observer: 2,
            target: 1,
            trust: 0,
            fear: 0,
            obligation: 0,
            affinity: 8,
        });
    hidden
        .knowledge
        .as_mut()
        .unwrap()
        .minds
        .iter_mut()
        .find(|m| m.person == 2)
        .unwrap()
        .records
        .push(Record {
            region: 2,
            food: 999,
            observed_at: 0,
            witness: 2,
            learned_from: None,
        });
    let stock = hidden
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| matches!(c.place, Place::Stock { region: 2, .. }))
        .unwrap();
    let good = match stock.place {
        Place::Stock { good, .. } => good,
        _ => unreachable!(),
    };
    *stock = Container::new(
        stock.place,
        stock.capacity,
        vec![Lot {
            good,
            quantity: 7,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut first = game_parallel_economy::Economy::compile(baseline).unwrap();
    let mut second = game_parallel_economy::Economy::compile(hidden).unwrap();
    let mut player = Player::new(&mut first);
    assert_eq!(
        player.view().unwrap(),
        Player::new(&mut second).view().unwrap()
    );
    assert_eq!(player.view().unwrap().person.unwrap().id, 1);
    assert!(player.view().unwrap().nearby.is_empty());
    let conditions = player.view().unwrap().conditions.unwrap();
    assert_eq!(conditions.season.unwrap().name, "growing season");
    assert_eq!(conditions.local_hazard.unwrap().region, 1);
    assert!(player.past_knowledge(2).is_err());
    assert!(player.meet(2).is_err());
    assert_eq!(player.view().unwrap().tick, 0);
}

#[test]
fn suspended_control_hides_the_rulers_view_and_rejects_actions() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .campaign
        .as_mut()
        .unwrap()
        .lineage
        .invocation
        .as_mut()
        .unwrap()
        .control_until = Some(2);
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let view = player.view().unwrap();
    assert!(view.person.is_none());
    assert!(
        view.nearby.is_empty()
            && view.possessions.is_empty()
            && view.local_stock.is_empty()
            && view.food_reports.is_empty()
    );
    assert!(player.eat(31).is_err());
    assert!(player.start_expedition(2, 1).is_err());
    player.advance().unwrap();
    player.advance().unwrap();
    assert_eq!(player.view().unwrap().person.unwrap().id, 1);
}

#[test]
fn player_can_make_a_promise_and_later_choose_real_delivery() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let good = player.view().unwrap().possessions[0].good;
    let id = player.promise(2, good, 1, 8).unwrap();
    player.advance().unwrap();
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_parallel_agreements::Status::Active
    );
    assert!(player.fulfill_promise(id).unwrap());
    assert_eq!(
        player.view().unwrap().promises[0].status,
        game_parallel_agreements::Status::Fulfilled
    );
}

#[test]
fn player_fights_on_the_authored_generated_map() {
    use game_parallel_economy_combat::Order;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let id = player.attack(2).unwrap();
    assert_eq!(player.view().unwrap().battle.unwrap().encounter, id);
    assert!(player.rest().is_err());
    player.advance().unwrap();
    assert!(player.view().unwrap().person.unwrap().health < 100);
    player.combat_order(Order::Retreat).unwrap();
    player.advance().unwrap();
    assert!(player.view().unwrap().battle.is_none());
}

#[test]
fn player_request_can_be_refused_or_followed_from_personal_fear() {
    use game_parallel_economy_combat::Order;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut initial = world.snapshot();
    for member in &mut initial.family.as_mut().unwrap().members {
        member.pregnancy = None;
        member.reproduction_allowed = false;
    }
    let mut world = game_parallel_economy::Economy::compile(initial).unwrap();
    let mut player = Player::new(&mut world);
    let recipe = player
        .catalog()
        .unwrap()
        .1
        .into_iter()
        .find(|r| r.name == "fixture-tool")
        .unwrap()
        .id;
    let first = player.ask_work(2, recipe, 15).unwrap();
    assert_eq!(player.view().unwrap().pending_work_requests[0].id, first);
    for _ in 0..8 {
        player.advance().unwrap();
        if player
            .view()
            .unwrap()
            .work_memories
            .iter()
            .any(|m| m.worker == 2 && m.failures == 1)
        {
            break;
        }
    }
    let view = player.view().unwrap();
    assert!(
        view.work_memories
            .iter()
            .any(|m| m.worker == 2 && m.failures == 1),
        "{view:?}"
    );
    player.attack(2).unwrap();
    player.advance().unwrap();
    player.combat_order(Order::Retreat).unwrap();
    player.advance().unwrap();
    let second = player.ask_work(2, recipe, 15).unwrap();
    let mut accepted = false;
    for _ in 0..10 {
        player.advance().unwrap();
        if player
            .view()
            .unwrap()
            .observed_work
            .iter()
            .any(|work| work.request == second && work.worker == 2)
        {
            accepted = true;
            break;
        }
    }
    let view = player.view().unwrap();
    assert!(accepted, "{view:?}");
}

#[test]
fn player_treats_infection_with_authored_medicine_taken_from_stock() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 1,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let (medicine, quantity) = player.view().unwrap().infection.unwrap().medicine.unwrap();
    assert!(player.transfer_stock(medicine, quantity, true).unwrap());
    assert!(player.treat().unwrap());
    assert!(player.view().unwrap().infection.is_none());
}

#[test]
fn player_can_treat_a_nearby_person_with_carried_medicine() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .dynamics
        .as_mut()
        .unwrap()
        .disease
        .as_mut()
        .unwrap()
        .cases
        .push(game_parallel_disease::Case {
            person: 2,
            strain: 1,
            infected_at: 0,
            source: None,
        });
    let medicine = state
        .physical
        .goods
        .iter()
        .find(|g| g.name == "medicine")
        .unwrap()
        .id;
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    assert!(player.transfer_stock(medicine, 1, true).unwrap());
    assert!(player.treat_other(2, medicine, 1).unwrap());
    assert!(
        !world
            .snapshot()
            .dynamics
            .unwrap()
            .disease
            .unwrap()
            .cases
            .iter()
            .any(|c| c.person == 2)
    );
}

#[test]
fn player_can_cultivate_a_visible_field() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let field = player.view().unwrap().fields[0].id;
    assert!(player.farm(field).unwrap());
    assert_eq!(
        player.view().unwrap().fields[0].phase,
        game_parallel_economy::CropPhase::Growing
    );
}

#[test]
fn player_rest_advances_time_and_recovers_personal_fatigue() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = world.snapshot();
    state
        .fatigue
        .as_mut()
        .unwrap()
        .people
        .push(game_parallel_fatigue::Person {
            id: 1,
            fatigue: 10,
            resting: false,
        });
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    player.rest().unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.fatigue, Some((6, true)));
}

#[test]
fn player_observes_and_recovers_local_deceased_possessions() {
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Lot, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut state = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap()
    .snapshot();
    state.people.iter_mut().find(|p| p.id == 2).unwrap().health = 0;
    state
        .family
        .as_mut()
        .unwrap()
        .members
        .iter_mut()
        .find(|p| p.id == 2)
        .unwrap()
        .alive = false;
    let good = state.physical.goods[0].id;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(
        Place::Person(2),
        holder.capacity,
        vec![Lot {
            good,
            quantity: 1,
            expires_at: None,
        }],
    )
    .unwrap();
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    assert!(
        player
            .view()
            .unwrap()
            .bodies
            .iter()
            .any(|body| body.person == 2 && body.possessions.iter().any(|lot| lot.good == good))
    );
    assert!(player.recover_possessions(2, good, 1).unwrap());
    assert!(
        player
            .view()
            .unwrap()
            .possessions
            .iter()
            .any(|lot| lot.good == good)
    );
}

#[test]
fn player_gathers_from_an_authored_local_resource_site() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let site = player
        .view()
        .unwrap()
        .resource_sites
        .into_iter()
        .find(|site| site.id == 1)
        .unwrap();
    assert_eq!(site.available, 20);
    assert!(player.gather(1).unwrap());
    assert_eq!(
        player
            .view()
            .unwrap()
            .resource_sites
            .into_iter()
            .find(|site| site.id == 1)
            .unwrap()
            .available,
        18
    );
    assert!(player.gather(2).is_err());
}

#[test]
fn player_sees_only_their_own_actual_journey() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player.depart(2).unwrap();
    let view = player.view().unwrap();
    let journey = view.journey.unwrap();
    assert_eq!(
        (
            journey.person,
            journey.source,
            journey.destination,
            journey.remaining
        ),
        (1, 1, 2, 3)
    );
    assert!(view.nearby.is_empty());
    assert!(view.resource_sites.is_empty());
}

#[test]
fn player_can_give_carried_goods_without_creating_a_promise() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let before = player.view().unwrap();
    let good = before.possessions[0].good;
    assert!(player.give(2, good, 1).unwrap());
    let after = player.view().unwrap();
    assert_eq!(
        after.possessions[0].quantity,
        before.possessions[0].quantity - 1
    );
    assert!(after.promises.is_empty());
}

#[test]
fn player_can_ask_a_local_person_for_food_and_see_the_request() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    player.ask_food(2, 4).unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.tick, 1);
    assert_eq!(view.food_requests.len(), 1);
    assert_eq!(
        (
            view.food_requests[0].requester,
            view.food_requests[0].target
        ),
        (1, 2)
    );
    assert!(player.ask_food(99, 4).is_err());
    assert_eq!(player.view().unwrap().tick, 1);
}

#[test]
fn player_can_request_a_personal_hauling_effort() {
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut player = Player::new(&mut world);
    let bread = player
        .catalog()
        .unwrap()
        .0
        .into_iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    player.ask_haul(2, 2, 1, bread, 8).unwrap();
    let view = player.view().unwrap();
    assert_eq!(view.hauling_requests.len(), 1);
    assert_eq!(
        (
            view.hauling_requests[0].requester,
            view.hauling_requests[0].carrier
        ),
        (1, 2)
    );
    assert_eq!(view.tick, 1);
}

#[test]
fn player_request_can_lead_to_a_real_cross_region_delivery() {
    use game_parallel_economy_player::Player;
    use game_parallel_lots::{Container, Place};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let original = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    let mut state = original.snapshot();
    state.autowork = None;
    state.goals = None;
    state.dreams = None;
    state.agriculture = None;
    state.gathering = None;
    state.people[1].seek_meals = false;
    let holder = state
        .physical
        .custody
        .containers
        .iter_mut()
        .find(|c| c.place == Place::Person(2))
        .unwrap();
    *holder = Container::new(holder.place, holder.capacity, vec![]).unwrap();
    let mut world = game_parallel_economy::Economy::compile(state).unwrap();
    let mut player = Player::new(&mut world);
    let bread = player
        .catalog()
        .unwrap()
        .0
        .into_iter()
        .find(|good| good.name == "bread")
        .unwrap()
        .id;
    player.ask_haul(2, 2, 1, bread, 20).unwrap();
    for _ in 0..15 {
        player.advance().unwrap();
    }
    let view = player.view().unwrap();
    assert!(
        view.local_stock
            .iter()
            .any(|lot| lot.good == bread && lot.quantity >= 2)
    );
}

#[test]
fn player_can_see_and_give_to_a_companion_on_the_same_journey() {
    use game_parallel_economy::Departure;
    use game_parallel_economy_player::Player;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut world = scenario(
        &root.join("content/parallel/economy-campaign.toml"),
        &root.join("content"),
    )
    .unwrap();
    world
        .depart(&[
            Departure {
                person: 1,
                destination: 2,
            },
            Departure {
                person: 2,
                destination: 2,
            },
        ])
        .unwrap();
    let mut player = Player::new(&mut world);
    let view = player.view().unwrap();
    assert_eq!(view.nearby, vec![2]);
    assert!(player.give(2, view.possessions[0].good, 1).unwrap());
    assert_eq!(player.view().unwrap().journey.unwrap().remaining, 2);
}

#[test]
fn scenario_shelter_goods_resolve_from_content_names() {
    use crate::{EnvironmentConfig, resolve_environment};
    use std::collections::BTreeMap;
    let parse = |good: &str| {
        toml::from_str::<EnvironmentConfig>(&format!("regions = [{{ region = 1, damage = 2, shelter = 0, shelter_reduction = 1, shelter_good = \"{good}\" }}]")).unwrap()
    };
    let goods = BTreeMap::from([("reed-shelter-kit", 7)]);
    assert_eq!(
        resolve_environment(parse("reed-shelter-kit"), &goods)
            .unwrap()
            .regions[0]
            .shelter_good,
        Some(7)
    );
    assert!(resolve_environment(parse("missing"), &goods).is_err());
}

#[test]
fn authored_personal_promise_offers_resolve_named_goods() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let world = scenario(
        &root.join("content/parallel/economy.toml"),
        &root.join("content"),
    )
    .unwrap();
    let state = world.snapshot();
    let offer = &state.agreements.as_ref().unwrap().offering[0];
    let good = state
        .physical
        .goods
        .iter()
        .find(|good| good.id == offer.good)
        .unwrap();
    assert_eq!(
        (offer.person, offer.recipient, good.name.as_str()),
        (2, 1, "bread")
    );
    assert!(!offer.willing);
}

#[test]
fn authored_personal_offers_resolve_observed_work_by_recipe_name() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("content/parallel/economy.toml")).unwrap();
    let configured = source.replacen(
        "[[promise_offering]]",
        "[[promise_offering]]\nafter_work_recipe = \"fixture-tool\"",
        1,
    );
    let path =
        std::env::temp_dir().join(format!("legacy-observed-work-{}.toml", std::process::id()));
    std::fs::write(&path, configured).unwrap();
    let world = scenario(&path, &root.join("content")).unwrap();
    std::fs::remove_file(path).unwrap();
    let state = world.snapshot();
    let id = state.agreements.unwrap().offering[0]
        .after_work_recipe
        .unwrap();
    assert_eq!(
        state
            .physical
            .recipes
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .name,
        "fixture-tool"
    );
}
