//! Local chosen experiences change personal aims. Teaching transmits only the
//! teacher's remembered information and mastery; family intent is a conversation,
//! not marriage, custody, title, or permanent consent to conception.
use crate::*;
#[cfg(test)]
use game_characters::social::pair;

use game_characters::social::{self as personal, ConversationInput, Temperament};
pub use game_characters::social::{Development, Interaction, Offer, Rules, World};

fn baseline(personality: &decisions::Personality, current: decisions::Aspiration) -> Development {
    Development::baseline(Temperament::from_personality(personality), current)
        .expect("validated personal temperament")
}
impl Simulation {
    pub fn initialize_social(&mut self, rules: Rules) -> Result<(), Error> {
        if !rules.valid()
            || self.host.state.decisions.is_none()
            || self.host.state.development.is_none()
            || self.host.state.physical.is_none()
        {
            return Err(Error::InvalidConfiguration(
                "invalid social development rules or missing human domains".into(),
            ));
        }
        let records = game_characters::decisions::minds::<game_api::Command>(&self.host.storage)
            .map(|(id, mind)| {
                let mut record = baseline(&mind.personality, mind.dream);
                record.next_choice = self.host.clock().time;
                (
                    id,
                    personal::Personal {
                        development: record,
                        ..Default::default()
                    },
                )
            })
            .collect::<Vec<_>>();
        for (id, row) in records {
            personal::insert_personal(&mut self.host.storage, id, row)?;
        }
        self.host.state.social = Some(World {
            rules,
            last_review: self.host.clock().time,
        });
        Ok(())
    }
    pub(crate) fn social_family_intent(&self, first: PersonId, second: PersonId) -> bool {
        self.host.state.social.as_ref().is_none_or(|_| {
            personal::personal(&self.host.storage, first)
                .is_some_and(|row| row.family_intent.contains(&second))
        })
    }
    fn social_adult(&self, actor: PersonId) -> bool {
        personal::adult_stage(
            self.host
                .state
                .development
                .as_ref()
                .and_then(|_| game_lifecycle::life(&self.host.storage, actor))
                .map(|life| life.stage),
        )
    }
    fn social_speech(&self, actor: PersonId) -> bool {
        let body = self
            .host
            .state
            .physical
            .as_ref()
            .and_then(|_| game_physiology::body(&self.host.storage, actor));
        personal::can_speak(personal::SpeechFacts {
            stationary: matches!(
                game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)),
                Some(Location::AtArea(_))
            ),
            independent: self.independent(actor),
            in_combat: self.in_combat(actor),
            alive: body.is_some_and(|body| body.vitals.alive),
            health: body.map_or(0, |body| body.vitals.health),
            idle_or_rest: matches!(
                self.host.storage.activity(actor),
                Some(Activity::Idle | Activity::Rest)
            ),
            receiving_care: self.care_index.contains_key(&actor),
            conversation_available: self.conversation_actor_available(actor),
        })
    }
    fn relationship(&self, first: PersonId, second: PersonId) -> decisions::Relationship {
        game_characters::decisions::mind::<game_api::Command>(&self.host.storage, first)
            .unwrap()
            .relationships
            .get(&second)
            .cloned()
            .unwrap_or(decisions::Relationship {
                affection: 0,
                trust: 0,
            })
    }
    fn social_person_facts(
        &self,
        actor: PersonId,
        sender: PersonId,
    ) -> game_society::personal::PersonFacts {
        let mind = game_characters::decisions::mind::<game_api::Command>(&self.host.storage, actor)
            .expect("mind exists");
        let relation = self.relationship(actor, sender);
        let body = game_physiology::body(&self.host.storage, actor).expect("body exists");
        let rules = self.body_rules(actor);
        let quantity = self
            .host
            .state
            .social
            .as_ref()
            .map_or(Quantity(0), |world| world.rules.practice_material);
        game_society::personal::PersonFacts {
            actor,
            area: match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)) {
                Some(Location::AtArea(area)) => Some(*area),
                _ => None,
            },
            speech: self.social_speech(actor),
            adult: self.social_adult(actor),
            practical: self.social_practical_actor(actor),
            tool: self.social_tool(actor),
            materials: [Good::Timber, Good::Seed, Good::Herb]
                .into_iter()
                .filter_map(|good| {
                    self.social_material(actor, good, quantity)
                        .map(|id| (good, id))
                })
                .collect(),
            farming_access: self.host.state.agriculture.as_ref().is_some_and(|runtime| {
                runtime.fields(&self.host.storage).any(|(_, field)| {
                    Some(&Location::AtArea(field.area))
                        == game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0))
                        && field.access.contains(&actor)
                })
            }),
            routes: mind.routes.clone(),
            practice: game_skills::skills(&self.host.storage, actor)
                .map(|row| {
                    row.records
                        .iter()
                        .map(|(skill, progress)| (*skill, progress.practice_ms))
                        .collect()
                })
                .unwrap_or_default(),
            empathy: mind.personality.empathy,
            patience: mind.personality.patience,
            affection: relation.affection,
            trust: relation.trust,
            conversation: Some(ConversationInput {
                temperament: Temperament::from_personality(&mind.personality),
                affection: relation.affection,
                trust: relation.trust,
                hunger: body.vitals.hunger,
                meal_relief: rules.meal_relief,
                fatigue: body.vitals.fatigue,
                fatigue_limit: rules.fatigue_limit,
                recipient_adult: self.social_adult(actor),
                sender_adult: self.social_adult(sender),
                recipient_aspiration: mind.dream,
                chosen_companionship: false,
                visible_disruption: self.invocation_social_penalty(sender),
            }),
        }
    }
    fn social_refusal(
        &mut self,
        actor: PersonId,
        recipient: PersonId,
        reason: &str,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        self.ensure_event_capacity(1)?;
        self.host.state.social.as_ref().unwrap().refuse(
            &mut self.host.storage,
            actor,
            recipient,
            owner_now,
            reason,
        )?;
        self.emit(EventKind::SocialRefused {
            actor,
            recipient,
            reason: reason.into(),
        });
        self.host.bump_revision(1)?;
        Ok(())
    }
    fn social_stock_facts(&self, actor: PersonId) -> Vec<game_society::personal::StockFacts> {
        game_characters::decisions::stocks(&self.host.storage, actor)
            .into_iter()
            .flat_map(|row| row.known.values())
            .filter_map(|known| {
                game_cargo::inventory(&self.host.storage, known.inventory).map(|stock| {
                    game_society::personal::StockFacts {
                        inventory: stock.id,
                        known_good: known.good,
                        known_quantity: known.quantity,
                        known_accessible: known.accessible,
                        actual_good: stock.good,
                        actual_quantity: stock.quantity,
                        area: self.inventory_area(stock),
                        accessible: self.has_stock_access(actor, stock),
                        tool_available: self.tool_available_for(actor, stock.id),
                    }
                })
            })
            .collect()
    }
    fn social_tool(&self, actor: PersonId) -> Option<InventoryId> {
        game_society::personal::select_tool(self.social_stock_facts(actor))
    }
    fn social_practical_facts(&self, actor: PersonId) -> personal::PracticalFacts {
        personal::PracticalFacts {
            combat_restricted: self.combat_restricts(actor),
            fatigue: game_physiology::body(&self.host.storage, actor)
                .map_or(u64::MAX, |body| body.vitals.fatigue),
            fatigue_limit: self.body_rules(actor).fatigue_limit,
        }
    }
    pub(crate) fn social_practical_actor(&self, actor: PersonId) -> bool {
        personal::can_practice(self.social_practical_facts(actor))
    }
    fn social_material(
        &self,
        actor: PersonId,
        good: Good,
        quantity: Quantity,
    ) -> Option<InventoryId> {
        let area = match game_spatial::location(&self.host.storage, game_ecs::StableId(actor.0)) {
            Some(Location::AtArea(area)) => Some(*area),
            _ => None,
        };
        game_society::personal::select_material(
            self.social_stock_facts(actor),
            area,
            good,
            quantity,
        )
    }
    pub(crate) fn start_social(
        &mut self,
        actor: PersonId,
        action: SocialAction,
    ) -> Result<(), Error> {
        let owner_now = self.host.clock().time;
        self.host.preflight_records(0, 0, 1)?;
        let world = self.host.state.social.as_ref().ok_or(Error::Unavailable)?;
        if !self.person_index.contains_key(&actor) {
            return Err(Error::Unavailable);
        }
        let facts = self.social_person_facts(actor, actor);
        let partner = match action {
            SocialAction::Talk { partner, .. } => Some(partner),
            SocialAction::Learn { teacher, .. } => Some(teacher),
            SocialAction::Practice { .. } => None,
        };
        if partner.is_some_and(|id| !self.person_index.contains_key(&id)) {
            return Err(Error::Unavailable);
        }
        let partner = partner.map(|id| self.social_person_facts(id, actor));
        let decision = game_society::personal::prepare_start(
            world,
            &self.host.storage,
            &facts,
            partner.as_ref(),
            action,
            owner_now,
        )?;
        let (interaction, materials) = match decision {
            game_society::personal::StartDecision::Refused { recipient, reason } => {
                return self.social_refusal(actor, recipient, reason);
            }
            game_society::personal::StartDecision::Started {
                interaction,
                materials,
                ..
            } => (interaction, materials),
        };
        let opening = self.host.transaction_opening();
        let actors = interaction.actors();
        let completes = interaction.completes();
        let final_activities = interaction
            .activity_bindings()
            .into_iter()
            .map(|(actor, activity)| (actor, bind_personal_activity(activity)))
            .collect::<BTreeMap<_, _>>();
        let start = world.prepare_start(&self.host.storage, actor, interaction, owner_now)?;
        let interruptions =
            self.prepare_interruptions(&actors, commands::ActivityOwner::Physical)?;
        let consumption = materials
            .map(|(_, source, quantity)| {
                game_cargo::prepare_consumptions(&self.host.storage, &[(source, quantity)])
            })
            .transpose()?;
        let mut batch = game_ecs::Batch::new();
        let start = start.stage_into(world, &self.host.storage, owner_now, &mut batch)?;
        let consumption = consumption
            .map(|plan| plan.stage_guarded_into(&self.host.storage, &mut batch))
            .transpose()?;
        let mut finish = interruptions.stage_into(self, &mut batch, &final_activities)?;
        start.validate(world, &self.host.storage, owner_now, &batch)?;
        if let Some(consumption) = &consumption {
            consumption.validate(&self.host.storage, &batch)?;
        }
        finish.validate_batch(self, &batch)?;
        let mut events = materials
            .map(|(actor, source, quantity)| EventKind::Consumed {
                actor,
                source,
                quantity,
            })
            .into_iter()
            .collect::<Vec<_>>();
        events.extend(finish.events_for(&actors));
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.events = events;
        transaction.revisions = 1;
        transaction.guards.push(start.row_guard());
        if let Some(consumption) = consumption {
            transaction.guards.push(consumption);
        }
        self.host.commit_transaction(transaction)?;
        for actor in &actors {
            finish.finish_actor_metadata(self, *actor);
        }
        for actor in actors {
            self.host
                .state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(actor));
        }
        Ok(())
    }
    pub(crate) fn social_activity_is_practical(&self, actor: PersonId) -> bool {
        personal::participant(&self.host.storage, actor)
            .and_then(|key| personal::interaction(&self.host.storage, key))
            .is_some_and(personal::interaction_is_practical)
    }
    pub(crate) fn social_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        personal::selected_tool(
            personal::interaction(
                &self.host.storage,
                personal::participant(&self.host.storage, actor)?,
            )?,
            actor,
        )
    }
    fn social_readiness_facts(
        &self,
        interaction: &Interaction,
    ) -> (Vec<personal::InteractionActorFacts>, bool) {
        let facts = interaction
            .actors()
            .into_iter()
            .map(|actor| {
                let body = game_physiology::body(&self.host.storage, actor);
                personal::InteractionActorFacts {
                    actor,
                    area: match game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId(actor.0),
                    ) {
                        Some(Location::AtArea(area)) => Some(*area),
                        _ => None,
                    },
                    alive: body.is_some_and(|body| body.vitals.alive),
                    health: body.map_or(0, |body| body.vitals.health),
                    in_combat: self.in_combat(actor),
                    practical: self.social_practical_facts(actor),
                }
            })
            .collect::<Vec<_>>();
        let tool_available = personal::interaction_tool(interaction)
            .is_none_or(|(user, tool)| self.tool_available_for(user, tool));
        (facts, tool_available)
    }
    pub(crate) fn social_interaction_valid(&self, interaction: &Interaction) -> bool {
        let (facts, tool_available) = self.social_readiness_facts(interaction);
        personal::interaction_ready(interaction, &facts, tool_available)
    }
    pub(crate) fn try_interrupt_social(&mut self, actor: PersonId) -> Result<(), Error> {
        let Some(world) = self.host.state.social.as_ref() else {
            return Ok(());
        };
        let Some(interaction) = personal::participant(&self.host.storage, actor)
            .and_then(|leader| personal::interaction(&self.host.storage, leader))
        else {
            return Ok(());
        };
        let kinds = interaction
            .actors()
            .into_iter()
            .map(|actor| {
                self.host
                    .storage
                    .activity(actor)
                    .map(|activity| (actor, bind_social_activity_kind(activity)))
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let Some(plan) =
            world.prepare_interruption::<Activity>(&self.host.storage, actor, &kinds)?
        else {
            return Ok(());
        };
        let mut batch = game_ecs::Batch::new();
        let publication = plan.stage_into(&self.host.storage, &mut batch, Activity::Idle)?;
        publication.validate(&self.host.storage, &batch)?;
        let mut transaction = game_world::Transaction::new(self.host.transaction_opening(), batch);
        transaction.guards = publication.row_guards();
        self.host.commit_transaction(transaction)?;
        for actor in publication.actors {
            for due in self.host.state.due.values_mut() {
                due.retain(|due| !matches!(due,Due::Physical(current) if *current==actor));
            }
        }
        self.host.state.due.retain(|_, events| !events.is_empty());
        Ok(())
    }
    pub(crate) fn reconcile_social(&mut self) -> Result<(), Error> {
        let facts = personal::interactions(&self.host.storage)
            .map(|(leader, interaction)| {
                let (actors, tool_available) = self.social_readiness_facts(interaction);
                personal::ReconciliationFacts {
                    leader,
                    actors,
                    tool_available,
                }
            })
            .collect::<Vec<_>>();
        let interrupted = personal::interrupted_interactions(&self.host.storage, &facts);
        for actor in interrupted {
            self.try_interrupt_social(actor)?;
        }
        Ok(())
    }
    pub(crate) fn complete_social(&mut self, actor: PersonId) -> Result<(), Error> {
        use game_society::personal::Completion;
        let Some(world) = self.host.state.social.as_ref() else {
            return Ok(());
        };
        let now = self.time();
        let opening = self.host.transaction_opening();
        let can_progress = personal::participant(&self.host.storage, actor)
            .and_then(|leader| personal::interaction(&self.host.storage, leader))
            .is_some_and(|interaction| self.social_interaction_valid(interaction));
        let (effect, prepared, minds) = game_society::personal::prepare_completion::<
            game_api::Command,
        >(
            world, &self.host.storage, actor, now, can_progress
        )?;
        let Some(prepared) = prepared else {
            return match effect {
                Completion::Interrupted => self.try_interrupt_social(actor),
                _ => Ok(()),
            };
        };
        let interaction = prepared.interaction();
        let kinds = interaction
            .actors()
            .into_iter()
            .map(|actor| {
                self.host
                    .storage
                    .activity(actor)
                    .map(|activity| (actor, bind_social_activity_kind(activity)))
                    .ok_or(Error::Unavailable)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let cleanup = world
            .prepare_interruption::<Activity>(&self.host.storage, actor, &kinds)?
            .ok_or(Error::Unavailable)?;
        let event = match effect {
            Completion::Socialized {
                first,
                second,
                topic,
            } => EventKind::Socialized {
                first,
                second,
                topic,
            },
            Completion::Instructed {
                teacher,
                student,
                topic,
            } => EventKind::InstructionCompleted {
                teacher,
                student,
                topic,
            },
            Completion::Practiced { actor, skill } => EventKind::PracticeCompleted { actor, skill },
            Completion::Waiting | Completion::Interrupted => return Err(Error::Unavailable),
        };
        let mut batch = game_ecs::Batch::new();
        let cleanup = cleanup.stage_into(&self.host.storage, &mut batch, Activity::Idle)?;
        let personal =
            prepared.stage_with_cleanup(world, &self.host.storage, now, &mut batch, &cleanup)?;
        let mut guards = cleanup.row_guards();
        guards.push(personal.row_guard());
        if let (Some(temperament), Some(genes)) =
            (&self.host.state.temperament, &self.host.state.genetics)
        {
            let experience = match &event {
                EventKind::Socialized { first, second, .. } => {
                    game_characters::temperament::ExperienceEvent::Company {
                        first: *first,
                        second: *second,
                    }
                }
                EventKind::InstructionCompleted {
                    teacher, student, ..
                } => game_characters::temperament::ExperienceEvent::Guidance {
                    teacher: *teacher,
                    student: *student,
                },
                EventKind::PracticeCompleted { actor, .. } => {
                    game_characters::temperament::ExperienceEvent::Practice { actor: *actor }
                }
                _ => return Err(Error::Unavailable),
            };
            let source = genes.allele_source(&self.host.storage);
            let traits = temperament
                .prepare_event::<game_api::Command>(&self.host.storage, experience, &source)?
                .fold_minds(&self.host.storage, minds)?
                .stage_into(temperament, &self.host.storage, &mut batch, &source)?;
            traits.validate(temperament, &self.host.storage, &batch, &source)?;
            guards.extend(traits.row_guards().iter().cloned());
        } else {
            guards.push(minds.stage_into(&self.host.storage, &mut batch)?);
        }
        personal.validate(world, &self.host.storage, now, &batch)?;
        cleanup.validate(&self.host.storage, &batch)?;
        let mut transaction = game_world::Transaction::new(opening, batch);
        transaction.events = vec![event];
        transaction.guards = guards;
        self.host.commit_transaction(transaction)?;
        for actor in cleanup.actors {
            for due in self.host.state.due.values_mut() {
                due.retain(|due| !matches!(due,Due::Physical(current) if *current==actor));
            }
        }
        self.host.state.due.retain(|_, events| !events.is_empty());
        Ok(())
    }
    pub(crate) fn social_boundary(&self, next: SimTime) -> SimTime {
        self.host.state.social.as_ref().map_or(next, |world| {
            game_society::personal::next_boundary(world, self.host.clock().time, next)
        })
    }
    fn bind_personal_receipt(
        &self,
        kind: &EventKind,
    ) -> Result<Option<game_society::personal::PersonalReceipt>, Error> {
        use game_society::personal::{BirthWitness, PersonalReceipt};
        let receipt = match *kind {
            EventKind::Born { child, .. } => {
                let location =
                    game_spatial::location(&self.host.storage, game_ecs::StableId(child.0));
                let witnesses = self
                    .people()
                    .map(|person| BirthWitness {
                        actor: person.id,
                        same_location: game_spatial::location(
                            &self.host.storage,
                            game_ecs::StableId(person.id.0),
                        ) == location,
                        alive: game_physiology::body(&self.host.storage, person.id)
                            .is_some_and(|body| body.vitals.alive),
                    })
                    .collect();
                PersonalReceipt::Born {
                    child,
                    temperament: Temperament::from_personality(
                        &game_characters::decisions::mind::<game_api::Command>(
                            &self.host.storage,
                            child,
                        )
                        .ok_or(Error::Unavailable)?
                        .personality,
                    ),
                    witnesses,
                }
            }
            EventKind::Cared {
                caregiver, child, ..
            } => PersonalReceipt::Cared { caregiver, child },
            EventKind::Comforted { caregiver, child } => {
                PersonalReceipt::Comforted { caregiver, child }
            }
            EventKind::Practiced {
                actor, interest, ..
            } => PersonalReceipt::Played { actor, interest },
            EventKind::Produced { actor, .. } | EventKind::PracticeCompleted { actor, .. } => {
                PersonalReceipt::Produced { actor }
            }
            EventKind::InstructionCompleted {
                teacher, student, ..
            } => PersonalReceipt::Instructed { teacher, student },
            EventKind::Socialized {
                first,
                second,
                topic,
            } => PersonalReceipt::Socialized {
                first,
                second,
                topic,
            },
            _ => return Ok(None),
        };
        Ok(Some(receipt))
    }
    pub(crate) fn prepare_social_receipts(
        &self,
        events: &[EventKind],
    ) -> Result<Option<game_society::personal::PreparedPersonalReceipts>, Error> {
        let Some(world) = self.host.state.social.as_ref() else {
            return Ok(None);
        };
        let receipts = events
            .iter()
            .filter_map(|event| self.bind_personal_receipt(event).transpose())
            .collect::<Result<Vec<_>, _>>()?;
        if receipts.is_empty() {
            return Ok(None);
        }
        game_society::personal::prepare_receipts(world, &self.host.storage, receipts, self.time())
            .map(Some)
    }
    fn receive_social_event(&mut self, kind: &EventKind) -> Result<(), Error> {
        let now = self.time();
        if let Some(plan) = self.prepare_social_receipts(std::slice::from_ref(kind))? {
            plan.commit(
                self.host.state.social.as_ref().unwrap(),
                &mut self.host.storage,
                now,
            )?;
        }
        Ok(())
    }
    pub(crate) fn on_social_event(&mut self, kind: &EventKind) {
        self.receive_social_event(kind)
            .expect("validated personal receipt");
    }

    fn social_speaker_facts(&self) -> Vec<game_society::personal::SpeakerFacts> {
        self.people()
            .map(|person| {
                let mind = game_characters::decisions::mind::<game_api::Command>(
                    &self.host.storage,
                    person.id,
                )
                .expect("mind exists");
                game_society::personal::SpeakerFacts {
                    actor: person.id,
                    area: match game_spatial::location(
                        &self.host.storage,
                        game_ecs::StableId(person.id.0),
                    ) {
                        Some(Location::AtArea(area)) => Some(*area),
                        _ => None,
                    },
                    speech: self.social_speech(person.id),
                    guardian: person.role == Role::Guardian,
                    autonomous: person.id != self.host.state.ruler,
                    adult: self.social_adult(person.id),
                    offers_teaching: mind.offers_teaching(),
                    knows_routes: !mind.routes.is_empty(),
                    practice: game_skills::skills(&self.host.storage, person.id)
                        .map(|row| {
                            row.records
                                .iter()
                                .map(|(skill, progress)| (*skill, progress.practice_ms))
                                .collect()
                        })
                        .unwrap_or_default(),
                }
            })
            .collect()
    }
    pub(crate) fn review_social(&mut self) -> Result<(), Error> {
        let now = self.host.clock().time;
        let Some(world) = &self.host.state.social else {
            return Ok(());
        };
        if !game_society::personal::review_due(&world.rules, world.last_review, now)? {
            return Ok(());
        }
        let margin = world.rules.aspiration_margin;
        let facts = personal::personals(&self.host.storage)
            .map(|(actor, _)| game_society::personal::AspirationFacts {
                actor,
                is_current_ruler: actor == self.host.state.ruler,
                role: self.person_at(self.person_index[&actor]).role,
                independent: self.independent(actor),
                vitals: game_physiology::body(&self.host.storage, actor)
                    .expect("body exists")
                    .vitals
                    .clone(),
            })
            .collect::<Vec<_>>();
        game_society::personal::develop_aspirations::<game_api::Command>(
            &mut self.host.storage,
            &facts,
            margin,
        );
        self.host.state.social.as_mut().unwrap().last_review = now;
        let roster = self.social_speaker_facts();
        game_society::personal::publish_teaching_offers(
            self.host.state.social.as_ref().unwrap(),
            &mut self.host.storage,
            &roster,
            now,
        )?;
        let speakers = game_society::personal::review_speakers(&roster, &self.host.storage, now);
        let mut speaker_index = game_society::personal::SpeakerIndex::new(roster);
        for actor in speakers {
            if !speaker_index.can_choose(&self.host.storage, actor, now) {
                continue;
            }
            let world = self.host.state.social.as_ref().unwrap();
            let chosen = speaker_index.choose(
                world,
                &self.host.storage,
                game_society::personal::PersonalChoice {
                    actor,
                    mind: game_characters::decisions::mind::<game_api::Command>(
                        &self.host.storage,
                        actor,
                    )
                    .expect("mind exists"),
                    now,
                    levels: [
                        Skill::Craft,
                        Skill::Farming,
                        Skill::Medicine,
                        Skill::Negotiation,
                        Skill::Leadership,
                    ]
                    .into_iter()
                    .map(|skill| (skill, self.own_skill_level(actor, skill)))
                    .collect(),
                    craft: game_society::personal::CraftAvailability {
                        tool: self.social_tool(actor),
                        timber: self.social_material(
                            actor,
                            Good::Timber,
                            world.rules.practice_material,
                        ),
                    },
                },
            );
            if let Some(action) = chosen {
                let partner = match action {
                    SocialAction::Talk { partner, .. } => Some(partner),
                    SocialAction::Learn { teacher, .. } => Some(teacher),
                    _ => None,
                };
                match self.start_social(actor, action) {
                    Ok(()) | Err(Error::Unavailable) => {}
                    Err(error) => return Err(error),
                }
                speaker_index.set_speech(actor, self.social_speech(actor));
                if let Some(partner) = partner {
                    speaker_index.set_speech(partner, self.social_speech(partner));
                }
            }
            self.host
                .state
                .social
                .as_ref()
                .unwrap()
                .finish_personal_choice(&mut self.host.storage, actor, now)?;
        }
        Ok(())
    }
    pub(crate) fn available_social_commands(&self, actor: PersonId) -> Vec<Command> {
        let Some(world) = self.host.state.social.as_ref() else {
            return Vec::new();
        };
        if !self.social_speech(actor) {
            return Vec::new();
        }
        let facts = self.social_person_facts(actor, actor);
        let roster = self
            .people()
            .map(|person| self.social_person_facts(person.id, actor))
            .collect::<Vec<_>>();
        game_society::personal::available_actions(
            world,
            &self.host.storage,
            &facts,
            &roster,
            self.combat_restricts(actor),
        )
        .into_iter()
        .map(|action| Command::Social { action })
        .collect()
    }
    pub(crate) fn social_view(&self, actor: PersonId) -> Option<SocialView> {
        self.host
            .state
            .social
            .as_ref()?
            .view(&self.host.storage, actor)
    }
}

fn bind_personal_activity(activity: personal::SocialActivity) -> Activity {
    match activity {
        personal::SocialActivity::Conversation {
            partner,
            topic,
            completes,
        } => Activity::Social {
            partner,
            topic,
            completes,
        },
        personal::SocialActivity::Instruction {
            teacher,
            student,
            topic,
            completes,
        } => Activity::Instruction {
            teacher,
            student,
            topic,
            completes,
        },
        personal::SocialActivity::Practice {
            skill,
            tool,
            completes,
        } => Activity::Practice {
            skill,
            tool: Some(tool),
            completes,
        },
    }
}
fn bind_social_activity_kind(activity: &Activity) -> personal::SocialActivityKind {
    match activity {
        Activity::Social { .. } => personal::SocialActivityKind::Conversation,
        Activity::Instruction { .. } => personal::SocialActivityKind::Instruction,
        Activity::Practice { .. } => personal::SocialActivityKind::Practice,
        _ => personal::SocialActivityKind::Other,
    }
}

fn bind_social_activity(activity: &Activity) -> game_society::personal::BoundActivity {
    use game_society::personal::BoundActivity;
    match *activity {
        Activity::Social {
            partner,
            topic,
            completes,
        } => BoundActivity::Talk {
            partner,
            topic,
            completes,
        },
        Activity::Instruction {
            teacher,
            student,
            topic,
            completes,
        } => BoundActivity::Instruction {
            teacher,
            student,
            topic,
            completes,
        },
        Activity::Practice {
            skill,
            tool,
            completes,
        } => BoundActivity::Practice {
            skill,
            tool,
            completes,
        },
        _ => BoundActivity::Other,
    }
}
pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.state.social else {
        return Ok(());
    };
    let mut deadlines = BTreeMap::<PersonId, BTreeSet<SimTime>>::new();
    for (at, entries) in &state.state.due {
        for entry in entries {
            if let Due::Physical(actor) = entry {
                deadlines.entry(*actor).or_default().insert(*at);
            }
        }
    }
    let actors = state
        .components
        .activities()
        .map(|(actor, activity)| {
            let body = game_physiology::body(state.components, actor);
            (
                actor,
                game_society::personal::ActorReference {
                    has_mind: game_characters::decisions::mind::<game_api::Command>(
                        state.components,
                        actor,
                    )
                    .is_some(),
                    alive: body.is_some_and(|body| body.vitals.alive),
                    area: match game_spatial::location(
                        state.components,
                        game_ecs::StableId(actor.0),
                    ) {
                        Some(Location::AtArea(area)) => Some(*area),
                        _ => None,
                    },
                    activity: Some(bind_social_activity(activity)),
                    physical_deadlines: deadlines.remove(&actor).unwrap_or_default(),
                },
            )
        })
        .collect();
    game_society::personal::validate_context(
        world,
        state.components,
        state.time,
        &game_society::personal::ValidationFacts {
            human_domains: state.state.physical.is_some() && state.state.decisions.is_some(),
            people: state.people().map(|person| person.id).collect(),
            areas: state.state.areas.iter().map(|area| area.id).collect(),
            actors,
            stocks: game_cargo::inventories(state.components)
                .map(|(id, row)| (id, row.good))
                .collect(),
            practice_per_level: state
                .state
                .skills
                .as_ref()
                .map_or(0, |runtime| runtime.rules.per_level.0),
            maximum_level: state
                .state
                .skills
                .as_ref()
                .map_or(0, |runtime| runtime.rules.max_level),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(workers: usize) -> Simulation {
        let mut sim = Simulation::skills_fixture(
            37,
            Execution {
                workers,
                reverse_jobs: true,
            },
        )
        .unwrap();
        let village = sim.host.state.areas[0].id;
        for person_id in sim.people().map(|person| person.id).collect::<Vec<_>>() {
            *game_spatial::location_mut(&mut sim.host.storage, game_ecs::StableId((person_id).0))
                .expect("location exists") = Location::AtArea(village);
            game_transport::replace_route(&mut sim.host.storage, person_id, Vec::new())
                .expect("actor route");
        }
        sim.host
            .state
            .due
            .values_mut()
            .for_each(|due| due.retain(|entry| !matches!(entry, Due::Arrive(_) | Due::Age(_))));
        let rules = &mut sim.host.state.development.as_mut().unwrap().rules;
        rules.lifespan = SimDuration(5_000_000);
        let ids: Vec<_> = sim.people().map(|p| p.id).collect();
        for actor in &ids {
            sim.schedule_age(*actor);
        }
        // Ordinary chosen needs remain real, with sufficient authored supplies.
        for actor in ids {
            sim.interrupt_actor_activity(actor);
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.personality.sociability = 950;
            mind.personality.empathy = 950;
            mind.personality.patience = 950;
        }
        sim.initialize_social(Rules {
            aspiration_margin: 200,
            ..Rules::fixture()
        })
        .unwrap();
        sim
    }
    fn cancel(sim: &mut Simulation, actor: PersonId) {
        sim.debug_actor_command(actor, Command::Cancel).unwrap();
    }
    fn social(sim: &mut Simulation, actor: PersonId, action: SocialAction) {
        sim.start_social(actor, action).unwrap();
    }
    fn actual_birth(sim: &mut Simulation, first: PersonId, second: PersonId) -> PersonId {
        // The relocated peer acquires an actual local report, rather than a
        // fixture granting knowledge merely because its body changed area.
        let food = game_characters::decisions::stocks(&sim.host.storage, first)
            .unwrap()
            .known
            .values()
            .find(|fact| fact.good == Good::Food && fact.accessible && fact.quantity.0 >= 3_000)
            .unwrap()
            .inventory;
        sim.debug_actor_command(
            first,
            Command::Tell {
                recipient: second,
                inventory: food,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        for (actor, partner) in [(first, second), (second, first)] {
            cancel(sim, actor);
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.dream = decisions::Aspiration::Parenthood;
            mind.relationships.insert(
                partner,
                decisions::Relationship {
                    affection: 800,
                    trust: 800,
                },
            );
        }
        for topic in [SocialTopic::Companionship, SocialTopic::FamilyPlans] {
            for actor in [first, second] {
                cancel(sim, actor);
            }
            social(
                sim,
                first,
                SocialAction::Talk {
                    partner: second,
                    topic,
                },
            );
            sim.advance(MINUTE).unwrap();
        }
        let active = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .any(|pregnancy| {
                pregnancy.gestator == first
                    && matches!(
                        pregnancy.state,
                        development::PregnancyState::Conceiving { .. }
                            | development::PregnancyState::Gestating { .. }
                    )
            });
        if !active {
            sim.start_conception(first, second).unwrap();
        }
        let birth_at = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .find_map(|pregnancy| {
                if pregnancy.gestator != first {
                    return None;
                }
                match pregnancy.state {
                    development::PregnancyState::Conceiving { completes } => {
                        Some(completes.0 + 180_000)
                    }
                    development::PregnancyState::Gestating { due } => Some(due.0),
                    _ => None,
                }
            })
            .expect("the mutually chosen conception must be active");
        sim.advance(SimDuration(birth_at - sim.host.clock().time.0))
            .unwrap();
        // This authored pair now prefers supporting their actual child. The
        // fixture does not assume that their old intent obliges more births.
        for actor in [first, second] {
            game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap()
            .dream = decisions::Aspiration::SupportOthers;
        }
        game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .filter_map(|p| match p.state {
                development::PregnancyState::Born(child)
                    if pair(p.gestator, p.partner) == pair(first, second) =>
                {
                    Some(child)
                }
                _ => None,
            })
            .last()
            .unwrap()
    }
    #[test]
    fn ordinary_guidance_is_coupled_timed_and_transmits_only_frozen_remembered_routes() {
        let mut sim = fixture(1);
        let teacher = sim.person_at(3).id;
        let student = sim.person_at(4).id;
        let village = sim.host.state.areas[0].id;
        let farm = sim.host.state.areas[1].id;
        let forest = sim.host.state.areas[2].id;
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, student)
            .unwrap()
            .routes
            .clear();
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, teacher)
            .unwrap()
            .routes = BTreeMap::from([(farm, BTreeMap::from([(village, MINUTE)]))]);
        social(
            &mut sim,
            student,
            SocialAction::Learn {
                teacher,
                topic: InstructionTopic::Routes,
            },
        );
        assert!(matches!(
            *sim.host.storage.activity(teacher).expect("body exists"),
            Activity::Instruction { .. }
        ));
        let frozen =
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, teacher)
                .unwrap()
                .routes
                .clone();
        game_characters::decisions::mind_mut::<game_api::Command>(&mut sim.host.storage, teacher)
            .unwrap()
            .routes
            .insert(forest, BTreeMap::from([(farm, MINUTE)]));
        sim.advance(SimDuration(59_999)).unwrap();
        assert!(
            !game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, student)
                .unwrap()
                .routes
                .contains_key(&farm)
        );
        sim.advance(SimDuration(1)).unwrap();
        let known =
            &game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, student)
                .unwrap()
                .routes;
        assert_eq!(known.get(&farm), frozen.get(&farm));
        assert!(!known.contains_key(&forest));
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::InstructionCompleted{teacher:t,student:s,..}if t==teacher&&s==student)));
    }
    #[test]
    fn practical_trials_use_real_materials_and_partial_time_survives_cancellation() {
        let mut sim = fixture(1);
        let actor = sim.person_at(3).id;
        // Local observation is acquired through the ordinary simulation review.
        sim.advance(SimDuration(1)).unwrap();
        cancel(&mut sim, actor);
        let timber = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Timber
                    && stock.location == InventoryLocation::Area(sim.host.state.areas[0].id)
            })
            .unwrap()
            .id;
        let before = game_cargo::inventory(&sim.host.storage, timber)
            .expect("inventory exists")
            .quantity
            .0;
        let prior = game_skills::skills(&sim.host.storage, actor)
            .map(|row| &row.records)
            .and_then(|records| records.get(&Skill::Craft))
            .map_or(0, |progress| progress.practice_ms);
        social(
            &mut sim,
            actor,
            SocialAction::Practice {
                skill: Skill::Craft,
            },
        );
        assert_eq!(
            game_cargo::inventory(&sim.host.storage, timber)
                .expect("inventory exists")
                .quantity
                .0,
            before - 100
        );
        sim.advance(SimDuration(30_000)).unwrap();
        cancel(&mut sim, actor);
        assert_eq!(
            game_skills::skills(&sim.host.storage, actor)
                .unwrap()
                .records[&Skill::Craft]
                .practice_ms,
            prior + 30_000
        );
        sim.debug_set_personality(
            actor,
            decisions::Personality::restful(),
            decisions::Aspiration::Comfort,
        )
        .unwrap();
        let own = game_skills::skills(&sim.host.storage, actor)
            .unwrap()
            .records[&Skill::Craft]
            .practice_ms;
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(
            game_skills::skills(&sim.host.storage, actor)
                .unwrap()
                .records[&Skill::Craft]
                .practice_ms,
            own
        );
    }
    #[test]
    fn real_tool_removal_interrupts_a_trial_without_later_ghost_practice() {
        let mut sim = fixture(1);
        let actor = sim.person_at(3).id;
        let carrier = sim.person_at(4).id;
        sim.host.state.social.as_mut().unwrap().rules.practice = SimDuration(180_000);
        sim.advance(SimDuration(1)).unwrap();
        cancel(&mut sim, actor);
        cancel(&mut sim, carrier);
        social(
            &mut sim,
            actor,
            SocialAction::Practice {
                skill: Skill::Craft,
            },
        );
        let tool = sim.social_selected_tool(actor).unwrap();
        assert!(!sim.tool_available_for(carrier, tool));
        sim.debug_actor_command(
            carrier,
            Command::Carry {
                source: tool,
                quantity: Quantity(1_000),
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(personal::interactions(&sim.host.storage).next().is_none());
        let learned = game_skills::skills(&sim.host.storage, actor)
            .unwrap()
            .records[&Skill::Craft]
            .practice_ms;
        sim.debug_set_personality(
            actor,
            decisions::Personality::restful(),
            decisions::Aspiration::Comfort,
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            game_skills::skills(&sim.host.storage, actor)
                .unwrap()
                .records[&Skill::Craft]
                .practice_ms,
            learned
        );
        assert!(!sim.host.events().iter().any(|event| matches!(event.event, EventKind::PracticeCompleted { actor: worker, .. } if worker == actor)));
    }

    #[test]
    fn unsupported_or_impossible_requests_are_inert_and_refusals_are_actual_answers() {
        let mut sim = fixture(1);
        let actor = sim.person_at(3).id;
        let recipient = sim.person_at(4).id;
        let before = sim.canonical_snapshot().unwrap();
        assert_eq!(
            sim.start_social(
                actor,
                SocialAction::Learn {
                    teacher: recipient,
                    topic: InstructionTopic::Skill(Skill::Invocation)
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        assert_eq!(
            sim.start_social(
                actor,
                SocialAction::Practice {
                    skill: Skill::Combat
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.canonical_snapshot().unwrap(), before);
        game_characters::decisions::mind_mut::<game_api::Command>(
            &mut sim.host.storage,
            recipient,
        )
        .unwrap()
        .dream = decisions::Aspiration::Comfort;
        social(
            &mut sim,
            actor,
            SocialAction::Talk {
                partner: recipient,
                topic: SocialTopic::FamilyPlans,
            },
        );
        assert!(!sim.social_family_intent(actor, recipient));
        assert!(sim.host.events().iter().any(|event|matches!(event.event,EventKind::SocialRefused{actor:a,recipient:r,..}if a==actor&&r==recipient)));
        assert!(matches!(
            *sim.host.storage.activity(recipient).expect("body exists"),
            Activity::Idle | Activity::Rest
        ));
    }
    #[test]
    fn ordinary_children_remember_chosen_care_and_reach_their_own_adult_family_decisions() {
        let mut sim = fixture(1);
        let first = sim.person_at(3).id;
        let second = sim.person_at(4).id;
        let child = actual_birth(&mut sim, first, second);
        assert!(
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, child)
                .unwrap()
                .routes
                .len()
                <= 1
        );
        assert!(
            sim.own_skill_view(child)
                .iter()
                .all(|progress| progress.practice_ms == 0)
        );
        // Ordinary voluntary caregivers support this child through actual meals.
        sim.advance(SimDuration(660_000)).unwrap();
        assert!(
            personal::personal(&sim.host.storage, child)
                .unwrap()
                .development
                .experiences
                > 0
        );
        assert_eq!(
            game_lifecycle::life(&sim.host.storage, child)
                .unwrap()
                .stage,
            LifeStage::Adult
        );
        assert_ne!(
            game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, child)
                .unwrap()
                .dream,
            decisions::Aspiration::Comfort
        );
        assert!(
            game_lifecycle::life(&sim.host.storage, child)
                .unwrap()
                .caregivers
                .iter()
                .any(|id| sim.person_at(sim.person_index[id]).role == Role::Citizen)
        );
        validate(&sim.canonical_state()).unwrap();
        let restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: false,
            },
        )
        .unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
    }
    #[test]
    fn ordinary_interactions_are_save_chunk_worker_and_query_independent() {
        fn run(workers: usize, chunked: bool) -> Simulation {
            let mut sim = fixture(workers);
            let first = sim.person_at(3).id;
            let second = sim.person_at(4).id;
            social(
                &mut sim,
                first,
                SocialAction::Talk {
                    partner: second,
                    topic: SocialTopic::Companionship,
                },
            );
            let saved = sim.save().unwrap();
            let mut sim = Simulation::load(
                &saved,
                Execution {
                    workers,
                    reverse_jobs: false,
                },
            )
            .unwrap();
            if chunked {
                for _ in 0..60 {
                    let _ = sim.observe();
                    sim.advance(SimDuration(1_000)).unwrap();
                }
            } else {
                sim.advance(MINUTE).unwrap();
            }
            sim
        }
        assert_eq!(
            run(1, false).canonical_snapshot().unwrap(),
            run(4, true).canonical_snapshot().unwrap()
        );
    }
}

#[cfg(test)]
mod generations {
    use super::*;
    #[test]
    fn observed_upbringing_chosen_peer_plans_and_actual_permissions_allow_another_generation() {
        let config = scenarios::Config {
            genetics: None,
            biology: development::Rules {
                conception_time: MINUTE,
                gestation: SimDuration(180_000),
                infancy: SimDuration(300_000),
                adulthood: SimDuration(600_000),
                elderhood: SimDuration(365 * 86_400_000),
                lifespan: SimDuration(2 * 365 * 86_400_000),
            },
            social: Rules {
                aspiration_margin: 150,
                ..Rules::fixture()
            },
            ..scenarios::Config::default()
        };
        let mut sim = Simulation::playable_scenario(
            "settlement-founder",
            0,
            Execution {
                workers: 1,
                reverse_jobs: false,
            },
            config,
        )
        .unwrap();
        let parents = [
            (sim.person_at(3).id, sim.person_at(4).id),
            (sim.person_at(5).id, sim.person_at(6).id),
        ];
        let mut children = Vec::new();
        // These additional adults begin this fixture with their own intention;
        // later children develop aims from the actual care and activities below.
        for actor in [parents[1].0, parents[1].1] {
            let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                &mut sim.host.storage,
                actor,
            )
            .unwrap();
            mind.dream = decisions::Aspiration::Parenthood;
            mind.personality.empathy = 650;
            mind.personality.sociability = 1_000;
            mind.personality.discipline = 0;
            mind.personality.ambition = 0;
            mind.personality.hedonism = 400;
            let mut record = baseline(&mind.personality, mind.dream);
            record.next_choice = sim.host.clock().time;
            personal::personal_mut(&mut sim.host.storage, actor)
                .unwrap()
                .development = record;
        }
        for (gestator, partner) in parents {
            let already_chosen = game_lifecycle::all_pregnancies(&sim.host.storage)
                .map(|(_, row)| row)
                .any(|pregnancy| {
                    pregnancy.gestator == gestator
                        && matches!(
                            pregnancy.state,
                            development::PregnancyState::Gestating { .. }
                        )
                });
            for (actor, other) in [(gestator, partner), (partner, gestator)] {
                sim.debug_actor_command(actor, Command::Cancel).unwrap();
                let mind = game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut sim.host.storage,
                    actor,
                )
                .unwrap();
                mind.dream = decisions::Aspiration::Parenthood;
                mind.personality.empathy = 650;
                mind.personality.sociability = 1_000;
                mind.personality.discipline = 0;
                mind.personality.ambition = 0;
                mind.personality.hedonism = 400;
                mind.relationships.insert(
                    other,
                    decisions::Relationship {
                        affection: 800,
                        trust: 800,
                    },
                );
            }
            if !already_chosen {
                for topic in [SocialTopic::Companionship, SocialTopic::FamilyPlans] {
                    for actor in [gestator, partner] {
                        sim.debug_actor_command(actor, Command::Cancel).unwrap();
                    }
                    sim.start_social(gestator, SocialAction::Talk { partner, topic })
                        .unwrap();
                    sim.advance(MINUTE).unwrap();
                }
                let active = game_lifecycle::all_pregnancies(&sim.host.storage)
                    .map(|(_, row)| row)
                    .any(|pregnancy| {
                        pregnancy.gestator == gestator
                            && matches!(
                                pregnancy.state,
                                development::PregnancyState::Conceiving { .. }
                                    | development::PregnancyState::Gestating { .. }
                            )
                    });
                if !active {
                    sim.start_conception(gestator, partner).unwrap();
                }
            }
            let birth_at = game_lifecycle::all_pregnancies(&sim.host.storage)
                .map(|(_, row)| row)
                .find_map(|pregnancy| {
                    if pregnancy.gestator != gestator {
                        return None;
                    }
                    match pregnancy.state {
                        development::PregnancyState::Conceiving { completes } => {
                            Some(completes.0 + 180_000)
                        }
                        development::PregnancyState::Gestating { due } => Some(due.0),
                        _ => None,
                    }
                })
                .unwrap();
            sim.advance(SimDuration(birth_at - sim.host.clock().time.0))
                .unwrap();
            let child = game_lifecycle::all_pregnancies(&sim.host.storage)
                .map(|(_, row)| row)
                .find_map(|pregnancy| match pregnancy.state {
                    development::PregnancyState::Born(child) if pregnancy.gestator == gestator => {
                        Some(child)
                    }
                    _ => None,
                })
                .unwrap();
            children.push(child);
            for actor in [gestator, partner] {
                game_characters::decisions::mind_mut::<game_api::Command>(
                    &mut sim.host.storage,
                    actor,
                )
                .unwrap()
                .dream = decisions::Aspiration::SupportOthers;
            }
        }
        let food = game_cargo::inventories(&sim.host.storage)
            .map(|(_, row)| row)
            .find(|stock| {
                stock.good == Good::Food
                    && stock.location == InventoryLocation::Area(sim.host.state.areas[0].id)
                    && stock.quantity.0 > 10_000
            })
            .unwrap()
            .id;
        // Chosen caregiving is performed by actual adults with their own access.
        for child in &children {
            for _ in 0..3 {
                let caregiver = parents[0].0;
                sim.debug_actor_command(caregiver, Command::Cancel).unwrap();
                sim.debug_actor_command(*child, Command::Cancel).unwrap();
                sim.debug_actor_command(
                    caregiver,
                    Command::Care {
                        child: *child,
                        source: food,
                    },
                )
                .unwrap();
                sim.advance(MINUTE).unwrap();
            }
        }
        let adult_at = children
            .iter()
            .map(|child| {
                game_lifecycle::life(&sim.host.storage, *child)
                    .unwrap()
                    .born
                    .0
                    + 600_000
            })
            .max()
            .unwrap();
        if sim.host.clock().time.0 < adult_at {
            sim.advance(SimDuration(adult_at - sim.host.clock().time.0))
                .unwrap();
        }
        for child in &children {
            assert_eq!(
                game_lifecycle::life(&sim.host.storage, *child)
                    .unwrap()
                    .stage,
                LifeStage::Adult
            );
            assert_eq!(
                game_characters::decisions::mind::<game_api::Command>(&sim.host.storage, *child)
                    .unwrap()
                    .dream,
                decisions::Aspiration::Parenthood,
                "child {child:?} at {:?}: {:?}, body {:?}",
                sim.host.clock().time,
                personal::personal(&sim.host.storage, *child)
                    .unwrap()
                    .development,
                *sim.host.storage.activity(*child).expect("body exists"),
            );
            // Institutional rights may emerge independently; personal access
            // permissions are not copied from either parent.
            assert!(
                !game_cargo::inventory(&sim.host.storage, food)
                    .expect("inventory exists")
                    .access
                    .contains(child)
            );
        }
        assert_ne!(
            game_lifecycle::life(&sim.host.storage, children[0])
                .unwrap()
                .reproduction,
            game_lifecycle::life(&sim.host.storage, children[1])
                .unwrap()
                .reproduction,
            "this seeded pair must have compatible biological roles; pregnancies {:?}",
            game_lifecycle::all_pregnancies(&sim.host.storage)
                .map(|(_, row)| row)
                .map(|pregnancy| (pregnancy.id, &pregnancy.state))
                .collect::<Vec<_>>()
        );
        // Each new adult obtains chosen access through an actual local promise,
        // rather than inheriting the parents' title or old people's permissions.
        let grantor = game_property::inventory_custodian(&sim.host.storage, food).unwrap();
        for child in &children {
            sim.debug_actor_command(grantor, Command::Cancel).unwrap();
            sim.debug_actor_command(*child, Command::Cancel).unwrap();
            sim.debug_agreement_action(
                grantor,
                AgreementAction::Propose {
                    parties: BTreeSet::from([grantor, *child]),
                    terms: vec![AgreementTerm::Access {
                        grantor,
                        grantee: *child,
                        inventory: food,
                        allowed: true,
                    }],
                    procedure: None,
                },
            )
            .unwrap();
            let agreement = game_agreements::agreements(&sim.host.storage)
                .map(|(id, _)| id)
                .last()
                .unwrap();
            sim.debug_agreement_action(*child, AgreementAction::Accept { agreement })
                .unwrap();
        }
        let first = children[0];
        let second = children[1];
        for _ in 0..2 {
            for actor in children.iter().copied() {
                sim.debug_actor_command(actor, Command::Cancel).unwrap();
            }
            sim.start_social(
                first,
                SocialAction::Talk {
                    partner: second,
                    topic: SocialTopic::Companionship,
                },
            )
            .unwrap();
            sim.advance(MINUTE).unwrap();
        }
        for actor in children.iter().copied() {
            sim.debug_actor_command(actor, Command::Cancel).unwrap();
        }
        sim.start_social(
            first,
            SocialAction::Talk {
                partner: second,
                topic: SocialTopic::FamilyPlans,
            },
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert!(sim.social_family_intent(first, second));
        for actor in children.iter().copied() {
            sim.debug_actor_command(actor, Command::Cancel).unwrap();
        }
        sim.start_conception(first, second).unwrap();
        sim.advance(SimDuration(240_000)).unwrap();
        let grandchild = game_lifecycle::all_pregnancies(&sim.host.storage)
            .map(|(_, row)| row)
            .find_map(|pregnancy| match pregnancy.state {
                development::PregnancyState::Born(child)
                    if pair(pregnancy.gestator, pregnancy.partner) == pair(first, second) =>
                {
                    Some(child)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            game_lifecycle::life(&sim.host.storage, grandchild)
                .unwrap()
                .parents
                .len(),
            2
        );
        assert!(
            sim.own_skill_view(grandchild)
                .iter()
                .all(|progress| progress.practice_ms == 0)
        );
        assert!(
            !game_cargo::inventory(&sim.host.storage, food)
                .expect("inventory exists")
                .access
                .contains(&grandchild)
        );
        assert!(
            game_society::institutions(&sim.host.storage)
                .map(|(_, row)| row)
                .all(|institution| institution
                    .charter
                    .offices
                    .iter()
                    .all(|office| office.holder != grandchild))
        );
        let restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: true,
            },
        )
        .unwrap();
        assert_eq!(sim.canonical_state(), restored.canonical_state());
    }
}
