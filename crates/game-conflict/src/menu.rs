//! Tactical command availability over already-observed fighter facts.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCommandKind {
    Rest,
    Eat,
    Tell,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuFighter {
    pub actor: PersonId,
    pub incapacitated: bool,
    pub surrendered: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct TacticalMenuFacts<'a> {
    pub encounter: EncounterId,
    pub turn: Option<PersonId>,
    pub fighters: &'a [MenuFighter],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Wait,
    Defend,
    Surrender,
    Attack(PersonId),
    Capture(PersonId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuPolicy {
    Preserve,
    Restricted,
    Combat {
        encounter: Option<EncounterId>,
        actions: Vec<MenuAction>,
    },
}
impl MenuPolicy {
    pub fn filter_commands<A>(
        &self,
        commands: &mut Vec<A>,
        classify: impl Fn(&A) -> MenuCommandKind,
    ) {
        match self {
            Self::Combat { .. } => commands.clear(),
            Self::Preserve => {}
            Self::Restricted => commands.retain(|command| self.retains(classify(command))),
        }
    }
    /// The receiving application only classifies its public command variant.
    pub fn retains(&self, kind: MenuCommandKind) -> bool {
        match self {
            Self::Preserve => true,
            Self::Restricted => matches!(
                kind,
                MenuCommandKind::Rest | MenuCommandKind::Eat | MenuCommandKind::Tell
            ),
            Self::Combat { .. } => false,
        }
    }
}
pub fn menu_policy(
    actor: PersonId,
    in_combat: bool,
    restricted: bool,
    tactical: Option<TacticalMenuFacts<'_>>,
) -> MenuPolicy {
    if in_combat {
        let encounter = tactical.as_ref().map(|facts| facts.encounter);
        let mut actions = Vec::new();
        if let Some(facts) = tactical
            && facts.turn == Some(actor)
        {
            actions.extend([MenuAction::Wait, MenuAction::Defend, MenuAction::Surrender]);
            for fighter in facts.fighters {
                if fighter.actor == actor {
                    continue;
                }
                actions.push(MenuAction::Attack(fighter.actor));
                if fighter.incapacitated || fighter.surrendered {
                    actions.push(MenuAction::Capture(fighter.actor));
                }
            }
        }
        MenuPolicy::Combat { encounter, actions }
    } else if restricted {
        MenuPolicy::Restricted
    } else {
        MenuPolicy::Preserve
    }
}

/// The predicate invokes the receiving release owner's factual operation; this
/// owner selects targets from canonical custody in stable identity order.
pub fn available_releases(
    store: &game_ecs::Store,
    actor: PersonId,
    mut available: impl FnMut(PersonId, PersonId) -> bool,
) -> Vec<PersonId> {
    captivities(store)
        .map(|(target, _)| target)
        .filter(|target| available(actor, *target))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combat_replaces_existing_menu_and_preserves_exact_turn_and_fighter_order() {
        let actor = PersonId(1);
        let id = EncounterId(9);
        let fighters = [
            MenuFighter {
                actor,
                incapacitated: true,
                surrendered: true,
            },
            MenuFighter {
                actor: PersonId(5),
                incapacitated: false,
                surrendered: false,
            },
            MenuFighter {
                actor: PersonId(2),
                incapacitated: true,
                surrendered: false,
            },
            MenuFighter {
                actor: PersonId(4),
                incapacitated: false,
                surrendered: true,
            },
        ];
        let policy = menu_policy(
            actor,
            true,
            true,
            Some(TacticalMenuFacts {
                encounter: id,
                turn: Some(actor),
                fighters: &fighters,
            }),
        );
        assert_eq!(
            policy,
            MenuPolicy::Combat {
                encounter: Some(id),
                actions: vec![
                    MenuAction::Wait,
                    MenuAction::Defend,
                    MenuAction::Surrender,
                    MenuAction::Attack(PersonId(5)),
                    MenuAction::Attack(PersonId(2)),
                    MenuAction::Capture(PersonId(2)),
                    MenuAction::Attack(PersonId(4)),
                    MenuAction::Capture(PersonId(4))
                ]
            }
        );
        assert!(!policy.retains(MenuCommandKind::Rest));
        let policy = menu_policy(
            actor,
            true,
            false,
            Some(TacticalMenuFacts {
                encounter: id,
                turn: Some(PersonId(2)),
                fighters: &fighters,
            }),
        );
        assert_eq!(
            policy,
            MenuPolicy::Combat {
                encounter: Some(id),
                actions: Vec::new()
            }
        );
        assert_eq!(
            menu_policy(actor, true, false, None),
            MenuPolicy::Combat {
                encounter: None,
                actions: Vec::new()
            }
        );
    }
    #[test]
    fn restriction_filters_only_the_baseline_and_preserve_allows_every_class() {
        let restricted = menu_policy(PersonId(1), false, true, None);
        for kind in [
            MenuCommandKind::Rest,
            MenuCommandKind::Eat,
            MenuCommandKind::Tell,
        ] {
            assert!(restricted.retains(kind));
        }
        assert!(!restricted.retains(MenuCommandKind::Other));
        assert!(menu_policy(PersonId(1), false, false, None).retains(MenuCommandKind::Other));
        let mut commands = vec![
            MenuCommandKind::Other,
            MenuCommandKind::Tell,
            MenuCommandKind::Eat,
            MenuCommandKind::Rest,
        ];
        restricted.filter_commands(&mut commands, |kind| *kind);
        assert_eq!(
            commands,
            vec![
                MenuCommandKind::Tell,
                MenuCommandKind::Eat,
                MenuCommandKind::Rest
            ]
        );
        menu_policy(PersonId(1), true, false, None).filter_commands(&mut commands, |kind| *kind);
        assert!(commands.is_empty());
    }
    #[test]
    fn release_target_iteration_uses_only_canonical_custody_in_identity_order() {
        let mut store = crate::test_store();
        for actor in [PersonId(3), PersonId(2)] {
            insert_captivity(
                &mut store,
                actor,
                Captivity {
                    actor,
                    captor: PersonId(9),
                    area: AreaId(1),
                    position: Point { x: 1, y: 1 },
                    map: CombatMapId(5),
                    binding: InventoryId(actor.0 + 10),
                },
            )
            .unwrap();
        }
        let mut queried = Vec::new();
        let selected = available_releases(&store, PersonId(9), |actor, target| {
            assert_eq!(actor, PersonId(9));
            queried.push(target);
            target == PersonId(3)
        });
        assert_eq!(queried, vec![PersonId(2), PersonId(3)]);
        assert_eq!(selected, vec![PersonId(3)]);
    }
}
