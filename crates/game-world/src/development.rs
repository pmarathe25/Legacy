//! Births require chosen actions by two adults. Kinship supplies a relationship,
//! not shared ownership or mandatory care; actual care records the supporting people.
use crate::*;

use game_lifecycle::{Attempt, BodyFacts, Consent, Effect as LifecycleEffect, ParentChoiceInput};
pub use game_lifecycle::{Life, Pregnancy, PregnancyState, Reproduction, Rules, World};

impl Simulation {
    pub(crate) fn initialize_life_people(
        &mut self,
        rules: Rules,
        lives: BTreeMap<PersonId, Life>,
    ) -> Result<(), Error> {
        if self.state.development.is_some()
            || !rules.valid()
            || lives.len() != self.state.people.len()
            || self.state.people.iter().any(|p| !lives.contains_key(&p.id))
        {
            return Err(Error::InvalidConfiguration("invalid initial lives".into()));
        }
        self.state.development = Some(World {
            rules,
            lives,
            pregnancies: BTreeMap::new(),
        });
        for actor in self.state.people.iter().map(|p| p.id).collect::<Vec<_>>() {
            self.schedule_age(actor);
        }
        Ok(())
    }
    pub fn life_fixture(seed: u64, execution: Execution) -> Result<Self, Error> {
        let mut sim = Self::autonomy_fixture(seed, execution)?;
        sim.state.scenario = scenarios().remove(3);
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
        let first = sim.state.people[3].id;
        let second = sim.state.people[4].id;
        for (id, partner) in [(first, second), (second, first)] {
            let mind = sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&id)
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
        let predecessor = sim.state.ruler;
        sim.debug_injure(
            predecessor,
            sim.state.physical.as_ref().unwrap().rules.max_health,
        )?;
        sim.advance(SimDuration(300_000))?;
        let child = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .lives
            .values()
            .find(|l| l.born == SimTime(300_000) && !l.parents.is_empty())
            .ok_or_else(|| Error::InvalidFixture("fixture did not produce its child".into()))?
            .id;
        // Explicit scenario control chooses this actual post-death newborn. This
        // fixture does not establish the game's unresolved succession policy.
        sim.state.people[sim.person_index[&predecessor]].role = Role::Citizen;
        sim.state.people[sim.person_index[&child]].role = Role::Ruler;
        sim.state.ruler = child;
        sim.state.scenario = scenarios().remove(5);
        Ok(sim)
    }

    pub(crate) fn independent(&self, id: PersonId) -> bool {
        self.state.development.as_ref().is_none_or(|w| {
            w.lives
                .get(&id)
                .is_some_and(|l| !matches!(l.stage, LifeStage::Infant))
        })
    }

