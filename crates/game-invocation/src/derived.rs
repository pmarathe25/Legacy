//! Invocation interprets its native subject, historical benefits, and offers.
use crate::*;
use game_core::*;
use game_ecs::{Component, Store};

impl Runtime {
    pub fn social_penalty(&self, store: &Store, actor: PersonId) -> i64 {
        burden(store, actor).map_or(0, |cost| {
            i64::from(cost.emotional_disruption)
                + i64::from(cost.temporary_identity)
                + i64::from(cost.permanent_identity)
        })
    }
    pub fn invoked_personality<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
    ) -> Option<game_characters::decisions::Personality> {
        let active = self
            .control
            .active(store)
            .filter(|active| active.actor == actor)?;
        let life = active
            .dominant
            .or_else(|| active.lives.iter().next().copied())?;
        let empty = Burden::default();
        Some(controller_personality(
            &archive::<A>(store, life)?.mind.personality,
            burden(store, actor).unwrap_or(&empty),
        ))
    }
    pub fn borrowed_skill<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        skill: Skill,
    ) -> u16 {
        let Some(active) = self
            .control
            .active(store)
            .filter(|active| active.actor == actor)
        else {
            return 0;
        };
        borrowed_level(active.lives.iter().map(|life| {
            archive::<A>(store, *life)
                .and_then(|past| past.skills.iter().find(|entry| entry.skill == skill))
                .map_or(0, |entry| entry.level)
        }))
    }
    pub fn allocation_bound(&self, store: &Store, next: SimTime) -> u64 {
        self.memory_allocation_bound(store, next) + self.control.allocation_bound(store, next)
    }
    pub fn current_preview<A: Component>(
        &self,
        store: &Store,
        facts: &AdmissionFacts,
    ) -> Result<InvocationPreview, Error> {
        let lives = self
            .control
            .activation(store)
            .filter(|plan| plan.actor == facts.actor)
            .map(|plan| plan.lives.clone())
            .or_else(|| {
                self.control
                    .active(store)
                    .filter(|active| active.actor == facts.actor)
                    .map(|active| active.lives.clone())
            })
            .unwrap_or_default();
        if lives.is_empty() {
            return Ok(InvocationPreview {
                lives,
                burden_rate: 0,
                activation_burden: 0,
                activation_fatigue: 0,
                borrowed_skills: Vec::new(),
                next_cost_in: None,
            });
        }
        self.control.preview(store, facts, lives, |life, skill| {
            archive::<A>(store, life)
                .and_then(|past| past.skills.iter().find(|entry| entry.skill == skill))
                .map_or(0, |entry| entry.level)
        })
    }
    pub fn available_invocation_actions(
        &self,
        store: &Store,
        admission: &AdmissionFacts,
        memory: &impl MemoryFacts,
    ) -> Vec<InvocationAction> {
        let mut actions = self
            .control
            .available_actions(store, admission)
            .into_iter()
            .map(InvocationAction::from)
            .collect::<Vec<_>>();
        actions.extend(self.available_memory_actions(
            store,
            memory,
            admission.actor,
            admission.now,
        ));
        actions
    }
}
impl ControlState {
    pub fn allocation_bound(&self, store: &Store, next: SimTime) -> u64 {
        u64::from(self.active(store).is_some() || self.activation(store).is_some())
            + if self
                .active(store)
                .is_some_and(|active| active.episode.is_some() && active.next_controller <= next)
            {
                2
            } else {
                0
            }
    }
    pub fn prepare_observed_deaths(
        &self,
        store: &Store,
        now: SimTime,
        mut alive: impl FnMut(PersonId) -> bool,
    ) -> Result<PreparedControl, Error> {
        let dead = burdens(store)
            .map(|(actor, _)| actor)
            .chain(self.actor(store))
            .filter(|actor| !alive(*actor))
            .collect();
        self.prepare_deaths(store, &dead, now)
    }
    pub fn boundary_for(
        &self,
        store: &Store,
        now: SimTime,
        limit: SimTime,
        mut alive: impl FnMut(PersonId) -> bool,
    ) -> SimTime {
        let alive = burdens(store)
            .map(|(actor, _)| actor)
            .filter(|actor| alive(*actor))
            .collect();
        self.boundary(store, now, limit, &alive)
    }
    pub fn player_control(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        alive: bool,
    ) -> bool {
        alive
            && !controls_actor(
                self.active(store)
                    .and_then(|active| active.episode.as_ref()),
                actor,
                now,
                alive,
            )
    }
}
impl InvocationAction {
    pub fn control_action(&self) -> Option<ControlAction> {
        match self {
            Self::Activate { lives } => Some(ControlAction::Activate {
                lives: lives.clone(),
            }),
            Self::Dismiss => Some(ControlAction::Dismiss),
            Self::AcceptWarning { warning } => {
                Some(ControlAction::AcceptWarning { warning: *warning })
            }
            _ => None,
        }
    }
}
impl From<ControlAction> for InvocationAction {
    fn from(action: ControlAction) -> Self {
        match action {
            ControlAction::Activate { lives } => Self::Activate { lives },
            ControlAction::Dismiss => Self::Dismiss,
            ControlAction::AcceptWarning { warning } => Self::AcceptWarning { warning },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_start::tests::{ACTOR, LIFE, fixture};
    use game_ecs::StableId;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn owner_selects_personality_costs_preview_and_dead_subjects_from_native_rows() {
        let (runtime, mut store, _) = fixture();
        insert_burden(
            &mut store,
            ACTOR,
            Burden {
                emotional_disruption: 100,
                temporary_identity: 5,
                permanent_identity: 3,
                uninterrupted: 100,
                exposure: 100,
                lifespan_loss: 200,
                ..Burden::default()
            },
        )
        .unwrap();
        let first = archive::<u8>(&store, LIFE)
            .unwrap()
            .mind
            .personality
            .clone();
        assert_eq!(runtime.social_penalty(&store, ACTOR), 108);
        assert_eq!(
            runtime.invoked_personality::<u8>(&store, ACTOR),
            Some(controller_personality(
                &first,
                burden(&store, ACTOR).unwrap()
            ))
        );
        let second = PersonId(2);
        let mut past = archive::<u8>(&store, LIFE).unwrap().clone();
        past.person = second;
        past.mind.personality.sociability = 50;
        let second_personality = past.mind.personality.clone();
        store.insert_or_spawn(StableId(second.0), past).unwrap();
        let active = session_mut(&mut store, ACTOR)
            .unwrap()
            .active
            .as_mut()
            .unwrap();
        active.lives.insert(second);
        active.dominant = Some(second);
        assert_eq!(
            runtime.invoked_personality::<u8>(&store, ACTOR),
            Some(controller_personality(
                &second_personality,
                burden(&store, ACTOR).unwrap()
            ))
        );
        let facts = AdmissionFacts {
            actor: ACTOR,
            now: SimTime(0),
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
        assert_eq!(
            runtime
                .current_preview::<u8>(&store, &facts)
                .unwrap()
                .burden_rate,
            4
        );
        assert_eq!(runtime.allocation_bound(&store, SimTime(0)), 1);
        runtime
            .control
            .prepare_observed_deaths(&store, facts.now, |_| false)
            .unwrap()
            .commit(&runtime.control, &mut store)
            .unwrap();
        assert!(session(&store, ACTOR).is_none());
        assert_eq!(burden(&store, ACTOR).unwrap().lifespan_loss, 200);
        assert_eq!(burden(&store, ACTOR).unwrap().uninterrupted, 0);
        let preview = runtime.current_preview::<u8>(&store, &facts).unwrap();
        assert!(preview.lives.is_empty());
        assert_eq!(
            (
                preview.activation_fatigue,
                preview.activation_burden,
                preview.burden_rate
            ),
            (0, 0, 0)
        );
        assert_eq!(runtime.allocation_bound(&store, SimTime(0)), 0);
    }
    #[test]
    fn memory_offers_follow_the_native_control_episode_at_the_requested_time() {
        let (runtime, mut store, memory) = fixture();
        game_cargo::insert_inventory(
            &mut store,
            game_cargo::Inventory {
                id: InventoryId(40),
                good: Good::Record,
                quantity: Quantity(1_000),
                capacity: Quantity(5_000),
                location: game_cargo::InventoryLocation::Area(AreaId(3)),
                access: BTreeSet::from([ACTOR]),
                last_granted: None,
                spoils_at: None,
            },
        )
        .unwrap();
        store
            .insert_or_spawn(
                StableId(41),
                Record {
                    id: RecordId(41),
                    inventory: InventoryId(40),
                    author: ACTOR,
                    written_at: SimTime(0),
                    content: RecordedMemory::Route {
                        life: LIFE,
                        origin: AreaId(3),
                        destination: AreaId(4),
                        duration: SimDuration(1_000),
                    },
                    readers: BTreeSet::new(),
                    intact: true,
                },
            )
            .unwrap();
        let facts = AdmissionFacts {
            actor: ACTOR,
            now: SimTime(0),
            current: Some(ACTOR),
            alive: true,
            health: 100_000,
            independent: true,
            learning: 100,
            required_learning: 100,
            fatigue: 0,
            fatigue_limit: 100_000,
            archived: BTreeSet::from([LIFE]),
        };
        assert!(
            runtime
                .available_invocation_actions(&store, &facts, &memory)
                .iter()
                .any(|action| matches!(
                    action,
                    InvocationAction::ReadRecord {
                        record: RecordId(41)
                    }
                ))
        );
        let active = session_mut(&mut store, ACTOR)
            .unwrap()
            .active
            .as_mut()
            .unwrap();
        active.episode = Some(Episode {
            actor: ACTOR,
            starts: SimTime(0),
            ends: SimTime(100),
        });
        active.accepted = BTreeMap::from([(InvocationCost::Strain, Consent::default())]);
        assert!(
            runtime
                .available_invocation_actions(&store, &facts, &memory)
                .is_empty()
        );
        assert!(
            !runtime
                .control
                .player_control(&store, ACTOR, facts.now, true)
        );
        assert_eq!(runtime.allocation_bound(&store, SimTime(0)), 3);
        let later = AdmissionFacts {
            now: SimTime(100),
            ..facts
        };
        assert!(
            runtime
                .available_invocation_actions(&store, &later, &memory)
                .iter()
                .any(|action| matches!(action, InvocationAction::ReadRecord { .. }))
        );
    }
}

impl Runtime {
    pub fn controller_mind<'a, A: Component>(
        &self,
        store: &'a Store,
        actor: PersonId,
    ) -> Option<&'a game_characters::decisions::Mind<A>> {
        let active = self
            .control
            .active(store)
            .filter(|active| active.actor == actor)?;
        Some(&archive::<A>(store, active.dominant?)?.mind)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControllerSubject {
    pub actor: PersonId,
    pub life: PersonId,
    pub changed: bool,
}
impl PreparedControl {
    pub fn controller_subject(&self) -> Option<ControllerSubject> {
        self.effects.iter().find_map(|effect| match effect {
            ControlEffect::ControllerSelected {
                actor,
                life,
                changed,
            } => Some(ControllerSubject {
                actor: *actor,
                life: *life,
                changed: *changed,
            }),
            _ => None,
        })
    }
    pub fn controller_personality<A: Component>(
        &self,
        store: &Store,
    ) -> Option<game_characters::decisions::Personality> {
        let subject = self.controller_subject()?;
        let empty = Burden::default();
        Some(controller_personality(
            &archive::<A>(store, subject.life)?.mind.personality,
            burden(store, subject.actor).unwrap_or(&empty),
        ))
    }
}

impl Runtime {
    pub fn work_rate<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        skill: Skill,
        own: SkillRate,
        body: SkillRate,
        bonus_per_level: u16,
    ) -> Result<SkillRate, Error> {
        let empty = Burden::default();
        let borrowing = self
            .control
            .active(store)
            .is_some_and(|active| active.actor == actor);
        borrowed_work_rate(
            own,
            body,
            burden(store, actor).unwrap_or(&empty),
            borrowing.then(|| self.borrowed_skill::<A>(store, actor, skill)),
            bonus_per_level,
        )
    }
    pub fn combat_damage<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        base: u64,
        own: u16,
        bonus_per_level: u16,
    ) -> Result<u64, Error> {
        let empty = Burden::default();
        borrowed_combat_damage(
            base,
            own,
            self.borrowed_skill::<A>(store, actor, Skill::Combat),
            bonus_per_level,
            burden(store, actor).unwrap_or(&empty),
        )
    }
    pub fn routes_for<A: Component>(
        &self,
        store: &Store,
        actor: PersonId,
        current: &RouteMemory,
    ) -> Option<RouteMemory> {
        let active = self
            .control
            .active(store)
            .filter(|active| active.actor == actor)?;
        Some(recalled_routes(
            current,
            active
                .lives
                .iter()
                .filter_map(|id| archive::<A>(store, *id).map(|past| &past.mind.routes)),
        ))
    }
    pub fn preview<A: Component>(
        &self,
        store: &Store,
        facts: &AdmissionFacts,
        lives: std::collections::BTreeSet<PersonId>,
    ) -> Result<InvocationPreview, Error> {
        self.control.preview(store, facts, lives, |life, skill| {
            archive::<A>(store, life)
                .and_then(|past| past.skills.iter().find(|entry| entry.skill == skill))
                .map_or(0, |entry| entry.level)
        })
    }
    pub fn body_rates(
        &self,
        store: &Store,
        actor: PersonId,
        now: SimTime,
        alive: bool,
    ) -> Result<BodyRates, Error> {
        let empty = Burden::default();
        let active = self
            .control
            .active(store)
            .filter(|active| active.actor == actor)
            .map(|active| ActiveInput {
                life_count: active.lives.len(),
                accepted: &active.accepted,
                episode: active.episode.as_ref(),
                next_loss_burden: active.next_loss_burden,
            });
        body_rates(
            &self.control.rules,
            burden(store, actor).unwrap_or(&empty),
            active,
            actor,
            now,
            alive,
        )
    }
}
impl ControlState {
    pub fn controls(&self, store: &Store, actor: PersonId, now: SimTime, alive: bool) -> bool {
        controls_actor(
            self.active(store)
                .and_then(|active| active.episode.as_ref()),
            actor,
            now,
            alive,
        )
    }
}
