//! Mission tactics select intentions from remembered positions and real geometry.
use crate::mission as stored_mission;
use crate::*;
use game_conflict::{Fighter, Map, SearchState};
use game_core::*;
use game_ecs::Store;
use game_logistics::KnownRoutes;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug)]
pub struct TacticalCapabilities {
    pub release_available: bool,
    pub target_can_fight: bool,
    pub target_captive: bool,
}
pub trait TacticalFacts {
    fn capabilities(&self, target: PersonId) -> TacticalCapabilities;
}
pub struct TacticalInput<'a> {
    pub actor: PersonId,
    pub player: bool,
    pub area: AreaId,
    pub map: &'a Map,
    pub fighter: &'a Fighter,
    pub roster: &'a BTreeMap<PersonId, Fighter>,
    pub routes: &'a KnownRoutes,
    pub health: u64,
    pub max_health: u64,
    pub risk_tolerance: u16,
    pub movement: u64,
    pub reach: u64,
    pub binding_available: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TacticAction {
    Defend,
    Surrender,
    Retreat(AreaId),
    Release(PersonId),
    Capture(PersonId),
    Attack(PersonId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TacticalDecision {
    pub movement: Option<Point>,
    pub action: TacticAction,
    pub search: Option<SearchState>,
}
fn distance(a: Point, b: Point) -> u128 {
    u128::from(a.x.abs_diff(b.x)).pow(2) + u128::from(a.y.abs_diff(b.y)).pow(2)
}
fn search(input: &TacticalInput<'_>) -> TacticalDecision {
    let mut search = input.fighter.search.clone();
    let movement = input
        .map
        .search_step(input.fighter.position, input.movement, &mut search);
    TacticalDecision {
        movement,
        action: TacticAction::Defend,
        search: Some(search),
    }
}
impl Runtime {
    pub fn tactical_decision(
        &self,
        store: &Store,
        id: MissionId,
        input: TacticalInput<'_>,
        facts: &impl TacticalFacts,
    ) -> Option<TacticalDecision> {
        if input.player {
            return None;
        }
        let member = stored_mission(store, id)?.members.get(&input.actor)?;
        let known_status = knowledge(store, input.actor)
            .map(|row| &row.missions)
            .and_then(|known| known.get(&id))
            .map(|view| view.status);
        let f = input.fighter;
        let map = input.map;
        if combat_retreat_wanted(
            input.health,
            input.max_health,
            input.risk_tolerance,
            known_status,
        ) {
            let exit = map
                .exits
                .iter()
                .filter(|(area, _)| {
                    game_characters::decisions::known_path(input.routes, input.area, *area).is_ok()
                })
                .filter(|(_, point)| map.route(f.position, *point).is_some())
                .min_by_key(|(area, point)| (distance(f.position, *point), *area));
            if let Some((destination, exit)) = exit {
                let movement = map.navigation_step(f.position, *exit, input.movement, 0);
                let position = movement.unwrap_or(f.position);
                let action = if distance(position, *exit) <= u128::from(input.reach).pow(2)
                    && map.clear(position, *exit)
                {
                    TacticAction::Retreat(*destination)
                } else {
                    TacticAction::Defend
                };
                return Some(TacticalDecision {
                    movement,
                    action,
                    search: None,
                });
            }
            return Some(TacticalDecision {
                movement: None,
                action: TacticAction::Surrender,
                search: None,
            });
        }
        let target = match member.directive {
            MissionObjective::Defeat { person }
            | MissionObjective::Rescue { person, .. }
            | MissionObjective::Confine { person, .. }
                if f.known.contains_key(&person) =>
            {
                Some(person)
            }
            _ => f
                .known
                .keys()
                .filter(|person| {
                    input.roster.get(person).is_some_and(|other| {
                        other.team != f.team && map.clear(f.position, other.position)
                    })
                })
                .min_by_key(|person| (distance(f.position, f.known[person]), **person))
                .copied(),
        };
        let Some(target) = target else {
            return Some(search(&input));
        };
        let point = f.known[&target];
        let other = &input.roster[&target];
        let movement = map.navigation_step(
            f.position,
            point,
            input.movement,
            input.reach.saturating_sub(2),
        );
        let visible = map.clear(f.position, other.position);
        if !visible
            && (movement.is_none()
                || map.clear(f.position, point)
                    && distance(f.position, point) <= u128::from(input.reach).pow(2))
        {
            return Some(search(&input));
        }
        let from = movement.unwrap_or(f.position);
        let reachable = visible
            && distance(from, point) <= u128::from(input.reach).pow(2)
            && map.clear(from, point);
        let capabilities = facts.capabilities(target);
        let action = if reachable
            && matches!(member.directive,MissionObjective::Rescue{person,..}if person==target)
            && capabilities.release_available
        {
            TacticAction::Release(target)
        } else if reachable
            && matches!(member.directive,MissionObjective::Confine{person,..}if person==target)
            && (other.surrendered || !capabilities.target_can_fight)
            && input.binding_available
            && !capabilities.target_captive
        {
            TacticAction::Capture(target)
        } else if reachable
            && other.position == point
            && map.clear(from, other.position)
            && other.team != f.team
            && !other.surrendered
        {
            TacticAction::Attack(target)
        } else {
            TacticAction::Defend
        };
        Some(TacticalDecision {
            movement,
            action,
            search: None,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ServiceAvailability {
    pub guardian_due: bool,
    pub alive: bool,
    pub health: u64,
    pub max_health: u64,
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub restricted: bool,
    pub care_urgency: i64,
}
impl Runtime {
    pub fn member_busy(&self, store: &Store, actor: PersonId, facts: ServiceAvailability) -> bool {
        self.membership(store, actor).is_some()
            && !facts.guardian_due
            && facts.alive
            && facts.health > facts.max_health / 2
            && facts.hunger < facts.meal_relief
            && facts.fatigue < facts.fatigue_limit
            && !facts.restricted
            && facts.care_urgency < 20_000
    }
}
#[cfg(test)]
mod tests;