    fn willing_parent(&self, id: PersonId, partner: PersonId) -> bool {
        if !self.social_family_intent(id, partner) {
            return false;
        }
        let Some(mind) = self.state.decisions.as_ref().and_then(|w| w.minds.get(&id)) else {
            return false;
        };
        let body = &self.state.physical.as_ref().unwrap().actors[&id];
        let Some(life) = self
            .state
            .development
            .as_ref()
            .and_then(|world| world.lives.get(&id))
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
            available: matches!(body.activity, Activity::Idle | Activity::Rest),
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
            .and_then(|index| self.state.people.get(*index))
            .ok_or(Error::Unavailable)?;
        let body = self
            .state
            .physical
            .as_ref()
            .and_then(|world| world.actors.get(&actor))
            .ok_or(Error::Unavailable)?;
        Ok(BodyFacts {
            actor,
            alive: body.vitals.alive,
            health: body.vitals.health,
            position: person.location.clone(),
            action_restricted: self.combat_restricts(actor),
            attempt: match body.activity {
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
        let first = self.lifecycle_body(actor)?;
        let second = self.lifecycle_body(partner)?;
        let world = self.state.development.as_ref().ok_or(Error::Unavailable)?;
        let id = PregnancyId(self.state.next_id);
        let prepared = world.prepare_start_conception(
            self.state.time,
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
            self.revision += 1;
            return Ok(());
        }
        prepared.validate(self.state.development.as_ref().unwrap())?;
        let allocated = PregnancyId(allocate(&mut self.state)?);
        assert_eq!(allocated, id);
        self.interrupt_actor_activity(actor);
        self.interrupt_actor_activity(partner);
        let effects = prepared.commit(self.state.development.as_mut().unwrap())?;
        for effect in effects {
            if let LifecycleEffect::AttemptStarted {
                pregnancy,
                parents,
                completes,
            } = effect
            {
                self.pregnancy_index.insert(parents[0], pregnancy);
                for person in [actor, partner] {
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&person)
                        .unwrap()
                        .activity = Activity::Conceive {
                        pregnancy,
                        completes,
                    };
                    self.state
                        .due
                        .entry(completes)
                        .or_default()
                        .push(Due::Physical(person));
                }
            }
        }
        self.revision += 1;
        Ok(())
    }

    pub(crate) fn review_conception(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.development else {
            return Ok(());
        };
        let mut pairs = BTreeSet::new();
        for id in world.lives.keys().filter(|id| **id != self.state.ruler) {
            let mind = &self.state.decisions.as_ref().unwrap().minds[id];
            if mind.dream != decisions::Aspiration::Parenthood {
                continue;
            }
            for partner in mind
                .relationships
                .keys()
                .filter(|p| **p != self.state.ruler)
            {
                if self.willing_parent(*id, *partner) && self.willing_parent(*partner, *id) {
                    pairs.insert(((*id).min(*partner), (*id).max(*partner)));
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
        let pregnancy = &self.state.development.as_ref().unwrap().pregnancies[&id];
        let pair = [pregnancy.gestator, pregnancy.partner];
        let bodies = [
            self.lifecycle_body(pair[0]).unwrap(),
            self.lifecycle_body(pair[1]).unwrap(),
        ];
        let prepared = self
            .state
            .development
            .as_ref()
            .unwrap()
            .prepare_complete_conception(id, self.state.time, [&bodies[0], &bodies[1]])
            .expect("preflighted coupled conception transition");
        let effects = prepared
            .commit(self.state.development.as_mut().unwrap())
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
                    self.state
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
        let Some(world) = &self.state.development else {
            return Ok(());
        };
        let deadlines = due.iter().filter_map(|event| match event {
            Due::Physical(actor) => {
                match self.state.physical.as_ref().unwrap().actors[actor].activity {
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
        world.preflight_deadlines(next, deadlines)?;
        let births = due.iter().filter(|d| matches!(d, Due::Birth(_))).count() as u64;
        self.state
            .next_id
            .checked_add(
                births
                    .checked_mul(2)
                    .ok_or(Error::Overflow)?
                    .checked_add(self.state.people.len() as u64 / 2)
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    pub(crate) fn complete_development(&mut self, due: Due) {
        match due {
            Due::Age(id) => {
                let alive = self.state.physical.as_ref().unwrap().actors[&id]
                    .vitals
                    .alive;
                let prepared = self
                    .state
                    .development
                    .as_ref()
                    .unwrap()
                    .prepare_age(
                        id,
                        self.state.time,
                        alive,
                        self.invocation_lifespan_loss(id),
                        self.state.people[self.person_index[&id]].role != Role::Guardian,
                    )
                    .expect("preflighted age transition");
                let effects = prepared
                    .commit(self.state.development.as_mut().unwrap())
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
                                self.state
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
        let pregnancy = self.state.development.as_ref().unwrap().pregnancies[&id].clone();
        let body = self
            .lifecycle_body(pregnancy.gestator)
            .expect("existing biological gestator body");
        let child = PersonId(self.state.next_id);
        let prepared = self
            .state
            .development
            .as_ref()
            .unwrap()
            .prepare_birth(id, self.state.time, child, &body, self.state.seed)
            .expect("preflighted actual birth transition");
        if prepared.effects().is_empty() {
            return;
        }
        self.pregnancy_index.remove(&pregnancy.gestator);
        if let [LifecycleEffect::PregnancyEnded(effect)] = prepared.effects() {
            let pregnancy = effect.pregnancy;
            prepared
                .commit(self.state.development.as_mut().unwrap())
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
            PersonId(allocate(&mut self.state).expect("preflight child ID")),
            child
        );
        let inventory = InventoryId(allocate(&mut self.state).expect("preflight inventory ID"));
        prepared
            .commit(self.state.development.as_mut().unwrap())
            .expect("immediately prepared lifecycle transition");
        let index = self.state.people.len();
        self.state.people.push(Person {
            id: child,
            role: Role::Citizen,
            location: location.clone(),
            inventory,
            work_units: 0,
            work_remainder: 0,
            work_rate: 1_000,
            route: Vec::new(),
            hunger_exempt: false,
        });
        if let Location::InTransit { arrives, .. } = location {
            self.state
                .due
                .entry(arrives)
                .or_default()
                .push(Due::Arrive(child));
        }
        self.person_index.insert(child, index);
        if let Some(world) = &mut self.state.combat {
            world
                .profiles
                .insert(child, combat::Profile { initiative: 100 });
        }
        let cargo = InventoryLocation::Carried(child);
        self.inventory_index
            .entry(cargo.clone())
            .or_default()
            .push(inventory);
        self.state.inventories.insert(
            inventory,
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
        );
        let max_health = self.state.physical.as_ref().unwrap().rules.max_health;
        self.state.physical.as_mut().unwrap().actors.insert(
            child,
            physical::Actor {
                fatigue_remainder: 0,
                health_remainder: 0,
                vitals: Vitals {
                    hunger: 0,
                    fatigue: 0,
                    health: max_health,
                    alive: true,
                },
                activity: Activity::Idle,
                job: None,
            },
        );
        let mut mind = self.state.decisions.as_ref().unwrap().minds[&pregnancy.gestator].clone();
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
        mind.retry_after = self.state.time;
        mind.goal = decisions::Goal::Survival;
        mind.explanation = decisions::Explanation {
            goal: decisions::Goal::Survival,
            need: 0,
            benefit: 0,
            value: 0,
            total: 0,
            blocker: None,
        };
        self.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .insert(child, mind);
        // Kinship changes a directed initial relationship. It grants no assets,
        // permissions, office, compulsory care, or legal guardianship.
        for parent in parents {
            if self.state.people[self.person_index[&parent]].location
                != self.state.people[self.person_index[&child]].location
            {
                continue;
            }
            let parent_mind = self
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&parent)
                .unwrap();
            parent_mind.relationships.insert(
                child,
                decisions::Relationship {
                    trust: 0,
                    affection: i32::from(parent_mind.personality.empathy),
                },
            );
        }
        if let Location::AtArea(area) = self.state.people[self.person_index[&child]].location {
            self.index_dependent(child, area);
        }
        self.record_genetic_birth(id, child)
            .expect("frozen conception genome for actual birth");
        self.transport_birth(child, pregnancy.gestator);
        self.state
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
        if self
            .state
            .development
            .as_ref()
            .is_some_and(|w| matches!(w.lives[&id].stage, LifeStage::Infant | LifeStage::Child))
        {
            self.dependent_index.entry(area).or_default().insert(id);
        }
    }

    pub(crate) fn schedule_age(&mut self, id: PersonId) {
        if self.state.people[self.person_index[&id]].role == Role::Guardian
            || !self.state.physical.as_ref().unwrap().actors[&id]
                .vitals
                .alive
        {
            return;
        }
        let world = self.state.development.as_ref().unwrap();
        if let Some(at) = world.lives[&id]
            .next_age_deadline(
                &world.rules,
                self.state.time,
                self.invocation_lifespan_loss(id),
                true,
            )
            .expect("validated life age arithmetic")
        {
            self.state.due.entry(at).or_default().push(Due::Age(id));
        }
    }

    pub(crate) fn end_life(&mut self, id: PersonId) {
        let a = self
            .state
            .physical
            .as_mut()
            .unwrap()
            .actors
            .get_mut(&id)
            .unwrap();
        if !a.vitals.alive {
            return;
        }
        a.vitals.alive = false;
        a.vitals.health = 0;
        a.job = None;
        self.transport_death(id);
        self.interrupt_actor_activity(id);
        self.state.people[self.person_index[&id]].route.clear();
        for due in self.state.due.values_mut() {
            due.retain(|d| !matches!(d, Due::Age(p) | Due::Arrive(p) if *p == id));
        }
        self.state.due.retain(|_, due| !due.is_empty());
        for people in self.dependent_index.values_mut() {
            people.remove(&id);
        }
        self.emit(EventKind::Died { actor: id });
        self.invalidate_pregnancies(id, true);
        self.observe_lineage_deaths();
    }

    pub(crate) fn invalidate_pregnancies(&mut self, actor: PersonId, died: bool) {
        if self.state.development.is_none() {
            return;
        }
        // The host's derived index selects affected records; lifecycle rules
        // remain inside the character domain, without scanning old pregnancies
        // whenever any person changes a physical activity.
        let mut ids = BTreeSet::new();
        if let Activity::Conceive { pregnancy, .. } =
            self.state.physical.as_ref().unwrap().actors[&actor].activity
        {
            ids.insert(pregnancy);
        }
        if died && let Some(pregnancy) = self.pregnancy_index.get(&actor) {
            ids.insert(*pregnancy);
        }
        let prepared = self
            .state
            .development
            .as_ref()
            .unwrap()
            .prepare_interruption(actor, died, ids);
        let ended = prepared
            .commit(self.state.development.as_mut().unwrap())
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
                    self.state
                        .physical
                        .as_mut()
                        .unwrap()
                        .actors
                        .get_mut(&person)
                        .unwrap()
                        .activity = Activity::Idle;
                    for due in self.state.due.values_mut() {
                        due.retain(|d| !matches!(d, Due::Physical(p) if *p == person));
                    }
                }
            }
            for due in self.state.due.values_mut() {
                due.retain(|d| !matches!(d, Due::Birth(p) if *p == id));
            }
            self.state.due.retain(|_, due| !due.is_empty());
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
            .state
            .decisions
            .as_mut()
            .and_then(|w| w.minds.get_mut(&actor))
            .ok_or(Error::Unavailable)?;
        mind.relationships.insert(other, relationship);
        Ok(())
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.development else {
        return Ok(());
    };
    let fail = || Error::InvalidSave("inconsistent development bindings".into());
    let physical = state.physical.as_ref().ok_or_else(fail)?;
    let mut bodies = BTreeMap::new();
    for person in &state.people {
        let body = physical.actors.get(&person.id).ok_or_else(fail)?;
        let attempt = match body.activity {
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
                    position: person.location.clone(),
                    action_restricted: false,
                    attempt,
                },
                aging_enabled: person.role != Role::Guardian,
                lifespan_loss: state
                    .invocation
                    .as_ref()
                    .and_then(|world| world.people.get(&person.id))
                    .map_or(0, |burden| burden.lifespan_loss),
            },
        );
    }
    let deadlines: Vec<_> = state
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
    world.validate_with_facts(state.time, &bodies, &deadlines)
}
