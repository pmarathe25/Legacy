//! Conflict-owned default combat choices and their navigation policy.
use crate::*;

/// Facts about one fighter known to the acting combatant. `known_position` is
/// the only coordinate used for target selection and movement. The current
/// position is supplied separately for line-of-sight and reach checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CombatTargetFacts {
    pub actor: PersonId,
    pub known_position: Point,
    pub current_position: Point,
    pub opposing: bool,
    pub surrendered: bool,
    pub can_fight: bool,
    pub captive: bool,
    pub force_can_act: bool,
}

/// Plain body, relationship, and inventory facts needed by default combat
/// autonomy. `can_release` is the result of the confinement owner's operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatAutonomyFacts {
    pub actor: PersonId,
    pub team: u32,
    pub position: Point,
    pub health: u64,
    pub max_health: u64,
    pub risk_tolerance: u16,
    pub guardian: bool,
    pub ruler: PersonId,
    pub rescue_target: Option<PersonId>,
    pub can_release: bool,
    pub binding_available: bool,
    pub targets: Vec<CombatTargetFacts>,
    pub search: SearchState,
}

/// A default policy action. Integration-specific mission and force actions
/// are sequenced by the assembly before this policy is called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatAutonomyAction {
    Defend,
    Surrender,
    Capture(PersonId),
    Attack(PersonId),
    Release(PersonId),
}

/// Complete projection of the default policy, including search progress.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatAutonomyDecision {
    pub target: Option<PersonId>,
    pub movement: Option<Point>,
    pub action: CombatAutonomyAction,
    pub search: SearchState,
}

