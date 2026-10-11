//! Births require chosen actions by two adults. Kinship supplies a relationship,
//! not shared ownership or mandatory care; actual care records the supporting people.
use crate::*;

use game_lifecycle::{Attempt, BodyFacts, Consent, Effect as LifecycleEffect, ParentChoiceInput};
pub use game_lifecycle::{Life, Pregnancy, PregnancyState, Reproduction, Rules, Runtime};

impl Simulation {
    pub(crate) fn initialize_life_people(
        &mut self,
        rules: Rules,
        lives: BTreeMap<PersonId, Life>,
    ) -> Result<(), Error> {
        if self.host.state.development.is_some()
            || !rules.valid()
            || lives.len() != self.host.state.people.len()
            || self
                .host
                .state
                .people
                .iter()
                .any(|p| !lives.contains_key(&p.id))
        {
            return Err(Error::InvalidConfiguration("invalid initial lives".into()));
        }
        if game_lifecycle::all_lives(&self.host.storage)
            .next()
            .is_some()
            || game_lifecycle::all_pregnancies(&self.host.storage)
                .next()
                .is_some()
        {
            return Err(Error::InvalidConfiguration(
                "initial lifecycle rows already exist".into(),
            ));
        }
        let mut batch = game_ecs::Batch::new();
        for (id, life) in lives {
            if id != life.id {
                return Err(Error::InvalidConfiguration(
                    "initial life identity mismatch".into(),
                ));
            }
            life.age_at(self.host.clock().time)?;
            batch.insert(game_ecs::StableId(id.0), life);
        }
        self.host.storage.apply_batch(batch)?;
        self.host.state.development = Some(Runtime { rules });
        for actor in self
            .host
            .state
            .people
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
        {
            self.schedule_age(actor);
        }
        Ok(())
    }
    pub fn life_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::autonomy_fixture(seed, execution)?;
        sim.host.state.scenario = scenarios().remove(3);
        // Accelerated biological rates isolate transitions; these are explicitly
        // distinct from the configurable human setting returned by Rules::human.
        let rules = Rules {
            conception_time: MINUTE,
            gestation: SimDuration(180_000),
            infancy: SimDuration(300_000),
            adulthood: SimDuration(600_000),
            elderhood: SimDuration(1_200_000),
            lifespan: SimDuration(1_500_000),
        };
        let lives = sim
            .host
            .state
            .people
            .iter()
            .enumerate()
            .map(|(i, p)| {
                (
                    p.id,
                    Life {
                        id: p.id,
                        born: SimTime(0),
                        initial_age: rules.adulthood,
                        stage: LifeStage::Adult,
                        reproduction: if p.role == Role::Guardian {
                            Reproduction::Neither
                        } else if i % 2 == 0 {
                            Reproduction::SuppliesSeed
                        } else {
                            Reproduction::Gestates
                        },
                        parents: Vec::new(),
                        caregivers: BTreeSet::new(),
                        preference: None,
                    },
                )
            })
            .collect();
        sim.initialize_life_people(rules, lives)?;
        let first = sim.host.state.people[3].id;
        let second = sim.host.state.people[4].id;
        for (id, partner) in [(first, second), (second, first)] {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                id,
            )
            .unwrap();
            mind.dream = decisions::Aspiration::Parenthood;
            mind.personality.empathy = 900;
            mind.relationships.insert(
                partner,
                decisions::Relationship {
                    affection: 800,
                    trust: 800,
                },
            );
        }
        Ok(sim)
    }

    pub fn childhood_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::life_fixture(seed, execution)?;
        let predecessor = sim.host.state.ruler;
        sim.debug_injure(
            predecessor,
            sim.host.state.physical.as_ref().unwrap().rules.max_health,
        )?;
        sim.advance(SimDuration(300_000))?;
        let child = game_lifecycle::all_lives(&sim.host.storage)
            .map(|(_, life)| life)
            .find(|l| l.born == SimTime(300_000) && !l.parents.is_empty())
            .ok_or_else(|| Error::InvalidFixture("fixture did not produce its child".into()))?
            .id;
        // Explicit scenario control chooses this actual post-death newborn. This
        // fixture does not establish the game's unresolved succession policy.
        sim.host.state.people[sim.person_index[&predecessor]].role = Role::Citizen;
        sim.host.state.people[sim.person_index[&child]].role = Role::Ruler;
        sim.host.state.ruler = child;
        sim.host.state.scenario = scenarios().remove(5);
        Ok(sim)
    }

    pub(crate) fn independent(&self, id: PersonId) -> bool {
        self.host.state.development.as_ref().is_none_or(|_| {
            game_lifecycle::life(&self.host.storage, id)
                .is_some_and(|l| !matches!(l.stage, LifeStage::Infant))
        })
    }

    fn willing_parent(&self, id: PersonId, partner: PersonId) -> bool {
        if !self.social_family_intent(id, partner) {
            return false;
        }
        let Some(mind) = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, id)
        }) else {
            return false;
        };
        let body = &game_physiology::body(&self.host.storage, id).expect("body exists");
        let Some(life) = self
            .host
            .state
            .development
            .as_ref()
            .and_then(|_| game_lifecycle::life(&self.host.storage, id))
        else {
            return false;
        };
        let relationship = mind.relationships.get(&partner);
        game_lifecycle::willing_parent(ParentChoiceInput {
            stage: life.stage,
            alive: body.vitals.alive,
            health: body.vitals.health,
            hunger: body.vitals.hunger,
            meal_relief: self.body_rules(id).meal_relief,
            available: matches!(
                *self.host.storage.activity(id).expect("activity exists"),
                Activity::Idle | Activity::Rest
            ),
            wants_parenthood: mind.dream == decisions::Aspiration::Parenthood,
            partner_affection: relationship.map_or(0, |relation| relation.affection),
            partner_trust: relationship.map_or(0, |relation| relation.trust),
            knows_adequate_food: mind
                .knowledge
                .values()
                .any(|fact| fact.good == Good::Food && fact.accessible && fact.quantity.0 >= 3_000),
        })
    }

    fn lifecycle_body(&self, actor: PersonId) -> Result<BodyFacts, Error> {
        let person = self
            .person_index
            .get(&actor)
            .and_then(|index| self.host.state.people.get(*index))
            .ok_or(Error::Unavailable)?;
        let body = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor))
            .ok_or(Error::Unavailable)?;
        Ok(BodyFacts {
            actor,
            alive: body.vitals.alive,
            health: body.vitals.health,
            position: game_spatial::location(&self.host.storage, game_ecs::StableId((person.id).0))
                .ok_or(Error::Unavailable)?
                .clone(),
            action_restricted: self.combat_restricts(actor),
            attempt: match *self
                .host
                .storage
                .activity(actor)
                .ok_or(Error::Unavailable)?
            {
                Activity::Conceive {
                    pregnancy,
                    completes,
                } => Some(Attempt {
                    pregnancy,
                    completes,
                }),
                _ => None,
            },
        })
    }

    pub(crate) fn start_conception(
        &mut self,
        actor: PersonId,
        partner: PersonId,
    ) -> Result<(), Error> {
        self.host.preflight_records(0, 0, 1)?;
        let first = self.lifecycle_body(actor)?;
        let second = self.lifecycle_body(partner)?;
        let world = self
            .host
            .state
            .development
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let id = PregnancyId(self.host.next_id());
        let prepared = world.prepare_start_conception(
            &self.host.storage,
            self.host.clock().time,
            id,
            [&first, &second],
            Consent {
                initiator: true,
                partner: self.willing_parent(partner, actor),
            },
        )?;
        self.ensure_event_capacity(1)?;
        if let [LifecycleEffect::Refused { initiator, partner }] = prepared.effects() {
            self.emit(EventKind::ConceptionRefused {
                actor: *initiator,
                partner: *partner,
            });
            self.host.bump_revision(1)?;
            return Ok(());
        }
        prepared.validate(
            self.host.state.development.as_ref().unwrap(),
            &self.host.storage,
        )?;
        let allocated = PregnancyId(self.host.allocate_id()?.0);
        assert_eq!(allocated, id);
        self.interrupt_actor_activity(actor);
        self.interrupt_actor_activity(partner);
        let effects = prepared.commit(
            self.host.state.development.as_ref().unwrap(),
            &mut self.host.storage,
        )?;
        for effect in effects {
            if let LifecycleEffect::AttemptStarted {
                pregnancy,
                parents,
                completes,
            } = effect
            {
                self.pregnancy_index.insert(parents[0], pregnancy);
                for person in [actor, partner] {
                    *self.host.storage.activity_mut(person).unwrap() = Activity::Conceive {
                        pregnancy,
                        completes,
                    };
                    self.host
                        .state
                        .due
                        .entry(completes)
                        .or_default()
                        .push(Due::Physical(person));
                }
            }
        }
        self.host.bump_revision(1)?;
        Ok(())
    }

    pub(crate) fn review_conception(&mut self) -> Result<(), Error> {
        let Some(_) = &self.host.state.development else {
            return Ok(());
        };
        let mut pairs = BTreeSet::new();
        for (id, _) in game_lifecycle::all_lives(&self.host.storage)
            .filter(|(id, _)| *id != self.host.state.ruler)
        {
            let mind =
                &game_characters::decisions::mind::<game_api::Command>(&self.host.storage, id)
                    .expect("mind exists");
            if mind.dream != decisions::Aspiration::Parenthood {
                continue;
            }
            for partner in mind
                .relationships
                .keys()
                .filter(|p| **p != self.host.state.ruler)
            {
                if self.willing_parent(id, *partner) && self.willing_parent(*partner, id) {
                    pairs.insert((id.min(*partner), id.max(*partner)));
                }
            }
        }
        for (a, b) in pairs {
            if self.willing_parent(a, b) && self.willing_parent(b, a) {
                match self.start_conception(a, b) {
                    Ok(_) | Err(Error::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }

    pub(crate) fn complete_conception(&mut self, id: PregnancyId) {
        let pregnancy = &game_lifecycle::pregnancy(&self.host.storage, id).unwrap();
        let pair = [pregnancy.gestator, pregnancy.partner];
        let bodies = [
            self.lifecycle_body(pair[0]).unwrap(),
            self.lifecycle_body(pair[1]).unwrap(),
        ];
        let prepared = self
            .host
            .state
            .development
            .as_ref()
            .unwrap()
            .prepare_complete_conception(
                &self.host.storage,
                id,
                self.host.clock().time,
                [&bodies[0], &bodies[1]],
            )
            .expect("preflighted coupled conception transition");
        let effects = prepared
            .commit(
                self.host.state.development.as_ref().unwrap(),
                &mut self.host.storage,
            )
            .expect("immediately prepared lifecycle transition");
        for person in pair {
            self.interrupt_actor_activity(person);
        }
        for effect in effects {
            match effect {
                LifecycleEffect::Conceived {
                    pregnancy,
                    parents,
                    due,
                } => {
                    self.freeze_genetic_conception(pregnancy)
                        .expect("validated biological parent genomes");
                    self.host
                        .state
                        .due
                        .entry(due)
                        .or_default()
                        .push(Due::Birth(pregnancy));
                    self.emit(EventKind::Conceived { pregnancy, parents });
                }
                LifecycleEffect::PregnancyEnded(effect) => {
                    self.pregnancy_index.remove(&effect.parents[0]);
                    self.emit(EventKind::PregnancyEnded {
                        pregnancy: effect.pregnancy,
                    });
                }
                _ => unreachable!("conception completion effect"),
            }
        }
    }

    pub(crate) fn preflight_development(&self, due: &[Due], next: SimTime) -> Result<(), Error> {
        let Some(world) = &self.host.state.development else {
            return Ok(());
        };
        let deadlines = due.iter().filter_map(|event| match event {
            Due::Physical(actor) => {
                match *self.host.storage.activity(*actor).expect("body exists") {
                    Activity::Conceive { pregnancy, .. } => {
                        Some(game_lifecycle::Deadline::Conception(pregnancy))
                    }
                    _ => None,
                }
            }
            Due::Birth(id) => Some(game_lifecycle::Deadline::Birth(*id)),
            Due::Age(id) => Some(game_lifecycle::Deadline::Age(*id)),
            _ => None,
        });
        world.preflight_deadlines(&self.host.storage, next, deadlines)?;
        let births = due.iter().filter(|d| matches!(d, Due::Birth(_))).count() as u64;
        self.host
            .next_id()
            .checked_add(
                births
                    .checked_mul(2)
                    .ok_or(Error::Overflow)?
                    .checked_add(self.host.state.people.len() as u64 / 2)
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    pub(crate) fn complete_development(&mut self, due: Due) {
        match due {
            Due::Age(id) => {
                let alive = game_physiology::body(&self.host.storage, id)
                    .expect("body exists")
                    .vitals
                    .alive;
                let prepared = self
                    .host
                    .state
                    .development
                    .as_ref()
                    .unwrap()
                    .prepare_age(
                        &self.host.storage,
                        id,
                        self.host.clock().time,
                        alive,
                        self.invocation_lifespan_loss(id),
                        self.host.state.people[self.person_index[&id]].role != Role::Guardian,
                    )
                    .expect("preflighted age transition");
                let effects = prepared
                    .commit(
                        self.host.state.development.as_ref().unwrap(),
                        &mut self.host.storage,
                    )
                    .expect("immediately prepared lifecycle transition");
                for effect in effects {
                    match effect {
                        LifecycleEffect::Death { actor } => self.end_life(actor),
                        LifecycleEffect::Developed(effect) => {
                            if effect.stage == LifeStage::Adult {
                                for people in self.dependent_index.values_mut() {
                                    people.remove(&effect.actor);
                                }
                            }
                            self.emit(EventKind::Developed {
                                actor: effect.actor,
                                stage: effect.stage,
                            });
                            if let Some(at) = effect.next_deadline {
                                self.host
                                    .state
                                    .due
                                    .entry(at)
                                    .or_default()
                                    .push(Due::Age(effect.actor));
                            }
                        }
                        _ => unreachable!("aging effect"),
                    }
                }
            }
            Due::Birth(id) => self.birth(id),
            _ => {}
        }
    }

    fn birth(&mut self, id: PregnancyId) {
        let pregnancy = game_lifecycle::pregnancy(&self.host.storage, id)
            .unwrap()
            .clone();
        let body = self
            .lifecycle_body(pregnancy.gestator)
            .expect("existing biological gestator body");
        let child = PersonId(self.host.next_id());
        let prepared = self
            .host
            .state
            .development
            .as_ref()
            .unwrap()
            .prepare_birth(
                &self.host.storage,
                id,
                self.host.clock().time,
                child,
                &body,
                self.host.seed(),
            )
            .expect("preflighted actual birth transition");
        if prepared.effects().is_empty() {
            return;
        }
        self.pregnancy_index.remove(&pregnancy.gestator);
        if let [LifecycleEffect::PregnancyEnded(effect)] = prepared.effects() {
            let pregnancy = effect.pregnancy;
            prepared
                .commit(
                    self.host.state.development.as_ref().unwrap(),
                    &mut self.host.storage,
                )
                .expect("immediately prepared lifecycle transition");
            self.emit(EventKind::PregnancyEnded { pregnancy });
            return;
        }
        let [
            LifecycleEffect::Born {
                position,
                parents,
                next_age,
                ..
            },
        ] = prepared.effects()
        else {
            unreachable!("birth effect")
        };
        let location = position.clone();
        let parents = *parents;
        let next_age = *next_age;
        assert_eq!(
            PersonId(self.host.allocate_id().expect("preflight child ID").0),
            child
        );
        let inventory = InventoryId(self.host.allocate_id().expect("preflight inventory ID").0);
        prepared
            .commit(
                self.host.state.development.as_ref().unwrap(),
                &mut self.host.storage,
            )
            .expect("immediately prepared lifecycle transition");
        let index = self.host.state.people.len();
        self.host.state.people.push(Person {
            id: child,
            role: Role::Citizen,
            inventory,
            work_units: 0,
            work_remainder: 0,
            work_rate: 1_000,
            route: Vec::new(),
            hunger_exempt: false,
        });
        game_spatial::set_location(
            &mut self.host.storage,
            game_ecs::StableId((child).0),
            location.clone(),
        )
        .expect("validated birth location");
        if let Location::InTransit { arrives, .. } = location {
            self.host
                .state
                .due
                .entry(arrives)
                .or_default()
                .push(Due::Arrive(child));
        }
        self.person_index.insert(child, index);
        if let Some(world) = &mut self.host.state.combat {
            world
                .profiles
                .insert(child, combat::Profile { initiative: 100 });
        }
        let cargo = InventoryLocation::Carried(child);
        self.inventory_index
            .entry(cargo.clone())
            .or_default()
            .push(inventory);
        game_cargo::insert_inventory(
            &mut self.host.storage,
            Inventory {
                id: inventory,
                location: cargo,
                quantity: Quantity(0),
                capacity: Quantity(10_000),
                access: BTreeSet::from([child]),
                last_granted: None,
                good: Good::Food,
                spoils_at: None,
            },
        )
        .expect("valid newborn inventory");
        let max_health = self.host.state.physical.as_ref().unwrap().rules.max_health;
        game_physiology::insert_body(
            &mut self.host.storage,
            child,
            game_physiology::Body {
                fatigue_remainder: 0,
                health_remainder: 0,
                vitals: Vitals {
                    hunger: 0,
                    fatigue: 0,
                    health: max_health,
                    alive: true,
                },
            },
        )
        .expect("validated newborn body");
        self.host
            .storage
            .set_activity(child, Activity::Idle)
            .expect("newborn entity exists");
        let mut mind = game_characters::decisions::mind::<game_api::Command>(
            &self.host.storage,
            pregnancy.gestator,
        )
        .expect("mind exists")
        .clone();
        mind.interests.clear();
        mind.routes.clear();
        mind.last_failure = None;
        mind.child_signals.clear();
        mind.knowledge.clear();
        mind.sites.clear();
        mind.values.clear();
        mind.relationships.clear();
        mind.dream = decisions::Aspiration::Comfort;
        mind.plan = None;
        mind.retry_after = self.host.clock().time;
        mind.goal = decisions::Goal::Survival;
        mind.explanation = decisions::Explanation {
            goal: decisions::Goal::Survival,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        };
        game_characters::decisions::insert_mind::<game_api::Command>(
            &mut self.host.storage,
            child,
            mind,
        )
        .expect("validated newborn mind");
        // Kinship changes a directed initial relationship. It grants no assets,
        // permissions, office, compulsory care, or legal guardianship.
        for parent in parents {
            if *game_spatial::location(
                &self.host.storage,
                game_ecs::StableId((self.host.state.people[self.person_index[&parent]].id).0),
            )
            .expect("location exists")
                != *game_spatial::location(
                    &self.host.storage,
                    game_ecs::StableId((self.host.state.people[self.person_index[&child]].id).0),
                )
                .expect("location exists")
            {
                continue;
            }
            let parent_mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut self.host.storage,
                parent,
            )
            .unwrap();
            parent_mind.relationships.insert(
                child,
                decisions::Relationship {
                    trust: 0,
                    affection: i32::from(parent_mind.personality.empathy),
                },
            );
        }
        if let Location::AtArea(area) = *game_spatial::location(
            &self.host.storage,
            game_ecs::StableId((self.host.state.people[self.person_index[&child]].id).0),
        )
        .expect("location exists")
        {
            self.index_dependent(child, area);
        }
        self.record_genetic_birth(id, child)
            .expect("frozen conception genome for actual birth");
        self.transport_birth(child, pregnancy.gestator);
        self.host
            .state
            .due
            .entry(next_age)
            .or_default()
            .push(Due::Age(child));
        self.emit(EventKind::Born {
            pregnancy: id,
            child,
            parents,
        });
        self.record_lineage_birth(child);
    }

    pub(crate) fn index_dependent(&mut self, id: PersonId, area: AreaId) {
        if self.host.state.development.as_ref().is_some_and(|_| {
            matches!(
                game_lifecycle::life(&self.host.storage, id).unwrap().stage,
                LifeStage::Infant | LifeStage::Child
            )
        }) {
            self.dependent_index.entry(area).or_default().insert(id);
        }
    }

    pub(crate) fn schedule_age(&mut self, id: PersonId) {
        if self.host.state.people[self.person_index[&id]].role == Role::Guardian
            || !game_physiology::body(&self.host.storage, id)
                .expect("body exists")
                .vitals
                .alive
        {
            return;
        }
        let world = self.host.state.development.as_ref().unwrap();
        if let Some(at) = game_lifecycle::life(&self.host.storage, id)
            .unwrap()
            .next_age_deadline(
                &world.rules,
                self.host.clock().time,
                self.invocation_lifespan_loss(id),
                true,
            )
            .expect("validated life age arithmetic")
        {
            self.host
                .state
                .due
                .entry(at)
                .or_default()
                .push(Due::Age(id));
        }
    }

    pub(crate) fn end_life(&mut self, id: PersonId) {
        let a = game_physiology::body_mut(&mut self.host.storage, id).unwrap();
        if !a.vitals.alive {
            return;
        }
        a.vitals.alive = false;
        a.vitals.health = 0;
        game_workshop::remove_job(&mut self.host.storage, id)
            .map(|_| ())
            .expect("actor exists");
        self.transport_death(id);
        self.interrupt_actor_activity(id);
        self.host.state.people[self.person_index[&id]].route.clear();
        for due in self.host.state.due.values_mut() {
            due.retain(|d| !matches!(d, Due::Age(p) | Due::Arrive(p) if *p == id));
        }
        self.host.state.due.retain(|_, due| !due.is_empty());
        for people in self.dependent_index.values_mut() {
            people.remove(&id);
        }
        self.emit(EventKind::Died { actor: id });
        self.invalidate_pregnancies(id, true);
        self.observe_lineage_deaths();
    }

    pub(crate) fn invalidate_pregnancies(&mut self, actor: PersonId, died: bool) {
        if self.host.state.development.is_none() {
            return;
        }
        // The host's derived index selects affected records; lifecycle rules
        // remain inside the character domain, without scanning old pregnancies
        // whenever any person changes a physical activity.
        let mut ids = BTreeSet::new();
        if let Activity::Conceive { pregnancy, .. } =
            *self.host.storage.activity(actor).expect("body exists")
        {
            ids.insert(pregnancy);
        }
        if died && let Some(pregnancy) = self.pregnancy_index.get(&actor) {
            ids.insert(*pregnancy);
        }
        let prepared = self
            .host
            .state
            .development
            .as_ref()
            .unwrap()
            .prepare_interruption(&self.host.storage, actor, died, ids);
        let ended = prepared
            .commit(
                self.host.state.development.as_ref().unwrap(),
                &mut self.host.storage,
            )
            .expect("immediately prepared lifecycle transition");
        for effect in ended {
            let LifecycleEffect::PregnancyEnded(effect) = effect else {
                unreachable!("pregnancy interruption effect")
            };
            let id = effect.pregnancy;
            let pair = effect.parents;
            let was_conceiving = effect.was_conceiving;
            self.pregnancy_index.remove(&pair[0]);
            if was_conceiving {
                for person in pair {
                    *self.host.storage.activity_mut(person).unwrap() = Activity::Idle;
                    for due in self.host.state.due.values_mut() {
                        due.retain(|d| !matches!(d, Due::Physical(p) if *p == person));
                    }
                }
            }
            for due in self.host.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Birth(p) if *p == id));
            }
            self.host.state.due.retain(|_, due| !due.is_empty());
            self.emit(EventKind::PregnancyEnded { pregnancy: id });
        }
    }

    pub fn debug_set_relationship(
        &mut self,
        actor: PersonId,
        other: PersonId,
        relationship: decisions::Relationship,
    ) -> Result<(), Error> {
        if !self.person_index.contains_key(&other)
            || !(-1_000..=1_000).contains(&relationship.affection)
            || !(-1_000..=1_000).contains(&relationship.trust)
        {
            return Err(Error::InvalidFixture("invalid relationship".into()));
        }
        let mind = self
            .host
            .state
            .decisions
            .as_ref()
            .and_then(|_| {
                game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut self.host.storage,
                    actor,
                )
            })
            .ok_or(Error::Unavailable)?;
        mind.relationships.insert(other, relationship);
        Ok(())
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.state.development else {
        if game_lifecycle::all_lives(state.components).next().is_some()
            || game_lifecycle::all_pregnancies(state.components)
                .next()
                .is_some()
        {
            return Err(Error::InvalidSave(
                "lifecycle components require lifecycle runtime".into(),
            ));
        }
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent development bindings".into());
    state.state.physical.as_ref().ok_or_else(fail)?;
    let mut bodies = BTreeMap::new();
    for person in &state.state.people {
        let body = game_physiology::body(state.components, person.id).ok_or_else(fail)?;
        let attempt = match *state
            .components
            .activity(person.id)
            .expect("activity exists")
        {
            Activity::Conceive {
                pregnancy,
                completes,
            } => Some(Attempt {
                pregnancy,
                completes,
            }),
            _ => None,
        };
        bodies.insert(
            person.id,
            game_lifecycle::ValidationBody {
                body: BodyFacts {
                    actor: person.id,
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                    position: (*game_spatial::location(
                        state.components,
                        game_ecs::StableId((person.id).0),
                    )
                    .expect("location exists"))
                    .clone(),
                    action_restricted: false,
                    attempt,
                },
                aging_enabled: person.role != Role::Guardian,
                lifespan_loss: state
                    .state
                    .invocation
                    .as_ref()
                    .and_then(|world| world.control.people.get(&person.id))
                    .map_or(0, |burden| burden.lifespan_loss),
            },
        );
    }
    let deadlines: Vec<_> = state
        .state
        .due
        .iter()
        .flat_map(|(at, events)| {
            events.iter().filter_map(|event| match event {
                Due::Age(actor) => Some((*at, game_lifecycle::Deadline::Age(*actor))),
                Due::Birth(id) => Some((*at, game_lifecycle::Deadline::Birth(*id))),
                _ => None,
            })
        })
        .collect();
    world.validate_with_facts(state.components, state.time, &bodies, &deadlines)
}
