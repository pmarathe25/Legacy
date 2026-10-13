//! Births require chosen actions by two adults. Kinship supplies a relationship,
//! not shared ownership or mandatory care; actual care records the supporting people.
use crate::*;

use game_lifecycle::{Attempt, BodyFacts, Consent, Effect as LifecycleEffect, ParentChoiceInput};
pub use game_lifecycle::{Life, Pregnancy, PregnancyState, Reproduction, Rules, Runtime};

pub(crate) struct PreparedDeathCleanup {
    actor: PersonId,
    motion: crate::transport_motion::PreparedMotionAdmission,
    lineage: Option<game_lineage::PreparedLineage<game_lineage::Succession>>,
    job: game_workshop::PreparedJobCancellation,
    facts: game_ecs::ReadSet,
    gestating: BTreeSet<PregnancyId>,
    constraints: BTreeMap<PersonId, Option<game_lineage::GuardianGoal>>,
    current_alive: bool,
    witness: Option<crate::lineage::PreparedLineageWitness<game_lineage::Succession>>,
    gestation: Option<game_lifecycle::PreparedLifecycle>,
    genetic_endings: Vec<game_genetics::PreparedPregnancyEnd>,
}
pub(crate) struct StagedDeathCleanup {
    actor: PersonId,
    motion: crate::transport_motion::StagedMotionAdmission,
    lineage: Option<game_lineage::StagedLineage<game_lineage::Succession>>,
    guards: Vec<game_ecs::ReadGuard>,
    gestating: BTreeSet<PregnancyId>,
    witness: Option<crate::lineage::StagedLineageWitness<game_lineage::Succession>>,
    gestation: Option<game_lifecycle::LifecycleFinalization>,
}

impl PreparedDeathCleanup {
    pub(crate) fn receive_lineage_witness(
        mut self,
        sim: &Simulation,
        witness: crate::lineage::PreparedLineageWitness,
    ) -> Result<Self, Error> {
        self.witness = Some(witness.with_death(
            sim,
            self.current_alive,
            sim.host.clock().time,
            &self.constraints,
        )?);
        self.lineage = None;
        Ok(self)
    }
    /// The originating Body owner stages its lethal row first. Cleanup never
    /// republishes the Body, so combat and lifespan origins share this receiver.
    pub(crate) fn stage_into(
        self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
    ) -> Result<StagedDeathCleanup, Error> {
        self.stage_into_with_completion(sim, batch, None)
    }
    pub(crate) fn stage_into_with_completion(
        self,
        sim: &Simulation,
        batch: &mut game_ecs::Batch,
        completion: Option<&game_workshop::CompletedJobReceipt>,
    ) -> Result<StagedDeathCleanup, Error> {
        let witness = self
            .witness
            .map(|plan| plan.stage_into(sim, batch))
            .transpose()?;
        let lineage = self
            .lineage
            .map(|plan| {
                plan.stage_into(
                    sim.host.state.lineage.as_ref().ok_or(Error::Unavailable)?,
                    &sim.host.storage,
                    batch,
                )
            })
            .transpose()?;
        let motion =
            self.motion
                .stage_into(sim, batch, &BTreeMap::from([(self.actor, Activity::Idle)]))?;
        let gestation = self
            .gestation
            .map(|plan| {
                plan.stage_into(
                    sim.host
                        .state
                        .development
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &sim.host.storage,
                    batch,
                )
            })
            .transpose()?;
        let job = if let Some(completion) = completion {
            self.job
                .stage_into_with_completion(&sim.host.storage, batch, completion)?
        } else {
            self.job.stage_into(&sim.host.storage, batch)?
        };
        let mut guards = vec![job];
        for ending in self.genetic_endings {
            guards.push(ending.stage_guarded_into(&sim.host.storage, batch)?);
        }
        let mut admitted = game_ecs::ReadFootprint::new();
        admitted.protect::<game_physiology::Body>(game_ecs::StableId(self.actor.0));
        guards.push(self.facts.bind(&sim.host.storage, batch, &admitted)?);
        Ok(StagedDeathCleanup {
            actor: self.actor,
            motion,
            lineage,
            guards,
            gestating: self.gestating,
            witness,
            gestation,
        })
    }
}
impl StagedDeathCleanup {
    pub(crate) fn motion_finalization(&self) -> &game_transport::MotionFinalization {
        self.motion.motion_finalization()
    }
    pub(crate) fn validate(&self, sim: &Simulation, batch: &game_ecs::Batch) -> Result<(), Error> {
        let body = batch
            .projected::<game_physiology::Body>(
                &sim.host.storage,
                game_ecs::StableId(self.actor.0),
            )?
            .ok_or(Error::Unavailable)?;
        game_physiology::require_terminal_body(body)?;
        for guard in &self.guards {
            guard.validate(&sim.host.storage, batch)?;
        }
        self.motion.validate_batch(sim, batch)?;
        if let Some(lineage) = &self.lineage {
            lineage.validate(
                sim.host.state.lineage.as_ref().ok_or(Error::Unavailable)?,
                &sim.host.storage,
                batch,
            )?;
        }
        if let Some(witness) = &self.witness {
            witness.validate(sim, batch)?;
        }
        if let Some(gestation) = &self.gestation {
            gestation.validate(&sim.host.storage, batch)?;
        }
        Ok(())
    }
    pub(crate) fn guards(&self) -> impl Iterator<Item = game_ecs::ReadGuard> + '_ {
        self.guards
            .iter()
            .cloned()
            .chain(self.lineage.as_ref().map(|lineage| lineage.guard().clone()))
            .chain(
                self.witness
                    .as_ref()
                    .into_iter()
                    .flat_map(|witness| witness.row_guards()),
            )
    }
    pub(crate) fn events(&self) -> Vec<EventKind> {
        let (mut earlier, ended): (Vec<_>, Vec<_>) = self
            .motion
            .events()
            .into_iter()
            .partition(|event| !matches!(event, EventKind::PregnancyEnded { pregnancy } if self.gestating.contains(pregnancy)));
        earlier.push(EventKind::Died { actor: self.actor });
        earlier.extend(ended);
        earlier.extend(
            self.gestating
                .iter()
                .map(|id| EventKind::PregnancyEnded { pregnancy: *id }),
        );
        earlier
    }
    pub(crate) fn finish(self, sim: &mut Simulation) {
        self.motion.finish_metadata(sim);
        if let Some(lineage) = self.lineage {
            let _ = lineage.finish();
        }
        if let Some(witness) = self.witness {
            let _ = witness.finish();
        }
        if let Some(gestation) = self.gestation {
            let _ = gestation.finish();
        }
        if !self.gestating.is_empty() {
            sim.pregnancy_index.remove(&self.actor);
        }
        for due in sim.host.state.due.values_mut() {
            due.retain(|entry| !matches!(entry, Due::Age(actor) | Due::Arrive(actor) if *actor == self.actor));
            due.retain(|entry| !matches!(entry, Due::Birth(id) if self.gestating.contains(id)));
        }
        sim.host.state.due.retain(|_, due| !due.is_empty());
        for people in sim.dependent_index.values_mut() {
            people.remove(&self.actor);
        }
    }
}