/// Selects the closest known opponent with a stable person-ID tie-break, then
/// chooses the default action and legal navigation leg using observed facts.
pub fn default_combat_policy(
    map: &Map,
    rules: &Rules,
    facts: &CombatAutonomyFacts,
) -> CombatAutonomyDecision {
    let known_rescue = facts.rescue_target.filter(|target| {
        facts
            .targets
            .iter()
            .any(|candidate| candidate.actor == *target)
    });
    let target = known_rescue.or_else(|| {
        facts
            .targets
            .iter()
            .filter(|candidate| candidate.opposing)
            .min_by_key(|candidate| {
                (
                    distance(facts.position, candidate.known_position),
                    candidate.actor,
                )
            })
            .map(|candidate| candidate.actor)
    });
    let mut search = facts.search.clone();
    let Some(target_id) = target else {
        let movement = map.search_step(facts.position, rules.movement, &mut search);
        return CombatAutonomyDecision {
            target: None,
            movement,
            action: CombatAutonomyAction::Defend,
            search,
        };
    };
    let selected = facts
        .targets
        .iter()
        .find(|candidate| candidate.actor == target_id)
        .expect("selected target has facts");
    let destination = selected.known_position;
    let movement = map.navigation_step(
        facts.position,
        destination,
        rules.movement,
        rules.reach.saturating_sub(2),
    );
    let visible = map.clear(facts.position, selected.current_position);
    if !visible
        && (movement.is_none()
            || map.clear(facts.position, destination)
                && distance(facts.position, destination) <= u128::from(rules.reach).pow(2))
    {
        let search_movement = map.search_step(facts.position, rules.movement, &mut search);
        return CombatAutonomyDecision {
            target: Some(target_id),
            movement: search_movement,
            action: CombatAutonomyAction::Defend,
            search,
        };
    }
    let from = movement.unwrap_or(facts.position);
    let reachable = visible
        && map.clear(from, destination)
        && distance(from, selected.current_position) <= u128::from(rules.reach).pow(2);
    let action = if known_rescue == Some(target_id) && reachable && facts.can_release {
        CombatAutonomyAction::Release(target_id)
    } else if facts.guardian && target_id == facts.ruler {
        CombatAutonomyAction::Defend
    } else if facts.risk_tolerance < 500 && facts.health < facts.max_health / 2 {
        CombatAutonomyAction::Surrender
    } else if reachable
        && (selected.surrendered || !selected.can_fight)
        && facts.binding_available
        && !selected.captive
    {
        CombatAutonomyAction::Capture(target_id)
    } else if reachable
        && !selected.surrendered
        && (!selected.captive || selected.force_can_act)
        && known_rescue != Some(target_id)
    {
        CombatAutonomyAction::Attack(target_id)
    } else {
        CombatAutonomyAction::Defend
    };
    CombatAutonomyDecision {
        target: Some(target_id),
        movement,
        action,
        search,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena() -> Map {
        Map {
            id: CombatMapId(1),
            version: 1,
            area: AreaId(1),
            location: Location::AtArea(AreaId(1)),
            width: 20_000,
            height: 20_000,
            obstacles: vec![],
            exits: vec![],
        }
    }

    fn facts() -> CombatAutonomyFacts {
        CombatAutonomyFacts {
            actor: PersonId(1),
            team: 0,
            position: Point { x: 2_000, y: 2_000 },
            health: 100,
            max_health: 100,
            risk_tolerance: 500,
            guardian: false,
            ruler: PersonId(1),
            rescue_target: None,
            can_release: false,
            binding_available: true,
            targets: vec![],
            search: SearchState::default(),
        }
    }

    fn target(actor: u64, x: i64) -> CombatTargetFacts {
        CombatTargetFacts {
            actor: PersonId(actor),
            known_position: Point { x, y: 2_000 },
            current_position: Point { x, y: 2_000 },
            opposing: true,
            surrendered: false,
            can_fight: true,
            captive: false,
            force_can_act: false,
        }
    }

    fn rules() -> Rules {
        Rules {
            encounter_limit: 16,
            round: SimDuration(1),
            movement: 2_000,
            reach: 2_000,
            damage: 10,
            defence: 2,
            incapacitation: 10,
        }
    }

    #[test]
    fn target_selection_uses_distance_then_person_id_and_rescue_preempts_distance() {
        let map = arena();
        let mut input = facts();
        input.targets = vec![target(8, 4_000), target(3, 0), target(2, 4_000)];
        let result = default_combat_policy(&map, &rules(), &input);
        assert_eq!(result.target, Some(PersonId(2)));
        input.rescue_target = Some(PersonId(8));
        let result = default_combat_policy(&map, &rules(), &input);
        assert_eq!(result.target, Some(PersonId(8)));
    }

    #[test]
    fn guardian_safeguard_and_injury_threshold_belong_to_default_policy() {
        let map = arena();
        let mut input = facts();
        input.guardian = true;
        input.ruler = PersonId(2);
        input.targets = vec![target(2, 2_500)];
        assert_eq!(
            default_combat_policy(&map, &rules(), &input).action,
            CombatAutonomyAction::Defend
        );
        input.guardian = false;
        input.risk_tolerance = 499;
        input.health = 49;
        assert_eq!(
            default_combat_policy(&map, &rules(), &input).action,
            CombatAutonomyAction::Surrender
        );
    }

    #[test]
    fn owner_policy_projects_search_progress_and_capture_attack_release_choices() {
        let map = arena();
        let mut input = facts();
        input.targets = vec![target(2, 2_500)];
        input.targets[0].surrendered = true;
        assert_eq!(
            default_combat_policy(&map, &rules(), &input).action,
            CombatAutonomyAction::Capture(PersonId(2))
        );
        input.targets[0].surrendered = false;
        assert_eq!(
            default_combat_policy(&map, &rules(), &input).action,
            CombatAutonomyAction::Attack(PersonId(2))
        );
        input.rescue_target = Some(PersonId(2));
        input.can_release = true;
        assert_eq!(
            default_combat_policy(&map, &rules(), &input).action,
            CombatAutonomyAction::Release(PersonId(2))
        );
        input.targets.clear();
        let search = default_combat_policy(&map, &rules(), &input);
        assert!(search.movement.is_some());
        assert_ne!(search.search, SearchState::default());
    }
}
