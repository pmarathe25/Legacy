//! Read-only personal affordances and visibility over typed factual bindings.
//! Fixture navigation explicitly uses the authored catalog; live knowledge uses
//! only personal remembered edges. Queries never update owned components.
use crate::decisions::known_path;
use crate::people::persons;
use game_core::*;
use game_ecs::{StableId, Store};
use std::collections::BTreeMap;

pub type Routes = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
#[derive(Clone, Copy, Debug)]
pub enum NavigationKnowledge<'a> {
    Personal(&'a Routes),
    FixtureCatalog(&'a Routes),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationMenu {
    pub known_areas: Vec<AreaId>,
    pub destinations: Vec<AreaId>,
}
/// Physical navigation can be supplied by a currently invoked memory, then
/// personal knowledge; only the bare fixture falls back to its authored graph.
pub fn navigation_routes(
    override_routes: Option<Routes>,
    personal: Option<&Routes>,
    fixture: impl FnOnce() -> Routes,
) -> Routes {
    override_routes
        .or_else(|| personal.cloned())
        .unwrap_or_else(fixture)
}
/// Personal route presence selects the live knowledge model. The absent personal
/// model is the existing bare-kernel fixture's authored navigation contract.
pub fn public_navigation_menu(
    personal: Option<&Routes>,
    fixture: &Routes,
    areas: impl IntoIterator<Item = AreaId>,
    origin: AreaId,
    include_origin: bool,
) -> NavigationMenu {
    navigation_menu(
        personal.map_or(
            NavigationKnowledge::FixtureCatalog(fixture),
            NavigationKnowledge::Personal,
        ),
        areas,
        origin,
        include_origin,
    )
}
/// The fixture catalog intentionally exposes all authored areas, including
/// disconnected ones; only reachable destinations enter the movement menu.
pub fn navigation_menu(
    knowledge: NavigationKnowledge<'_>,
    areas: impl IntoIterator<Item = AreaId>,
    origin: AreaId,
    include_origin: bool,
) -> NavigationMenu {
    let (routes, fixture) = match knowledge {
        NavigationKnowledge::Personal(routes) => (routes, false),
        NavigationKnowledge::FixtureCatalog(routes) => (routes, true),
    };
    let mut known_areas = Vec::new();
    let mut destinations = Vec::new();
    for area in areas {
        if fixture
            || routes.contains_key(&area)
            || routes.values().any(|links| links.contains_key(&area))
        {
            known_areas.push(area);
        }
        if (area != origin || include_origin) && known_path(routes, origin, area).is_ok() {
            destinations.push(area);
        }
    }
    NavigationMenu {
        known_areas,
        destinations,
    }
}
#[derive(Clone, Debug)]
pub struct BasicMenuFacts {
    pub alive: bool,
    pub health: u64,
    pub resolved_location: Location,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BasicMenuAction {
    Rest,
    Cancel,
}
/// Manual baseline options belong to Characters. Foreign owners supply their
/// own local affordances through an operation port; it carries no personal policy.
pub fn apply_basic_menu<A>(
    commands: &mut Vec<A>,
    facts: Option<BasicMenuFacts>,
    mut map: impl FnMut(BasicMenuAction) -> A,
    local_actions: impl FnOnce() -> Vec<A>,
) {
    let Some(facts) = facts else { return };
    if !facts.alive || facts.health == 0 {
        commands.clear();
    } else if matches!(
        facts.resolved_location,
        Location::AtArea(_) | Location::StoppedOnRoad { .. }
    ) {
        commands.push(map(BasicMenuAction::Rest));
        commands.push(map(BasicMenuAction::Cancel));
        commands.extend(local_actions());
    }
}
/// Presentation suggestions retain first occurrence across providers, including
/// already-advertised commands. Each provider owns the quantities it suggests.
pub fn append_quantity_suggestions<A: PartialEq, Q>(
    commands: &mut Vec<A>,
    opportunities: &[Q],
    suggest: impl Fn(&Q) -> Vec<A>,
) {
    for opportunity in opportunities {
        for command in suggest(opportunity) {
            if !commands.contains(&command) {
                commands.push(command);
            }
        }
    }
}
/// Local stationary people are visible regardless of their own life status;
/// the observer's death suppresses the entire view. Raw carrying/road rows stay
/// distinct from resolved physical contact, matching this public-person view.
pub fn nearby_people(
    store: &Store,
    observer: PersonId,
    observer_alive: Option<bool>,
) -> impl Iterator<Item = (PersonId, Role)> + '_ {
    let own = game_spatial::location(store, StableId(observer.0));
    (observer_alive != Some(false)).then_some(store).into_iter().flat_map(move |store| {
        persons(store).filter(move |(actor,_)| *actor != observer && matches!((game_spatial::location(store, StableId(actor.0)), own), (Some(Location::AtArea(first)), Some(Location::AtArea(second))) if first == second))
            .map(|(actor,row)| (actor,row.role))
    })
}
pub fn nearby_page_limit(requested: usize) -> usize {
    requested.min(100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::people::{Person, insert_person, register_people};
    #[test]
    fn navigation_route_priority_preserves_empty_sources_and_lazy_fixture_reads() {
        let personal =
            BTreeMap::from([(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(10))]))]);
        assert_eq!(
            navigation_routes(Some(Routes::new()), Some(&personal), || {
                panic!("an invocation supplies even an empty route map")
            }),
            Routes::new()
        );
        assert_eq!(
            navigation_routes(None, Some(&personal), || {
                panic!("personal knowledge prevents fixture enumeration")
            }),
            personal
        );
        assert!(
            navigation_routes(None, Some(&Routes::new()), || {
                panic!("empty personal knowledge still prevents fixture enumeration")
            })
            .is_empty()
        );
        let mut fixture_reads = 0;
        assert_eq!(
            navigation_routes(None, None, || {
                fixture_reads += 1;
                personal.clone()
            }),
            personal
        );
        assert_eq!(fixture_reads, 1);
    }
    #[test]
    fn navigation_preserves_explicit_fixture_catalog_and_personal_knowledge() {
        let world = BTreeMap::from([
            (AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(10))])),
            (AreaId(2), BTreeMap::new()),
            (AreaId(3), BTreeMap::new()),
        ]);
        let personal =
            BTreeMap::from([(AreaId(1), BTreeMap::from([(AreaId(2), SimDuration(10))]))]);
        let fixture = public_navigation_menu(
            None,
            &world,
            [AreaId(1), AreaId(2), AreaId(3)],
            AreaId(1),
            false,
        );
        assert_eq!(fixture.known_areas, vec![AreaId(1), AreaId(2), AreaId(3)]);
        assert_eq!(fixture.destinations, vec![AreaId(2)]);
        let personal = public_navigation_menu(
            Some(&personal),
            &world,
            [AreaId(1), AreaId(2), AreaId(3)],
            AreaId(1),
            true,
        );
        assert_eq!(personal.known_areas, vec![AreaId(1), AreaId(2)]);
        assert_eq!(personal.destinations, vec![AreaId(1), AreaId(2)]);
        let unknown = navigation_menu(
            NavigationKnowledge::Personal(&Routes::new()),
            [AreaId(1), AreaId(2), AreaId(3)],
            AreaId(1),
            false,
        );
        assert!(unknown.known_areas.is_empty());
        assert!(unknown.destinations.is_empty());
    }
    #[test]
    fn manual_baseline_menu_queries_local_owners_only_for_healthy_stationary_actors() {
        for (location, local) in [
            (Location::AtArea(AreaId(1)), true),
            (
                Location::StoppedOnRoad {
                    from: AreaId(1),
                    to: AreaId(2),
                    progress: SimDuration(5),
                    duration: SimDuration(10),
                },
                true,
            ),
            (
                Location::InTransit {
                    from: AreaId(1),
                    to: AreaId(2),
                    departed: SimTime(0),
                    arrives: SimTime(10),
                    road_start: None,
                },
                false,
            ),
        ] {
            let mut commands = vec![1];
            apply_basic_menu(
                &mut commands,
                Some(BasicMenuFacts {
                    alive: true,
                    health: 1,
                    resolved_location: location,
                }),
                |action| match action {
                    BasicMenuAction::Rest => 2,
                    BasicMenuAction::Cancel => 3,
                },
                || vec![4, 5],
            );
            assert_eq!(commands, if local { vec![1, 2, 3, 4, 5] } else { vec![1] });
        }
        let mut dead = vec![1];
        apply_basic_menu(
            &mut dead,
            Some(BasicMenuFacts {
                alive: false,
                health: 1,
                resolved_location: Location::AtArea(AreaId(1)),
            }),
            |_| panic!("dead actor has no basic actions"),
            || panic!("dead actor cannot query local affordances"),
        );
        assert!(dead.is_empty());
        let mut kernel = vec![1];
        apply_basic_menu(
            &mut kernel,
            None,
            |_| panic!("kernel has no physical baseline"),
            || panic!("kernel has no physical local menu"),
        );
        assert_eq!(kernel, vec![1]);
        append_quantity_suggestions(&mut kernel, &[vec![1, 2, 2], vec![3, 2]], Clone::clone);
        assert_eq!(kernel, vec![1, 2, 3]);
    }
    #[test]
    fn nearby_visibility_preserves_stationary_roles_self_death_and_paging_cap() {
        let mut schema = game_ecs::Schema::new();
        register_people(&mut schema).unwrap();
        game_spatial::register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        for actor in 1..=5 {
            insert_person(
                &mut store,
                Person {
                    id: PersonId(actor),
                    role: if actor == 1 {
                        Role::Ruler
                    } else {
                        Role::Citizen
                    },
                    inventory: InventoryId(100 + actor),
                    work_units: 0,
                    work_remainder: 0,
                    work_rate: 1000,
                    hunger_exempt: false,
                },
            )
            .unwrap();
            game_spatial::set_location(
                &mut store,
                StableId(actor),
                Location::AtArea(AreaId(if actor == 5 { 2 } else { 1 })),
            )
            .unwrap();
        }
        game_spatial::set_location(
            &mut store,
            StableId(3),
            Location::Carried {
                carrier: PersonId(1),
            },
        )
        .unwrap();
        game_spatial::set_location(
            &mut store,
            StableId(4),
            Location::StoppedOnRoad {
                from: AreaId(1),
                to: AreaId(2),
                progress: SimDuration(5),
                duration: SimDuration(10),
            },
        )
        .unwrap();
        let before = store.clone();
        assert_eq!(
            nearby_people(&store, PersonId(1), None).collect::<Vec<_>>(),
            vec![(PersonId(2), Role::Citizen)]
        );
        assert!(
            nearby_people(&store, PersonId(1), Some(false))
                .next()
                .is_none()
        );
        assert_eq!(nearby_page_limit(200), 100);
        assert_eq!(nearby_page_limit(0), 0);
        assert_eq!(store, before);
    }
}
