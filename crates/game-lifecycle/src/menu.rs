//! Lifecycle policy for public menus, independent of application action types.
use crate::{ChildhoodInterest, Life, LifeStage, PersonId, SimTime, dependent, independent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuActionKind {
    Work,
    Signal,
    Cancel,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependentMenuAction {
    Cancel,
    Signal(ChildhoodInterest),
    Play(ChildhoodInterest),
}
#[derive(Clone, Copy, Debug)]
pub struct DependentMenuFacts {
    pub stage: Option<LifeStage>,
    pub alive: bool,
    pub health: u64,
    pub at_area: bool,
}
/// Callbacks identify existing action variants and construct descriptors only.
/// Filtering, availability, and append order belong to the lifecycle owner.
pub fn apply_dependent_menu<A>(
    actions: &mut Vec<A>,
    facts: DependentMenuFacts,
    classify: impl Fn(&A) -> MenuActionKind,
    mut map: impl FnMut(DependentMenuAction) -> A,
) {
    match facts.stage {
        Some(LifeStage::Infant) => {
            actions.clear();
            if facts.alive {
                actions.push(map(DependentMenuAction::Cancel));
            }
        }
        Some(LifeStage::Child) => actions.retain(|action| classify(action) != MenuActionKind::Work),
        _ => {}
    }
    if !facts.alive || facts.health == 0 || !dependent(facts.stage) {
        return;
    }
    let plays = facts.stage == Some(LifeStage::Child) && facts.at_area;
    for preference in [ChildhoodInterest::Food, ChildhoodInterest::Company] {
        actions.push(map(DependentMenuAction::Signal(preference)));
        if plays {
            actions.push(map(DependentMenuAction::Play(preference)));
        }
    }
    if plays {
        for interest in [ChildhoodInterest::Movement, ChildhoodInterest::Craft] {
            actions.push(map(DependentMenuAction::Play(interest)));
        }
    }
}
/// Apply the final agency restriction after other owners append their choices.
pub fn filter_independent_menu<A, Q>(
    actions: &mut Vec<A>,
    quantities: &mut Vec<Q>,
    stage: Option<LifeStage>,
    lifecycle_enabled: bool,
    classify: impl Fn(&A) -> MenuActionKind,
) {
    if !independent(stage, lifecycle_enabled) {
        actions.retain(|action| {
            matches!(
                classify(action),
                MenuActionKind::Signal | MenuActionKind::Cancel
            )
        });
        quantities.clear();
    }
}
pub fn active_preference(life: &Life, now: SimTime) -> Option<ChildhoodInterest> {
    life.preference
        .filter(|(_, until)| *until >= now)
        .map(|(preference, _)| preference)
}
pub fn visible_dependents<'a>(
    rows: impl IntoIterator<Item = (PersonId, &'a Life, bool)>,
) -> Vec<PersonId> {
    rows.into_iter()
        .filter_map(|(actor, life, visible_signal)| {
            (visible_signal && dependent(Some(life.stage))).then_some(actor)
        })
        .collect()
}
pub fn comfort_targets(
    actor: PersonId,
    targets: impl IntoIterator<Item = PersonId>,
) -> Vec<PersonId> {
    targets
        .into_iter()
        .filter(|target| *target != actor)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Reproduction;
    use game_core::SimDuration;
    use std::collections::BTreeSet;
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Action {
        Work,
        Other,
        Child(DependentMenuAction),
    }
    fn classify(action: &Action) -> MenuActionKind {
        match action {
            Action::Work => MenuActionKind::Work,
            Action::Child(DependentMenuAction::Cancel) => MenuActionKind::Cancel,
            Action::Child(DependentMenuAction::Signal(_)) => MenuActionKind::Signal,
            _ => MenuActionKind::Other,
        }
    }
    fn facts(stage: LifeStage) -> DependentMenuFacts {
        DependentMenuFacts {
            stage: Some(stage),
            alive: true,
            health: 10,
            at_area: true,
        }
    }
    fn life(id: u64, stage: LifeStage) -> Life {
        Life {
            id: PersonId(id),
            born: SimTime(0),
            initial_age: SimDuration(0),
            stage,
            reproduction: Reproduction::Neither,
            parents: vec![],
            caregivers: BTreeSet::new(),
            preference: None,
        }
    }
    #[test]
    fn infant_and_child_public_menus_preserve_exact_order_and_zero_health_cancel() {
        let mut infant = vec![Action::Work, Action::Other];
        apply_dependent_menu(
            &mut infant,
            facts(LifeStage::Infant),
            classify,
            Action::Child,
        );
        assert_eq!(
            infant,
            vec![
                Action::Child(DependentMenuAction::Cancel),
                Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Food)),
                Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Company))
            ]
        );
        let mut child = vec![Action::Work, Action::Other];
        apply_dependent_menu(&mut child, facts(LifeStage::Child), classify, Action::Child);
        assert_eq!(
            child,
            vec![
                Action::Other,
                Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Food)),
                Action::Child(DependentMenuAction::Play(ChildhoodInterest::Food)),
                Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Company)),
                Action::Child(DependentMenuAction::Play(ChildhoodInterest::Company)),
                Action::Child(DependentMenuAction::Play(ChildhoodInterest::Movement)),
                Action::Child(DependentMenuAction::Play(ChildhoodInterest::Craft))
            ]
        );
        let mut zero = vec![Action::Other];
        apply_dependent_menu(
            &mut zero,
            DependentMenuFacts {
                health: 0,
                ..facts(LifeStage::Infant)
            },
            classify,
            Action::Child,
        );
        assert_eq!(zero, vec![Action::Child(DependentMenuAction::Cancel)]);
        apply_dependent_menu(
            &mut zero,
            DependentMenuFacts {
                alive: false,
                ..facts(LifeStage::Infant)
            },
            classify,
            Action::Child,
        );
        assert!(zero.is_empty());
        let mut road = vec![Action::Work, Action::Other];
        apply_dependent_menu(
            &mut road,
            DependentMenuFacts {
                at_area: false,
                ..facts(LifeStage::Child)
            },
            classify,
            Action::Child,
        );
        assert_eq!(
            road,
            vec![
                Action::Other,
                Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Food)),
                Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Company))
            ]
        );
    }
    #[test]
    fn final_dependency_gate_removes_other_owner_choices_and_quantity_suggestions() {
        let signal = Action::Child(DependentMenuAction::Signal(ChildhoodInterest::Food));
        let cancel = Action::Child(DependentMenuAction::Cancel);
        let mut actions = vec![Action::Other, signal, Action::Work, cancel];
        let mut quantities = vec![1, 2];
        filter_independent_menu(
            &mut actions,
            &mut quantities,
            Some(LifeStage::Infant),
            true,
            classify,
        );
        assert_eq!(actions, vec![signal, cancel]);
        assert!(quantities.is_empty());
        let mut actions = vec![Action::Other, Action::Work];
        let mut quantities = vec![1];
        filter_independent_menu(&mut actions, &mut quantities, None, false, classify);
        assert_eq!(actions, vec![Action::Other, Action::Work]);
        assert_eq!(quantities, vec![1]);
    }
    #[test]
    fn visible_dependent_targets_keep_input_order_and_preferences_expire_after_their_boundary() {
        let mut infant = life(2, LifeStage::Infant);
        infant.preference = Some((ChildhoodInterest::Company, SimTime(10)));
        let child = life(3, LifeStage::Child);
        let adult = life(4, LifeStage::Adult);
        let hidden = life(5, LifeStage::Child);
        let targets = visible_dependents([
            (PersonId(3), &child, true),
            (PersonId(4), &adult, true),
            (PersonId(2), &infant, true),
            (PersonId(5), &hidden, false),
        ]);
        assert_eq!(targets, vec![PersonId(3), PersonId(2)]);
        assert_eq!(comfort_targets(PersonId(3), targets), vec![PersonId(2)]);
        assert_eq!(
            active_preference(&infant, SimTime(10)),
            Some(ChildhoodInterest::Company)
        );
        assert_eq!(active_preference(&infant, SimTime(11)), None);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependentCommand {
    Comfort {
        child: PersonId,
    },
    Care {
        child: PersonId,
        source: game_core::InventoryId,
    },
}
/// Candidate order follows the supplied visible targets and physical sources.
/// The operation callback invokes the existing care admission owner; it must
/// not reimplement care eligibility in an application adapter.
pub fn dependent_command_menu(
    actor: PersonId,
    targets: impl IntoIterator<Item = PersonId>,
    source_ids: &[game_core::InventoryId],
    mut care_available: impl FnMut(PersonId, game_core::InventoryId) -> bool,
) -> Vec<DependentCommand> {
    let mut commands = Vec::new();
    for child in targets {
        if child != actor {
            commands.push(DependentCommand::Comfort { child });
        }
        for source in source_ids {
            if care_available(child, *source) {
                commands.push(DependentCommand::Care {
                    child,
                    source: *source,
                });
            }
        }
    }
    commands
}

#[cfg(test)]
mod command_tests {
    use super::*;
    use crate::{CareSourceFacts, prepare_care_source};
    use game_core::{InventoryId, Quantity};
    #[test]
    fn dependent_command_discovery_orders_comfort_and_owner_admitted_sources_per_target() {
        let actor = PersonId(1);
        let mut calls = Vec::new();
        let commands = dependent_command_menu(
            actor,
            [PersonId(3), actor, PersonId(2)],
            &[InventoryId(12), InventoryId(10)],
            |child, source| {
                calls.push((child, source));
                prepare_care_source(CareSourceFacts {
                    actor,
                    child,
                    stage: LifeStage::Child,
                    alive: true,
                    available: true,
                    assigned_caregiver: None,
                    same_place: true,
                    authorized_source: source == InventoryId(10),
                    available_quantity: 1_000,
                    dose: Quantity(1_000),
                })
                .is_ok()
            },
        );
        assert_eq!(
            commands,
            vec![
                DependentCommand::Comfort { child: PersonId(3) },
                DependentCommand::Care {
                    child: PersonId(3),
                    source: InventoryId(10)
                },
                DependentCommand::Comfort { child: PersonId(2) },
                DependentCommand::Care {
                    child: PersonId(2),
                    source: InventoryId(10)
                }
            ]
        );
        assert_eq!(
            calls,
            vec![
                (PersonId(3), InventoryId(12)),
                (PersonId(3), InventoryId(10)),
                (actor, InventoryId(12)),
                (actor, InventoryId(10)),
                (PersonId(2), InventoryId(12)),
                (PersonId(2), InventoryId(10))
            ]
        );
    }
}
