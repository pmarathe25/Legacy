//! Public control choices follow the same consent and learned-access rules as admission.
use crate::*;
use game_core::PersonId;
use game_ecs::Store;
use std::collections::BTreeSet;

impl ControlState {
    pub fn available_actions(&self, store: &Store, facts: &AdmissionFacts) -> Vec<ControlAction> {
        if controls_actor(
            self.active(store)
                .and_then(|active| active.episode.as_ref()),
            facts.actor,
            facts.now,
            facts.alive,
        ) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        if let Some(pending) = self.pending_warning(store)
            && pending.actor == facts.actor
        {
            actions.push(ControlAction::AcceptWarning {
                warning: pending.id,
            });
        }
        if self
            .active(store)
            .is_some_and(|active| active.actor == facts.actor)
            || self
                .pending_warning(store)
                .is_some_and(|warning| warning.actor == facts.actor)
        {
            actions.push(ControlAction::Dismiss);
        }
        if self.ready(facts.actor, facts) && self.pending_consent(store).is_none() {
            for life in facts.archived.iter().rev().take(32) {
                let lives = BTreeSet::from([*life]);
                if self
                    .active(store)
                    .is_none_or(|active| active.lives != lives)
                {
                    actions.push(ControlAction::Activate { lives });
                }
            }
        }
        actions
    }
    /// The body's life and clock are factual inputs; choosing the active owner is invocation policy.
    pub fn actor(&self, store: &Store) -> Option<PersonId> {
        one_session(store).ok().flatten().map(|(actor, _)| actor)
    }
    pub fn next_cost_for(
        &self,
        store: &Store,
        actor: PersonId,
        lives: usize,
    ) -> Option<game_core::SimDuration> {
        let empty = Burden::default();
        let before = burden(store, actor).unwrap_or(&empty);
        let next_loss = self
            .active(store)
            .filter(|active| active.actor == actor)
            .map_or(self.rules.loss_threshold, |active| active.next_loss_burden);
        next_cost_after(&self.rules, before, lives, next_loss)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use game_core::SimTime;
    #[test]
    fn options_use_guidance_then_reverse_archive_order_and_stop_during_control_loss() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let mut store = Store::with_schema(schema);
        let control = ControlState::new(Rules::fixture()).unwrap();
        let actor = PersonId(100);
        let mut facts = AdmissionFacts {
            actor,
            now: SimTime(0),
            current: Some(actor),
            alive: true,
            health: 100000,
            independent: true,
            learning: 0,
            required_learning: 100,
            fatigue: 0,
            fatigue_limit: 100000,
            archived: (1..=40).map(PersonId).collect(),
        };
        assert!(control.available_actions(&store, &facts).is_empty());
        facts.learning = 100;
        let actions = control.available_actions(&store, &facts);
        assert_eq!(actions.len(), 32);
        assert!(
            matches!(&actions[0], ControlAction::Activate { lives } if lives == &BTreeSet::from([PersonId(40)]))
        );
        insert_burden(&mut store, actor, Burden::default()).unwrap();
        let active = Active {
            actor,
            lives: BTreeSet::from([PersonId(40)]),
            started: facts.now,
            accepted: std::collections::BTreeMap::new(),
            episode: Some(Episode {
                actor,
                starts: facts.now,
                ends: SimTime(420000),
            }),
            next_loss_burden: 1,
            next_controller: facts.now,
            dominant: None,
        };
        store
            .insert_or_spawn(
                game_ecs::StableId(actor.0),
                ControlSession {
                    actor,
                    active: Some(active),
                    activation: None,
                    warning: None,
                },
            )
            .unwrap();
        assert!(control.available_actions(&store, &facts).is_empty());
        assert_eq!(control.actor(&store), Some(actor));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InvocationPreview {
    pub lives: BTreeSet<PersonId>,
    pub burden_rate: u64,
    pub activation_burden: u64,
    pub activation_fatigue: u64,
    pub borrowed_skills: Vec<(game_core::Skill, u16)>,
    pub next_cost_in: Option<game_core::SimDuration>,
}
impl ControlState {
    pub fn preview(
        &self,
        store: &Store,
        facts: &AdmissionFacts,
        lives: BTreeSet<PersonId>,
        mut historical_level: impl FnMut(PersonId, game_core::Skill) -> u16,
    ) -> Result<InvocationPreview, game_core::Error> {
        if !self.ready(facts.actor, facts)
            || lives.is_empty()
            || lives.len() > 8
            || !lives.is_subset(&facts.archived)
        {
            return Err(game_core::Error::Unavailable);
        }
        let charge = activation_charge(&self.rules, &Burden::default(), lives.len())?;
        let borrowed_skills = game_core::Skill::ALL
            .into_iter()
            .map(|skill| {
                (
                    skill,
                    crate::borrowed_level(lives.iter().map(|life| historical_level(*life, skill))),
                )
            })
            .collect();
        Ok(InvocationPreview {
            burden_rate: (lives.len() as u64).pow(2),
            activation_burden: charge.burden,
            activation_fatigue: charge.fatigue,
            next_cost_in: self.next_cost_for(store, facts.actor, lives.len()),
            lives,
            borrowed_skills,
        })
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use game_core::{Error, SimTime, Skill};
    #[test]
    fn preview_owns_eligibility_quadratic_costs_and_historical_mastery() {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        let store = Store::with_schema(schema);
        let control = ControlState::new(Rules::fixture()).unwrap();
        let facts = AdmissionFacts {
            actor: PersonId(10),
            now: SimTime(0),
            current: Some(PersonId(10)),
            alive: true,
            health: 100000,
            independent: true,
            learning: 100,
            required_learning: 100,
            fatigue: 0,
            fatigue_limit: 100000,
            archived: BTreeSet::from([PersonId(1), PersonId(2)]),
        };
        assert_eq!(
            control.preview(
                &store,
                &facts,
                BTreeSet::from([PersonId(3)]),
                |_, _| panic!("unknown archive must be rejected before access")
            ),
            Err(Error::Unavailable)
        );
        let result = control
            .preview(&store, &facts, facts.archived.clone(), |life, skill| {
                if skill == Skill::Craft {
                    if life == PersonId(1) { 100 } else { 50 }
                } else {
                    0
                }
            })
            .unwrap();
        assert_eq!(
            (
                result.burden_rate,
                result.activation_burden,
                result.activation_fatigue
            ),
            (4, 24000, 4000)
        );
        assert_eq!(
            result
                .borrowed_skills
                .iter()
                .find(|(skill, _)| *skill == Skill::Craft)
                .unwrap()
                .1,
            125
        );
        assert_eq!(result.lives, facts.archived);
        assert!(result.next_cost_in.is_some());
    }
}