impl Simulation {
    pub(crate) fn prepare_death_cleanup(
        &self,
        actor: PersonId,
    ) -> Result<PreparedDeathCleanup, Error> {
        let now = self.host.clock().time;
        let mut facts = game_ecs::ReadSet::new();
        facts.read::<game_physiology::Body>(&self.host.storage, game_ecs::StableId(actor.0));
        facts.read::<game_characters::people::Person>(
            &self.host.storage,
            game_ecs::StableId(actor.0),
        );
        facts.membership::<game_lifecycle::Pregnancy>(&self.host.storage, move |_, pregnancy| {
            pregnancy.gestator == actor
        });
        let constraints = game_lineage::guardians(&self.host.storage)
            .map(|(guardian, _)| {
                let id = game_ecs::StableId(guardian.0);
                facts.read::<game_physiology::Body>(&self.host.storage, id);
                facts.read::<game_conflict::Captivity>(&self.host.storage, id);
                let constraint = if guardian == actor {
                    game_lineage::guardian_constraint(
                        game_conflict::captivity(&self.host.storage, guardian).is_some(),
                        true,
                        self.combat_restricts(guardian),
                    )
                } else {
                    self.guardian_constraint(guardian)
                };
                (guardian, constraint)
            })
            .collect();
        let current_alive = self.host.state.lineage.as_ref().is_some_and(|owner| {
            owner.current(&self.host.storage).is_some_and(|current| {
                current != actor
                    && facts
                        .read::<game_physiology::Body>(
                            &self.host.storage,
                            game_ecs::StableId(current.0),
                        )
                        .is_some_and(|body| body.vitals.alive)
            })
        });
        let lineage = self
            .host
            .state
            .lineage
            .as_ref()
            .map(|owner| owner.prepare_death(&self.host.storage, current_alive, now, &constraints))
            .transpose()?;
        let fallback = self.transport_default_runtime();
        let owner = self.host.state.transport.as_ref().unwrap_or(&fallback);
        let native = owner.prepare_death_motion(&self.host.storage, actor, now)?;
        let motion = self.prepare_death_motion_admission(
            crate::transport_motion::MotionPlan::Direct(native),
            game_transport::MotionScope::Ordinary,
            actor,
        )?;
        let gestation = self
            .host
            .state
            .development
            .as_ref()
            .map(|owner| owner.prepare_gestation_death(&self.host.storage, actor));
        let gestating: BTreeSet<_> = gestation
            .as_ref()
            .into_iter()
            .flat_map(|plan| plan.effects())
            .filter_map(|effect| {
                if let LifecycleEffect::PregnancyEnded(effect) = effect {
                    Some(effect.pregnancy)
                } else {
                    None
                }
            })
            .collect();
        let genetic_endings = self
            .host
            .state
            .genetics
            .as_ref()
            .map(|owner| {
                gestating
                    .iter()
                    .map(|id| owner.prepare_end_pregnancy(&self.host.storage, *id, now))
                    .collect()
            })
            .unwrap_or_default();
        Ok(PreparedDeathCleanup {
            actor,
            motion,
            lineage,
            job: game_workshop::prepare_cancel(&self.host.storage, actor),
            facts,
            gestating,
            constraints,
            current_alive,
            witness: None,
            gestation,
            genetic_endings,
        })
    }
    pub(crate) fn initialize_life_people(
        &mut self,
        rules: Rules,
        lives: BTreeMap<PersonId, Life>,
    ) -> Result<(), Error> {
        if self.host.state.development.is_some()
            || !rules.valid()
            || lives.len() != self.people().len()
            || self.people().any(|p| !lives.contains_key(&p.id))
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
        for actor in self.people().map(|p| p.id).collect::<Vec<_>>() {
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
            .people()
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
        let first = sim.person_at(3).id;
        let second = sim.person_at(4).id;
        for (id, partner) in [(first, second), (second, first)] {
            let (mind,_stocks)=sim.host.storage.get_pair_mut::<game_characters::decisions::Mind<game_api::Command>,game_characters::decisions::Stocks>(game_ecs::StableId((id).0)).expect("mind and stock knowledge exist");
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
        game_characters::people::prepare_roles(
            &sim.host.storage,
            [
                game_characters::people::RoleChange {
                    actor: predecessor,
                    role: Role::Citizen,
                },
                game_characters::people::RoleChange {
                    actor: child,
                    role: Role::Ruler,
                },
            ],
        )?
        .commit(&mut sim.host.storage)?;
        sim.host.state.ruler = child;
        sim.host.state.scenario = scenarios().remove(5);
        Ok(sim)
    }

    pub(crate) fn independent(&self, id: PersonId) -> bool {
        game_lifecycle::independent(
            game_lifecycle::life(&self.host.storage, id).map(|life| life.stage),
            self.host.state.development.is_some(),
        )
    }

    fn parent_choice_facts(
        &self,
        id: PersonId,
        partner: PersonId,
    ) -> Option<game_lifecycle::ParentCandidateFacts> {
        let mind = self.host.state.decisions.as_ref().and_then(|_| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, id)
        })?;
        let body = &game_physiology::body(&self.host.storage, id).expect("body exists");
        let life = self
            .host
            .state
            .development
            .as_ref()
            .and_then(|_| game_lifecycle::life(&self.host.storage, id))?;
        let relationship = mind.relationships.get(&partner);
        Some(game_lifecycle::ParentCandidateFacts {
            family_intent: self.social_family_intent(id, partner),
            choice: ParentChoiceInput {
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
                knows_adequate_food: game_lifecycle::adequate_food(
                    game_characters::decisions::stocks(&self.host.storage, id)
                        .expect("stock knowledge exists")
                        .known
                        .values()
                        .map(|fact| (fact.good, fact.accessible, fact.quantity)),
                ),
            },
        })
    }
    fn willing_parent(&self, id: PersonId, partner: PersonId) -> bool {
        self.parent_choice_facts(id, partner)
            .is_some_and(game_lifecycle::chooses_parenthood)
    }

    fn lifecycle_factual_reads(&self, actors: &[PersonId], activities: bool) -> game_ecs::ReadSet {
        let mut reads = game_ecs::ReadSet::new();
        for actor in actors {
            let id = game_ecs::StableId(actor.0);
            reads.read::<game_physiology::Body>(&self.host.storage, id);
            reads.read::<Location>(&self.host.storage, id);
            if activities {
                reads.read::<Activity>(&self.host.storage, id);
            }
            reads.read::<game_characters::decisions::Mind<game_api::Command>>(
                &self.host.storage,
                id,
            );
            reads.read::<game_characters::decisions::Stocks>(&self.host.storage, id);
        }
        reads
    }

    fn lifecycle_body(&self, actor: PersonId) -> Result<BodyFacts, Error> {
        let person = self
            .person_index
            .get(&actor)
            .map(|index| self.person_at(*index))
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
        let prepared = world
            .prepare_start_conception(
                &self.host.storage,
                self.host.clock().time,
                id,
                [&first, &second],
                Consent {
                    initiator: true,
                    partner: self.willing_parent(partner, actor),
                },
            )?
            .with_factual_reads(self.lifecycle_factual_reads(&[actor, partner], false));
        let opening = self.host.transaction_opening();
        if let [LifecycleEffect::Refused { initiator, partner }] = prepared.effects() {
            let event = EventKind::ConceptionRefused {
                actor: *initiator,
                partner: *partner,
            };
            let mut batch = game_ecs::Batch::new();
            let life = prepared.stage_into(world, &self.host.storage, &mut batch)?;
            life.validate(&self.host.storage, &batch)?;
            let mut transaction = game_world::Transaction::new(opening, batch);
            transaction.revisions = 1;
            transaction.events.push(event);
            self.host.commit_transaction(transaction)?;
            return Ok(());
        }
        let [
            LifecycleEffect::AttemptStarted {
                pregnancy,
                parents,
                completes,
            },
        ] = prepared.effects()
        else {
            return Err(Error::Unavailable);
        };
        let pregnancy = *pregnancy;
        let parents = *parents;
        let completes = *completes;
        let actors = [actor, partner];
        let interruptions =
            self.prepare_interruptions(&actors, crate::commands::ActivityOwner::Physical)?;
        let mut batch = game_ecs::Batch::new();
        let life = prepared.stage_into(world, &self.host.storage, &mut batch)?;
        let activities = actors
            .into_iter()
            .map(|person| {
                (
                    person,
                    Activity::Conceive {
                        pregnancy,
                        completes,
                    },
                )
            })
            .collect();
        let mut interrupted = interruptions.stage_into(self, &mut batch, &activities)?;
        life.validate(&self.host.storage, &batch)?;
        interrupted.validate_batch(self, &batch)?;
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = 1;
        transaction.revisions = 1;
        transaction.events = interrupted.events_for(&actors);
        self.host.commit_transaction(transaction)?;
        for person in actors {
            interrupted.finish_actor_metadata(self, person);
            self.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(person));
        }
        self.pregnancy_index.insert(parents[0], pregnancy);
        Ok(())
    }

    pub(crate) fn review_conception(&mut self) -> Result<(), Error> {
        let Some(_) = &self.host.state.development else {
            return Ok(());
        };
        let relationships = game_lifecycle::all_lives(&self.host.storage).flat_map(|(actor, _)| {
            game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
                .into_iter()
                .flat_map(move |mind| {
                    mind.relationships
                        .keys()
                        .map(move |partner| (actor, *partner))
                })
        });
        let pairs = game_lifecycle::conception_candidates(
            relationships,
            self.host.state.ruler,
            |actor, partner| self.parent_choice_facts(actor, partner),
        );
        for (a, b) in pairs {
            if let Some((a, b)) =
                game_lifecycle::current_conception_candidate((a, b), |actor, partner| {
                    self.parent_choice_facts(actor, partner)
                })
            {
                match self.start_conception(a, b) {
                    Ok(_) | Err(Error::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }

    pub(crate) fn complete_conception(&mut self, id: PregnancyId) -> Result<(), Error> {
        let pregnancy =
            game_lifecycle::pregnancy(&self.host.storage, id).ok_or(Error::Unavailable)?;
        let pair = [pregnancy.gestator, pregnancy.partner];
        let bodies = [self.lifecycle_body(pair[0])?, self.lifecycle_body(pair[1])?];
        let now = self.time();
        let opening = self.host.transaction_opening();
        let world = self
            .host
            .state
            .development
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let prepared = world
            .prepare_complete_conception(&self.host.storage, id, now, [&bodies[0], &bodies[1]])?
            .with_factual_reads(self.lifecycle_factual_reads(&pair, true));
        let effects = prepared.effects().to_vec();
        let interruptions = self.prepare_interruptions_after_conception(
            &pair,
            crate::commands::ActivityOwner::Physical,
            &prepared,
        )?;
        let genetic = if effects
            .iter()
            .any(|effect| matches!(effect, LifecycleEffect::Conceived { .. }))
        {
            self.host
                .state
                .genetics
                .as_ref()
                .map(|owner| {
                    owner.prepare_conception(&self.host.storage, self.host.seed(), id, pair, now)
                })
                .transpose()?
        } else {
            None
        };
        let endings = if effects
            .iter()
            .any(|effect| matches!(effect, LifecycleEffect::PregnancyEnded(_)))
        {
            self.host
                .state
                .genetics
                .as_ref()
                .map(|owner| owner.prepare_end_pregnancy(&self.host.storage, id, now))
        } else {
            None
        };
        let mut batch = game_ecs::Batch::new();
        let mut interrupted = interruptions.stage_into(
            self,
            &mut batch,
            &BTreeMap::from([(pair[0], Activity::Idle), (pair[1], Activity::Idle)]),
        )?;
        let mut factual_writes = game_ecs::ReadFootprint::new();
        for actor in pair {
            factual_writes.protect::<Activity>(game_ecs::StableId(actor.0));
        }
        let life = prepared.stage_into_with_factual_writes(
            world,
            &self.host.storage,
            &mut batch,
            factual_writes,
        )?;
        let genetic = genetic
            .map(|plan| {
                plan.stage_into(
                    self.host
                        .state
                        .genetics
                        .as_ref()
                        .ok_or(Error::Unavailable)?,
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let ending = endings
            .map(|plan| plan.stage_guarded_into(&self.host.storage, &mut batch))
            .transpose()?;
        interrupted.validate_batch(self, &batch)?;
        life.validate(&self.host.storage, &batch)?;
        if let Some(genetic) = &genetic {
            genetic.validate(
                self.host
                    .state
                    .genetics
                    .as_ref()
                    .ok_or(Error::Unavailable)?,
                &self.host.storage,
                &batch,
            )?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.events = interrupted.events_for(&pair);
        if let Some(ending) = ending {
            transaction.guards.push(ending);
        }
        for effect in &effects {
            transaction.events.push(match effect {
                LifecycleEffect::Conceived {
                    pregnancy, parents, ..
                } => EventKind::Conceived {
                    pregnancy: *pregnancy,
                    parents: *parents,
                },
                LifecycleEffect::PregnancyEnded(effect) => EventKind::PregnancyEnded {
                    pregnancy: effect.pregnancy,
                },
                _ => return Err(Error::Unavailable),
            });
        }
        self.host.commit_transaction(transaction)?;
        for actor in pair {
            interrupted.finish_actor_metadata(self, actor);
        }
        if let Some(genetic) = genetic {
            genetic.finish(self.host.state.genetics.as_mut().unwrap());
        }
        for effect in effects {
            match effect {
                LifecycleEffect::Conceived { pregnancy, due, .. } => {
                    self.host
                        .state
                        .due
                        .entry(due)
                        .or_default()
                        .push(Due::Birth(pregnancy));
                }
                LifecycleEffect::PregnancyEnded(effect) => {
                    self.pregnancy_index.remove(&effect.parents[0]);
                }
                _ => unreachable!("validated conception effect"),
            }
        }
        Ok(())
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
                    .checked_add(self.people().len() as u64 / 2)
                    .ok_or(Error::Overflow)?,
            )
            .ok_or(Error::Overflow)?;
        Ok(())
    }

    pub(crate) fn complete_development(&mut self, due: Due) -> Result<(), Error> {
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
                        self.person_at(self.person_index[&id]).role != Role::Guardian,
                    )
                    .expect("preflighted age transition")
                    .with_factual_reads(self.lifecycle_factual_reads(&[id], true));
                let effects = prepared
                    .commit(
                        self.host.state.development.as_ref().unwrap(),
                        &mut self.host.storage,
                    )
                    .expect("immediately prepared lifecycle transition");
                for effect in effects {
                    match effect {
                        LifecycleEffect::Death { actor } => self.end_life(actor)?,
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
            Due::Birth(id) => return self.birth(id),
            _ => {}
        }
        Ok(())
    }

    fn birth(&mut self, id: PregnancyId) -> Result<(), Error> {
        let now = self.host.clock().time;
        let opening = self.host.transaction_opening();
        let pregnancy = game_lifecycle::pregnancy(&self.host.storage, id)
            .ok_or(Error::Unavailable)?
            .clone();
        let mut gestator = self.lifecycle_body(pregnancy.gestator)?;
        gestator.position = self.resolved_location(pregnancy.gestator);
        let child = PersonId(opening.next_id);
        let lifecycle = self
            .host
            .state
            .development
            .as_ref()
            .ok_or(Error::Unavailable)?;
        let mut birth = lifecycle.prepare_birth(
            &self.host.storage,
            id,
            now,
            child,
            &gestator,
            self.host.seed(),
        )?;
        if birth.effects().is_empty() {
            return Ok(());
        }
        if let [LifecycleEffect::PregnancyEnded(effect)] = birth.effects() {
            let event = EventKind::PregnancyEnded {
                pregnancy: effect.pregnancy,
            };
            let mut batch = game_ecs::Batch::new();
            let life = birth.stage_into(lifecycle, &self.host.storage, &mut batch)?;
            let genetic = self
                .host
                .state
                .genetics
                .as_ref()
                .map(|genes| genes.prepare_end_pregnancy(&self.host.storage, id, now));
            let genetic = genetic
                .map(|plan| plan.stage_guarded_into(&self.host.storage, &mut batch))
                .transpose()?;
            life.validate(&self.host.storage, &batch)?;
            if let Some(genetic) = &genetic {
                genetic.validate(&self.host.storage, &batch)?;
            }
            let mut transaction = game_world::Transaction::new(opening, batch);
            transaction.events.push(event);
            if let Some(genetic) = genetic {
                transaction.guards.push(genetic);
            }
            self.host.commit_transaction(transaction)?;
            self.pregnancy_index.remove(&pregnancy.gestator);
            return Ok(());
        }
        let [
            LifecycleEffect::Born {
                position,
                parents,
                next_age,
                ..
            },
        ] = birth.effects()
        else {
            return Err(Error::Unavailable);
        };
        let inventory = InventoryId(opening.next_id.checked_add(1).ok_or(Error::Overflow)?);
        let initial_location = position.clone();
        let parents = *parents;
        let next_age = *next_age;
        let genes = self.host.state.genetics.as_ref();
        let genetic_birth = genes
            .map(|genes| genes.prepare_birth(&self.host.storage, id, child, now))
            .transpose()?;
        let genome = genetic_birth.as_ref().and_then(|birth| birth.genome());
        let source = genome.map(|genome| genes.unwrap().genome_source(child, genome));
        if genes.is_some() && source.is_none() {
            return Err(Error::Unavailable);
        }
        if let Some(source) = &source {
            let role = self
                .host
                .state
                .heredity
                .as_ref()
                .ok_or(Error::Unavailable)?
                .interpret_genes(child, source)?;
            birth = birth.receive_reproduction(role)?;
        }
        let physical = &self
            .host
            .state
            .physical
            .as_ref()
            .ok_or(Error::Unavailable)?
            .rules;
        let body = if let Some(source) = &source {
            self.host
                .state
                .physical_traits
                .as_ref()
                .ok_or(Error::Unavailable)?
                .prepare_newborn_body(&self.host.storage, child, physical, source)?
        } else {
            game_physiology::prepare_newborn_body(&self.host.storage, child, physical)?
        };
        let newborn_body = body.body().clone();
        let limits = physical.expressed(body.phenotype());
        let temperament = self
            .host
            .state
            .temperament
            .as_ref()
            .map(|owner| {
                if let Some(source) = &source {
                    owner.prepare_newborn_expression(&self.host.storage, child, source)
                } else {
                    owner.prepare_newborn_baseline(&self.host.storage, child)
                }
            })
            .transpose()?;
        let same_place = parents
            .into_iter()
            .filter(|parent| self.resolved_location(*parent) == initial_location)
            .collect::<Vec<_>>();
        let mut mind = game_characters::orchestration::prepare_newborn_mind::<game_api::Command>(
            &self.host.storage,
            child,
            pregnancy.gestator,
            parents,
            now,
            &same_place,
        )?;
        if let Some(temperament) = &temperament {
            mind = mind.with_personality(temperament.personality().clone())?;
        }
        let newborn_personality = mind.personality().clone();
        struct BirthBodies<'a> {
            simulation: &'a Simulation,
            child: PersonId,
            body: Option<game_transport::Body>,
        }
        impl game_transport::BodyFacts for BirthBodies<'_> {
            fn body(&self, actor: PersonId) -> Option<game_transport::Body> {
                if actor == self.child {
                    self.body.clone()
                } else {
                    game_transport::BodyFacts::body(&self.simulation.transport_body_lookup(), actor)
                }
            }
        }
        let measured = self
            .host
            .state
            .transport
            .as_ref()
            .map(|owner| {
                owner.measured_unplaced_body(
                    &initial_location,
                    now,
                    game_transport::BodyMeasurements {
                        age: Some((SimDuration(0), lifecycle.rules.adulthood)),
                        cargo: Quantity(0),
                        capacity: limits.carrying_capacity,
                        alive: newborn_body.vitals.alive,
                        health: newborn_body.vitals.health,
                        fatigue: newborn_body.vitals.fatigue,
                        fatigue_limit: limits.fatigue_limit,
                        independent: false,
                        combat_restricted: false,
                        work: game_transport::CarryWork::Idle,
                        incapacitation: self
                            .host
                            .state
                            .combat
                            .as_ref()
                            .map(|combat| combat.rules.incapacitation),
                    },
                )
            })
            .transpose()?;
        let bodies = BirthBodies {
            simulation: self,
            child,
            body: measured,
        };
        let motion = self
            .host
            .state
            .transport
            .as_ref()
            .filter(|_| matches!(initial_location, Location::InTransit { .. }))
            .map(|owner| {
                owner.prepare_reserved_birth_motion(
                    &self.host.storage,
                    child,
                    pregnancy.gestator,
                    now,
                    &bodies,
                )
            })
            .transpose()?;
        let motion_effects = motion
            .as_ref()
            .map(|motion| motion.effects().to_vec())
            .unwrap_or_default();
        let location = motion
            .as_ref()
            .and_then(|motion| motion.projected_location(&self.host.storage, child))
            .cloned()
            .unwrap_or(initial_location);
        let constraints = game_lineage::guardians(&self.host.storage)
            .map(|(actor, _)| (actor, self.guardian_constraint(actor)))
            .collect();
        let succession = self
            .host
            .state
            .lineage
            .as_ref()
            .map(|owner| {
                owner.prepare_reserved_birth(
                    &self.host.storage,
                    &[game_lineage::ActualBirth {
                        person: child,
                        born: now,
                        initial_age: SimDuration(0),
                        parent_count: parents.len(),
                        alive: newborn_body.vitals.alive,
                        location: location.clone(),
                    }],
                    now,
                    &constraints,
                )
            })
            .transpose()?;
        let role_changes = succession
            .as_ref()
            .map(|plan| plan.output().role_changes())
            .unwrap_or_default();
        let role = role_changes
            .iter()
            .find(|(actor, _)| *actor == child)
            .map_or(Role::Citizen, |(_, role)| *role);
        let person =
            game_characters::people::prepare_newborn_person(&self.host.storage, child, inventory)?
                .with_role(role);
        let roles = game_characters::people::prepare_roles(
            &self.host.storage,
            role_changes
                .into_iter()
                .filter(|(actor, _)| *actor != child)
                .map(|(actor, role)| game_characters::people::RoleChange { actor, role }),
        )?;
        let mut facts = game_ecs::ReadSet::new();
        facts.read::<game_physiology::Body>(
            &self.host.storage,
            game_ecs::StableId(pregnancy.gestator.0),
        );
        facts.membership::<game_characters::people::Person>(&self.host.storage, |_, _| true);
        facts.membership::<game_transport::CarryGroup>(&self.host.storage, |_, _| true);
        for (carrier, _) in game_transport::groups(&self.host.storage) {
            facts.read::<game_transport::CarryGroup>(
                &self.host.storage,
                game_ecs::StableId(carrier.0),
            );
        }
        let witnesses = self
            .people()
            .map(|person| {
                facts.read::<game_physiology::Body>(
                    &self.host.storage,
                    game_ecs::StableId(person.id.0),
                );
                facts.read::<Location>(&self.host.storage, game_ecs::StableId(person.id.0));
                facts.read::<game_lifecycle::Life>(
                    &self.host.storage,
                    game_ecs::StableId(person.id.0),
                );
                facts.read::<game_health::Occupancy>(
                    &self.host.storage,
                    game_ecs::StableId(person.id.0),
                );
                game_society::personal::BirthWitness {
                    actor: person.id,
                    same_location: motion
                        .as_ref()
                        .and_then(|motion| motion.projected_location(&self.host.storage, person.id))
                        .or_else(|| {
                            game_spatial::location(
                                &self.host.storage,
                                game_ecs::StableId(person.id.0),
                            )
                        })
                        == Some(&location),
                    alive: game_physiology::body(&self.host.storage, person.id)
                        .is_some_and(|body| body.vitals.alive),
                }
            })
            .chain(std::iter::once(game_society::personal::BirthWitness {
                actor: child,
                same_location: true,
                alive: newborn_body.vitals.alive,
            }))
            .collect();
        let social = self
            .host
            .state
            .social
            .as_ref()
            .map(|owner| {
                game_society::personal::prepare_receipt(
                    owner,
                    &self.host.storage,
                    game_society::personal::PersonalReceipt::Born {
                        child,
                        temperament: game_characters::social::Temperament::from_personality(
                            &newborn_personality,
                        ),
                        witnesses,
                    },
                    now,
                )
            })
            .transpose()?;
        let health = self
            .host
            .state
            .health
            .as_ref()
            .map(|owner| {
                let gestating = parents
                    .iter()
                    .map(|parent| {
                        (
                            *parent,
                            game_lifecycle::life(&self.host.storage, *parent)
                                .is_some_and(|life| life.reproduction == Reproduction::Gestates),
                        )
                    })
                    .collect::<Vec<_>>();
                let inherited_shelter = owner.birth_shelter(&gestating, |parent| {
                    self.effective_shelter(parent).map(|shelter| shelter.id)
                });
                owner.prepare_reaction(
                    &self.host.storage,
                    now,
                    game_health::HealthReaction::Born {
                        child,
                        inherited_shelter,
                    },
                )
            })
            .transpose()?;
        let skills = self
            .host
            .state
            .skills
            .as_ref()
            .map(|owner| owner.prepare_newborn(&self.host.storage, child))
            .transpose()?;
        let realm = self
            .host
            .state
            .realm
            .as_ref()
            .map(|owner| owner.prepare_newborn(&self.host.storage, child))
            .transpose()?;
        let cargo = game_cargo::prepare_newborn_inventory(&self.host.storage, inventory, child)?;
        let property = self
            .host
            .state
            .property
            .as_ref()
            .map(|_| game_property::prepare_newborn(&self.host.storage, child, inventory))
            .transpose()?;
        let profile = self
            .host
            .state
            .combat
            .as_ref()
            .map(|_| game_conflict::prepare_newborn_profile(&self.host.storage, child))
            .transpose()?;
        let mut batch = game_ecs::Batch::new();
        let mut guards = Vec::new();
        let life = birth.stage_into(lifecycle, &self.host.storage, &mut batch)?;
        guards.push(person.stage_into(&self.host.storage, &mut batch)?);
        guards.push(roles.stage_into(&self.host.storage, &mut batch)?);
        guards.push(body.stage_into(
            physical,
            self.host.state.physical_traits.as_ref(),
            &self.host.storage,
            &mut batch,
        )?);
        guards.push(mind.stage_into(&self.host.storage, &mut batch)?);
        guards.push(cargo.stage_into(&self.host.storage, &mut batch)?);
        if let Some(property) = property {
            guards.push(property.stage_into(&self.host.storage, &mut batch)?);
        }
        if let Some(realm) = realm {
            guards.push(realm.stage_into(&self.host.storage, &mut batch)?);
        }
        if let Some(profile) = profile {
            guards.push(profile.stage_into(&self.host.storage, &mut batch)?);
        }
        let motion = motion
            .map(|motion| {
                motion.stage_into(
                    self.host.state.transport.as_ref().unwrap(),
                    &self.host.storage,
                    &bodies,
                    now,
                    &mut batch,
                )
            })
            .transpose()?;
        if motion.is_none() {
            guards.push(
                game_spatial::prepare_location(
                    &self.host.storage,
                    game_ecs::StableId(child.0),
                    location.clone(),
                )?
                .stage_into(&self.host.storage, &mut batch)?,
            );
        }
        let temperament = temperament
            .map(|plan| {
                plan.stage_into(
                    self.host.state.temperament.as_ref().unwrap(),
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        if let Some(temperament) = &temperament {
            temperament.validate(
                self.host.state.temperament.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )?;
            if let Some(source) = &source {
                temperament.validate_genes(source)?;
            }
            guards.push(temperament.native_guard().clone());
        }
        let genetic = genetic_birth
            .map(|plan| plan.stage_into(genes.unwrap(), &self.host.storage, &mut batch))
            .transpose()?;
        let social = social
            .map(|plan| {
                plan.stage_into(
                    self.host.state.social.as_ref().unwrap(),
                    &self.host.storage,
                    now,
                    &mut batch,
                )
            })
            .transpose()?;
        let health = health
            .map(|plan| {
                plan.stage_into(
                    self.host.state.health.as_ref().unwrap(),
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let skills = skills
            .map(|plan| {
                plan.stage_into(
                    self.host.state.skills.as_ref().unwrap(),
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let lineage = succession
            .map(|plan| {
                plan.stage_into(
                    self.host.state.lineage.as_ref().unwrap(),
                    &self.host.storage,
                    &mut batch,
                )
            })
            .transpose()?;
        let mut activity_reads = game_ecs::ReadSet::new();
        activity_reads.read::<Activity>(&self.host.storage, game_ecs::StableId(child.0));
        batch.insert(game_ecs::StableId(child.0), Activity::Idle);
        let mut activity_writes = game_ecs::ReadFootprint::new();
        activity_writes.protect::<Activity>(game_ecs::StableId(child.0));
        guards.push(activity_reads.bind(&self.host.storage, &batch, &activity_writes)?);
        let mut location_writes = game_ecs::ReadFootprint::new();
        for actor in batch.edited_ids::<Location>() {
            location_writes.protect::<Location>(actor);
        }
        for actor in batch.edited_ids::<game_transport::CarryGroup>() {
            location_writes.protect::<game_transport::CarryGroup>(actor);
        }
        for actor in batch.edited_ids::<game_characters::people::Person>() {
            location_writes.protect::<game_characters::people::Person>(actor);
        }
        for actor in batch.edited_ids::<game_health::Occupancy>() {
            location_writes.protect::<game_health::Occupancy>(actor);
        }
        guards.push(facts.bind(&self.host.storage, &batch, &location_writes)?);
        life.validate(&self.host.storage, &batch)?;
        if let Some(genetic) = &genetic {
            genetic.validate(genes.unwrap(), &self.host.storage, &batch)?;
        }
        if let Some(social) = &social {
            social.validate(
                self.host.state.social.as_ref().unwrap(),
                &self.host.storage,
                now,
                &batch,
            )?;
            guards.push(social.row_guard());
        }
        if let Some(health) = &health {
            health.validate_batch(
                self.host.state.health.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )?;
        }
        if let Some(skills) = &skills {
            skills.validate(
                self.host.state.skills.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )?;
        }
        if let Some(lineage) = &lineage {
            lineage.validate(
                self.host.state.lineage.as_ref().unwrap(),
                &self.host.storage,
                &batch,
            )?;
            guards.push(lineage.guard().clone());
        }
        if let Some(motion) = &motion {
            motion.validate_footprint(&self.host.storage, &batch)?;
        }
        let mut events = Vec::new();
        for effect in &motion_effects {
            if let game_transport::MotionEffect::Interrupted { actor } = effect {
                events.push(EventKind::Interrupted { actor: *actor });
            }
        }
        events.push(EventKind::Born {
            pregnancy: id,
            child,
            parents,
        });
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.ids = 2;
        transaction.guards = guards;
        transaction.events = events;
        self.host.commit_transaction(transaction)?;
        if let Some(motion) = motion {
            let _ = motion.finish(self.host.state.transport.as_mut().unwrap());
        }
        let index = self.person_ids.len();
        self.person_ids.push(child);
        self.person_index.insert(child, index);
        self.inventory_index
            .entry(InventoryLocation::Carried(child))
            .or_default()
            .push(inventory);
        self.pregnancy_index.remove(&pregnancy.gestator);
        for effect in motion_effects {
            if let game_transport::MotionEffect::ClearArrival(actor) = effect {
                self.clear_transport_arrival(actor);
            }
        }
        if let Location::InTransit { arrives, .. } = location {
            self.host
                .state
                .due
                .entry(arrives)
                .or_default()
                .push(Due::Arrive(child));
        }
        if let Location::AtArea(area) = self.resolved_location(child) {
            self.index_dependent(child, area);
        }
        self.host
            .state
            .due
            .entry(next_age)
            .or_default()
            .push(Due::Age(child));
        if let Some(lineage) = lineage
            && let game_lineage::Succession::Bound { successor, .. } = lineage.finish()
        {
            self.host.state.ruler = successor;
        }
        Ok(())
    }

    pub(crate) fn index_dependent(&mut self, id: PersonId, area: AreaId) {
        if self.host.state.development.is_some()
            && game_lifecycle::dependent(
                game_lifecycle::life(&self.host.storage, id).map(|life| life.stage),
            )
        {
            self.dependent_index.entry(area).or_default().insert(id);
        }
    }

    pub(crate) fn schedule_age(&mut self, id: PersonId) {
        if self.person_at(self.person_index[&id]).role == Role::Guardian
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

    pub(crate) fn end_life(&mut self, id: PersonId) -> Result<(), Error> {
        let death = game_physiology::prepare_death(&self.host.storage, id)?;
        if !death.lethal() {
            return Ok(());
        }
        let opening = self.host.transaction_opening();
        let cleanup = self.prepare_death_cleanup(id)?;
        let mut batch = game_ecs::Batch::new();
        let body = death.stage_into(&self.host.storage, &mut batch)?;
        let cleanup = cleanup.stage_into(self, &mut batch)?;
        let invocation = self.prepare_death_invocation(id, None, &mut batch)?;
        body.validate(&self.host.storage, &batch)?;
        cleanup.validate(self, &batch)?;
        if let Some(invocation) = &invocation {
            invocation.validate(self, &batch)?;
        }
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.guards.extend(cleanup.guards());
        if let Some(invocation) = &invocation {
            transaction.guards.extend(invocation.guards());
        }
        transaction.events = cleanup.events();
        self.host.commit_transaction(transaction)?;
        cleanup.finish(self);
        if let Some(invocation) = invocation {
            invocation.finish(self);
        }
        Ok(())
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
    for person_id in state.people().map(|person| person.id).collect::<Vec<_>>() {
        let person_role = state.person(person_id).expect("known person").role;
        let body = game_physiology::body(state.components, person_id).ok_or_else(fail)?;
        let attempt = match *state
            .components
            .activity(person_id)
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
            person_id,
            game_lifecycle::ValidationBody {
                body: BodyFacts {
                    actor: person_id,
                    alive: body.vitals.alive,
                    health: body.vitals.health,
                    position: (*game_spatial::location(
                        state.components,
                        game_ecs::StableId((person_id).0),
                    )
                    .expect("location exists"))
                    .clone(),
                    action_restricted: false,
                    attempt,
                },
                aging_enabled: person_role != Role::Guardian,
                lifespan_loss: state
                    .state
                    .invocation
                    .as_ref()
                    .and_then(|_| game_invocation::burden(state.components, person_id))
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
