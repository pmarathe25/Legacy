//! Local chosen experiences change personal aims. Teaching transmits only the
//! teacher's remembered information and mastery; family intent is a conversation,
//! not marriage, custody, title, or permanent consent to conception.
use crate::*;

use game_characters::social::{ConversationInput, Temperament, UpbringingEvidence, pair};
pub use game_characters::social::{Development, Interaction, Offer, Rules, World};

fn temperament(personality: &decisions::Personality) -> Temperament {
    Temperament {
        discipline: personality.discipline,
        ambition: personality.ambition,
        sociability: personality.sociability,
        hedonism: personality.hedonism,
        empathy: personality.empathy,
    }
}
fn baseline(personality: &decisions::Personality, current: decisions::Aspiration) -> Development {
    Development::baseline(temperament(personality), current)
        .expect("validated personal temperament")
}
impl Simulation {
    pub fn initialize_social(&mut self, rules: Rules) -> Result<(), Error> {
        if !rules.valid()
            || self.state.decisions.is_none()
            || self.state.development.is_none()
            || self.state.physical.is_none()
        {
            return Err(Error::InvalidConfiguration(
                "invalid social development rules or missing human domains".into(),
            ));
        }
        let development = self
            .state
            .decisions
            .as_ref()
            .unwrap()
            .minds
            .iter()
            .map(|(id, mind)| {
                let mut record = baseline(&mind.personality, mind.dream);
                record.next_choice = self.state.time;
                (*id, record)
            })
            .collect();
        self.state.social = Some(World {
            rules,
            development,
            last_review: self.state.time,
            ..World::default()
        });
        Ok(())
    }
    pub(crate) fn social_family_intent(&self, first: PersonId, second: PersonId) -> bool {
        self.state
            .social
            .as_ref()
            .is_none_or(|world| world.family_intent.contains(&pair(first, second)))
    }
    fn social_adult(&self, actor: PersonId) -> bool {
        self.state
            .development
            .as_ref()
            .and_then(|w| w.lives.get(&actor))
            .is_some_and(|life| matches!(life.stage, LifeStage::Adult | LifeStage::Elder))
    }
    fn social_speech(&self, actor: PersonId) -> bool {
        self.person_index
            .get(&actor)
            .is_some_and(|index| matches!(self.state.people[*index].location, Location::AtArea(_)))
            && self.independent(actor)
            && !self.in_combat(actor)
            && self
                .state
                .physical
                .as_ref()
                .and_then(|_| self.state.components.body(actor))
                .is_some_and(|body| {
                    body.vitals.alive
                        && body.vitals.health > 0
                        && matches!(
                            self.state.components.activity(actor),
                            Some(Activity::Idle | Activity::Rest)
                        )
                })
            && !self.care_index.contains_key(&actor)
            && self.conversation_actor_available(actor)
    }
    fn social_local_pair(&self, first: PersonId, second: PersonId) -> bool {
        first != second
            && self.social_speech(first)
            && self.social_speech(second)
            && self.state.people[self.person_index[&first]].location
                == self.state.people[self.person_index[&second]].location
    }
    fn relationship(&self, first: PersonId, second: PersonId) -> decisions::Relationship {
        self.state.decisions.as_ref().unwrap().minds[&first]
            .relationships
            .get(&second)
            .cloned()
            .unwrap_or(decisions::Relationship {
                affection: 0,
                trust: 0,
            })
    }
    fn conversation_willing(
        &self,
        recipient: PersonId,
        sender: PersonId,
        topic: SocialTopic,
    ) -> bool {
        let mind = &self.state.decisions.as_ref().unwrap().minds[&recipient];
        let relation = self.relationship(recipient, sender);
        let body = self.state.components.body(recipient).expect("body exists");
        let physical = self.body_rules(recipient);
        game_characters::social::conversation_willing(
            topic,
            ConversationInput {
                temperament: temperament(&mind.personality),
                affection: relation.affection,
                trust: relation.trust,
                hunger: body.vitals.hunger,
                meal_relief: physical.meal_relief,
                fatigue: body.vitals.fatigue,
                fatigue_limit: physical.fatigue_limit,
                recipient_adult: self.social_adult(recipient),
                sender_adult: self.social_adult(sender),
                recipient_aspiration: mind.dream,
                chosen_companionship: self
                    .state
                    .social
                    .as_ref()
                    .unwrap()
                    .companionship
                    .contains(&pair(recipient, sender)),
                visible_disruption: self.invocation_social_penalty(sender),
            },
        )
    }

    fn social_refusal(
        &mut self,
        actor: PersonId,
        recipient: PersonId,
        reason: &str,
    ) -> Result<(), Error> {
        self.ensure_event_capacity(1)?;
        self.state
            .social
            .as_mut()
            .unwrap()
            .approaches
            .entry(actor)
            .or_default()
            .insert(recipient, self.state.time);
        self.state
            .social
            .as_mut()
            .unwrap()
            .development
            .get_mut(&actor)
            .unwrap()
            .explanation = reason.into();
        self.emit(EventKind::SocialRefused {
            actor,
            recipient,
            reason: reason.into(),
        });
        self.revision += 1;
        Ok(())
    }
    fn social_tool(&self, actor: PersonId) -> Option<InventoryId> {
        self.state
            .decisions
            .as_ref()?
            .minds
            .get(&actor)?
            .knowledge
            .values()
            .filter(|known| {
                known.good == Good::Tool && known.accessible && known.quantity.0 >= 1_000
            })
            .find_map(|known| {
                self.state
                    .inventories
                    .get(&known.inventory)
                    .filter(|stock| {
                        self.has_stock_access(actor, stock)
                            && self.tool_available_for(actor, stock.id)
                    })
                    .map(|stock| stock.id)
            })
    }
    pub(crate) fn social_practical_actor(&self, actor: PersonId) -> bool {
        if self.state.physical.is_none() {
            return false;
        }
        !self.combat_restricts(actor)
            && self
                .state
                .components
                .body(actor)
                .is_some_and(|body| body.vitals.fatigue < self.body_rules(actor).fatigue_limit)
    }
    fn social_material(
        &self,
        actor: PersonId,
        good: Good,
        quantity: Quantity,
    ) -> Option<InventoryId> {
        let Location::AtArea(area) = self.state.people[*self.person_index.get(&actor)?].location
        else {
            return None;
        };
        self.state
            .decisions
            .as_ref()?
            .minds
            .get(&actor)?
            .knowledge
            .values()
            .filter(|known| known.good == good && known.accessible && known.quantity >= quantity)
            .find_map(|known| {
                self.state
                    .inventories
                    .get(&known.inventory)
                    .filter(|stock| {
                        stock.good == good
                            && stock.quantity >= quantity
                            && self.inventory_area(stock) == Some(area)
                            && self.has_stock_access(actor, stock)
                    })
                    .map(|stock| stock.id)
            })
    }
    pub(crate) fn start_social(
        &mut self,
        actor: PersonId,
        action: SocialAction,
    ) -> Result<(), Error> {
        let world = self.state.social.as_ref().ok_or(Error::Unavailable)?;
        if !self.social_speech(actor) || !world.development.contains_key(&actor) {
            return Err(Error::Unavailable);
        }
        let rules = world.rules.clone();
        let mut materials = None;
        let interaction = match action {
            SocialAction::Talk { partner, topic } => {
                if !self.social_local_pair(actor, partner)
                    || topic == SocialTopic::FamilyPlans
                        && (!self.social_adult(actor) || !self.social_adult(partner))
                {
                    return Err(Error::Unavailable);
                }
                if !self.conversation_willing(partner, actor, topic) {
                    return self.social_refusal(
                        actor,
                        partner,
                        "They prefer not to discuss this now",
                    );
                }
                Interaction::Talk {
                    first: actor,
                    second: partner,
                    topic,
                    completes: SimTime(
                        self.state
                            .time
                            .0
                            .checked_add(rules.conversation.0)
                            .ok_or(Error::Overflow)?,
                    ),
                }
            }
            SocialAction::Learn { teacher, topic } => {
                if !self.social_local_pair(actor, teacher)
                    || topic == InstructionTopic::Skill(Skill::Invocation)
                {
                    return Err(Error::Unavailable);
                }
                let mind = &self.state.decisions.as_ref().unwrap().minds[&teacher];
                let relation = self.relationship(teacher, actor);
                if i32::from(mind.personality.empathy + mind.personality.patience) / 2
                    + relation.affection
                    + relation.trust
                    < 400
                {
                    return self.social_refusal(
                        actor,
                        teacher,
                        "They prefer to spend their time elsewhere",
                    );
                }
                let mut tool = None;
                let mastery_cap = match topic {
                    InstructionTopic::Routes => {
                        if mind.routes.is_empty() {
                            return self.social_refusal(
                                actor,
                                teacher,
                                "They do not know routes to teach",
                            );
                        }
                        0
                    }
                    InstructionTopic::Skill(skill) => {
                        let Some(progress) = self
                            .state
                            .skills
                            .as_ref()
                            .and_then(|w| w.records.get(&teacher))
                            .and_then(|r| r.get(&skill))
                        else {
                            return self.social_refusal(
                                actor,
                                teacher,
                                "They have no experience of that skill",
                            );
                        };
                        if progress.practice_ms == 0 {
                            return self.social_refusal(
                                actor,
                                teacher,
                                "They have no experience of that skill",
                            );
                        }
                        if matches!(
                            skill,
                            Skill::Craft | Skill::Farming | Skill::Medicine | Skill::Combat
                        ) {
                            if !self.social_practical_actor(actor)
                                || !self.social_practical_actor(teacher)
                            {
                                return Err(Error::Unavailable);
                            }
                            if skill == Skill::Combat {
                                return Err(Error::Unavailable);
                            }
                            if matches!(skill, Skill::Craft | Skill::Farming) {
                                tool = Some(self.social_tool(teacher).ok_or(Error::Unavailable)?);
                            }
                            let good = match skill {
                                Skill::Craft => Good::Timber,
                                Skill::Farming => Good::Seed,
                                _ => Good::Herb,
                            };
                            if skill == Skill::Farming
                                && !self.state.agriculture.as_ref().is_some_and(|w| {
                                    w.fields(&self.state.components)
                                        .map(|(_, value)| value)
                                        .any(|field| {
                                            field.area
                                                == match self.state.people
                                                    [self.person_index[&teacher]]
                                                    .location
                                                {
                                                    Location::AtArea(a) => a,
                                                    _ => unreachable!(),
                                                }
                                                && field.access.contains(&teacher)
                                        })
                                })
                            {
                                return Err(Error::Unavailable);
                            }
                            materials = Some((
                                teacher,
                                self.social_material(teacher, good, rules.practice_material)
                                    .ok_or(Error::Unavailable)?,
                                rules.practice_material,
                            ));
                        }
                        progress.practice_ms
                    }
                };
                Interaction::Instruction {
                    teacher,
                    student: actor,
                    topic,
                    routes: if topic == InstructionTopic::Routes {
                        mind.routes.clone()
                    } else {
                        BTreeMap::new()
                    },
                    mastery_cap,
                    tool,
                    completes: SimTime(
                        self.state
                            .time
                            .0
                            .checked_add(rules.instruction.0)
                            .ok_or(Error::Overflow)?,
                    ),
                }
            }
            SocialAction::Practice { skill } => {
                // Real craft trials consume timber and occupy a toolkit. Other
                // skills use their ordinary production, care, or social actions.
                if skill != Skill::Craft || !self.social_practical_actor(actor) {
                    return Err(Error::Unavailable);
                }
                let tool = self.social_tool(actor).ok_or(Error::Unavailable)?;
                let source = self
                    .social_material(actor, Good::Timber, rules.practice_material)
                    .ok_or(Error::Unavailable)?;
                materials = Some((actor, source, rules.practice_material));
                Interaction::Practice {
                    actor,
                    skill,
                    tool,
                    completes: SimTime(
                        self.state
                            .time
                            .0
                            .checked_add(rules.practice.0)
                            .ok_or(Error::Overflow)?,
                    ),
                }
            }
        };
        interaction
            .completes()
            .0
            .checked_add(rules.review.0)
            .ok_or(Error::Overflow)?;
        self.ensure_event_capacity(1)?;
        if let Interaction::Talk { second, .. } = interaction {
            self.state
                .social
                .as_mut()
                .unwrap()
                .approaches
                .entry(actor)
                .or_default()
                .insert(second, self.state.time);
        }
        let actors = interaction.actors();
        let completes = interaction.completes();
        if let Some((worker, source, quantity)) = materials {
            self.state.inventories.get_mut(&source).unwrap().quantity.0 -= quantity.0;
            self.emit(EventKind::Consumed {
                actor: worker,
                source,
                quantity,
            });
        }
        for id in &actors {
            self.interrupt_actor_activity(*id);
            let activity = match interaction {
                Interaction::Talk {
                    first,
                    second,
                    topic,
                    ..
                } => Activity::Social {
                    partner: if *id == first { second } else { first },
                    topic,
                    completes,
                },
                Interaction::Instruction {
                    teacher,
                    student,
                    topic,
                    ..
                } => Activity::Instruction {
                    teacher,
                    student,
                    topic,
                    completes,
                },
                Interaction::Practice { skill, tool, .. } => Activity::Practice {
                    skill,
                    tool: Some(tool),
                    completes,
                },
            };
            *self.state.components.activity_mut(*id).unwrap() = activity;
            self.state
                .due
                .entry(completes)
                .or_default()
                .push(Due::Physical(*id));
            self.state
                .social
                .as_mut()
                .unwrap()
                .participants
                .insert(*id, actor);
        }
        let world = self.state.social.as_mut().unwrap();
        for id in actors {
            let record = world.development.get_mut(&id).unwrap();
            record.next_choice = SimTime(
                completes
                    .0
                    .checked_add(rules.review.0)
                    .ok_or(Error::Overflow)?,
            );
            record.explanation = "I chose to spend time in this conversation or practice".into();
        }
        world.interactions.insert(actor, interaction);
        self.revision += 1;
        Ok(())
    }
    pub(crate) fn social_activity_is_practical(&self, actor: PersonId) -> bool {
        let Some(world) = &self.state.social else {
            return false;
        };
        let interaction = world
            .participants
            .get(&actor)
            .and_then(|key| world.interactions.get(key));
        matches!(
            interaction,
            Some(
                Interaction::Practice { .. }
                    | Interaction::Instruction {
                        topic: InstructionTopic::Skill(
                            Skill::Craft | Skill::Farming | Skill::Medicine | Skill::Combat
                        ),
                        ..
                    }
            )
        )
    }
    pub(crate) fn social_selected_tool(&self, actor: PersonId) -> Option<InventoryId> {
        let world = self.state.social.as_ref()?;
        match world.interactions.get(world.participants.get(&actor)?)? {
            Interaction::Instruction { teacher, tool, .. } if *teacher == actor => *tool,
            Interaction::Practice { tool, .. } => Some(*tool),
            _ => None,
        }
    }
    fn social_interaction_valid(&self, interaction: &Interaction) -> bool {
        let actors = interaction.actors();
        if actors.iter().any(|actor| {
            !self
                .state
                .components
                .body(*actor)
                .expect("body exists")
                .vitals
                .alive
                || self
                    .state
                    .components
                    .body(*actor)
                    .expect("body exists")
                    .vitals
                    .health
                    == 0
                || self.in_combat(*actor)
        }) {
            return false;
        }
        let location = &self.state.people[self.person_index[&actors[0]]].location;
        if !matches!(location, Location::AtArea(_))
            || actors
                .iter()
                .any(|actor| &self.state.people[self.person_index[actor]].location != location)
        {
            return false;
        }
        if actors.iter().any(|actor| {
            self.social_activity_is_practical(*actor) && !self.social_practical_actor(*actor)
        }) {
            return false;
        }
        let user = match interaction {
            Interaction::Instruction {
                teacher,
                tool: Some(_),
                ..
            } => Some(*teacher),
            Interaction::Practice { actor, .. } => Some(*actor),
            _ => None,
        };
        if let Some(user) = user {
            let Some(tool) = self.social_selected_tool(user) else {
                return false;
            };
            if !self.tool_available_for(user, tool)
                || actors
                    .iter()
                    .any(|actor| !self.social_practical_actor(*actor))
            {
                return false;
            }
        }
        true
    }
    pub(crate) fn social_practice_deltas(&self, next: SimTime) -> Vec<skills::PracticeDelta> {
        let Some(world) = &self.state.social else {
            return Vec::new();
        };
        let mut deltas = Vec::new();
        for interaction in world.interactions.values() {
            if !self.social_interaction_valid(interaction) {
                continue;
            }
            let elapsed = next
                .0
                .min(interaction.completes().0)
                .saturating_sub(self.state.time.0);
            let entries = match *interaction {
                Interaction::Talk { first, second, .. } => vec![
                    (first, Skill::Negotiation, None),
                    (second, Skill::Negotiation, None),
                ],
                Interaction::Instruction {
                    teacher,
                    student,
                    topic: InstructionTopic::Skill(skill),
                    mastery_cap,
                    ..
                } => vec![
                    (student, skill, Some(mastery_cap)),
                    (teacher, Skill::Leadership, None),
                ],
                Interaction::Instruction { teacher, .. } => {
                    vec![(teacher, Skill::Leadership, None)]
                }
                Interaction::Practice { actor, skill, .. } => vec![(actor, skill, None)],
            };
            for (actor, skill, cap) in entries {
                deltas.push(skills::PracticeDelta {
                    actor,
                    skill,
                    milliseconds: elapsed,
                    mastery_cap: cap,
                });
            }
        }
        deltas
    }
    pub(crate) fn interrupt_social(&mut self, actor: PersonId) {
        let Some(world) = &mut self.state.social else {
            return;
        };
        let Some(key) = world.participants.get(&actor).copied() else {
            return;
        };
        let Some(interaction) = world.interactions.remove(&key) else {
            return;
        };
        let actors = interaction.actors();
        for id in &actors {
            world.participants.remove(id);
        }
        for id in actors {
            if matches!(
                *self.state.components.activity(id).expect("activity exists"),
                Activity::Social { .. } | Activity::Instruction { .. } | Activity::Practice { .. }
            ) {
                *self
                    .state
                    .components
                    .activity_mut(id)
                    .expect("activity exists") = Activity::Idle;
            }
            for due in self.state.due.values_mut() {
                due.retain(|due| !matches!(due,Due::Physical(actor)if *actor==id));
            }
        }
        self.state.due.retain(|_, events| !events.is_empty());
    }
    pub(crate) fn reconcile_social(&mut self) {
        let interrupted: Vec<_> = self
            .state
            .social
            .as_ref()
            .into_iter()
            .flat_map(|world| world.interactions.iter())
            .filter(|(_, interaction)| !self.social_interaction_valid(interaction))
            .map(|(actor, _)| *actor)
            .collect();
        for actor in interrupted {
            self.interrupt_social(actor);
        }
    }
    pub(crate) fn complete_social(&mut self, actor: PersonId) {
        let Some(world) = &self.state.social else {
            return;
        };
        let Some(interaction) = world
            .participants
            .get(&actor)
            .and_then(|key| world.interactions.get(key))
            .cloned()
        else {
            return;
        };
        if interaction.completes() != self.state.time {
            return;
        }
        if !self.social_interaction_valid(&interaction) {
            self.interrupt_social(actor);
            return;
        }
        match interaction {
            Interaction::Talk {
                first,
                second,
                topic,
                ..
            } => {
                let effect = self
                    .state
                    .social
                    .as_mut()
                    .unwrap()
                    .finish_conversation(first, second, topic)
                    .expect("validated paired conversation");
                for (id, other) in [(effect.first, effect.second), (effect.second, effect.first)] {
                    let relationship = self
                        .state
                        .decisions
                        .as_mut()
                        .unwrap()
                        .minds
                        .get_mut(&id)
                        .unwrap()
                        .relationships
                        .entry(other)
                        .or_insert(decisions::Relationship {
                            affection: 0,
                            trust: 0,
                        });
                    relationship.affection =
                        (relationship.affection + effect.affection_gain).min(1_000);
                    relationship.trust = (relationship.trust + effect.trust_gain).min(1_000);
                }
                self.emit(EventKind::Socialized {
                    first,
                    second,
                    topic,
                });
            }
            Interaction::Instruction {
                teacher,
                student,
                topic,
                routes,
                ..
            } => {
                if topic == InstructionTopic::Routes {
                    let remembered = &mut self
                        .state
                        .decisions
                        .as_mut()
                        .unwrap()
                        .minds
                        .get_mut(&student)
                        .unwrap()
                        .routes;
                    for (area, neighbors) in routes {
                        remembered.entry(area).or_default().extend(neighbors);
                    }
                }
                self.state
                    .social
                    .as_mut()
                    .unwrap()
                    .learned
                    .entry(student)
                    .or_default()
                    .insert((teacher, topic));
                self.emit(EventKind::InstructionCompleted {
                    teacher,
                    student,
                    topic,
                });
            }
            Interaction::Practice { actor, skill, .. } => {
                self.emit(EventKind::PracticeCompleted { actor, skill })
            }
        }
        self.interrupt_social(actor);
    }
    pub(crate) fn social_boundary(&self, next: SimTime) -> SimTime {
        let Some(world) = &self.state.social else {
            return next;
        };
        SimTime(
            next.0.min(
                world
                    .last_review
                    .0
                    .saturating_add(world.rules.review.0)
                    .max(self.state.time.0 + 1),
            ),
        )
    }
    pub(crate) fn on_social_event(&mut self, kind: &EventKind) {
        if self.state.social.is_none() {
            return;
        }
        match *kind {
            EventKind::Born { child, .. } => {
                // Biological traits are authored by the separate genetic model.
                // Social learning neither copies them from a caregiver nor
                // changes an inherited genotype.
                let personality = self.state.decisions.as_ref().unwrap().minds[&child]
                    .personality
                    .clone();
                let mut record = baseline(&personality, decisions::Aspiration::Comfort);
                record.next_choice = self.state.time;
                self.state
                    .social
                    .as_mut()
                    .unwrap()
                    .development
                    .insert(child, record);
                let location = self.state.people[self.person_index[&child]]
                    .location
                    .clone();
                let witnesses: Vec<_> = self
                    .state
                    .people
                    .iter()
                    .filter(|person| {
                        person.id != child
                            && person.location == location
                            && self
                                .state
                                .components
                                .body(person.id)
                                .expect("body exists")
                                .vitals
                                .alive
                    })
                    .map(|person| person.id)
                    .collect();
                for witness in witnesses {
                    self.state
                        .social
                        .as_mut()
                        .unwrap()
                        .observe(witness, UpbringingEvidence::WitnessedBirth);
                }
            }
            EventKind::Cared {
                caregiver, child, ..
            } => {
                let world = self.state.social.as_mut().unwrap();
                world.observe(child, UpbringingEvidence::ReceivedCare);
                world.observe(caregiver, UpbringingEvidence::GaveCare);
            }
            EventKind::Comforted { caregiver, child } => {
                let world = self.state.social.as_mut().unwrap();
                world.observe(child, UpbringingEvidence::ReceivedComfort);
                world.observe(caregiver, UpbringingEvidence::GaveComfort);
            }
            EventKind::Practiced {
                actor, interest, ..
            } => self
                .state
                .social
                .as_mut()
                .unwrap()
                .observe(actor, UpbringingEvidence::Played(interest)),
            EventKind::Produced { actor, .. } | EventKind::PracticeCompleted { actor, .. } => self
                .state
                .social
                .as_mut()
                .unwrap()
                .observe(actor, UpbringingEvidence::Produced),
            EventKind::InstructionCompleted {
                teacher, student, ..
            } => {
                let world = self.state.social.as_mut().unwrap();
                world.observe(student, UpbringingEvidence::ReceivedInstruction);
                world.observe(teacher, UpbringingEvidence::GaveInstruction);
            }
            EventKind::Socialized {
                first,
                second,
                topic,
            } => {
                let world = self.state.social.as_mut().unwrap();
                for actor in [first, second] {
                    world.observe(actor, UpbringingEvidence::Conversation(topic));
                }
            }
            _ => {}
        }
    }

    pub(crate) fn review_social(&mut self) -> Result<(), Error> {
        let Some(world) = &self.state.social else {
            return Ok(());
        };
        if self.state.time.0 < world.last_review.0.saturating_add(world.rules.review.0) {
            return Ok(());
        }
        self.state
            .time
            .0
            .checked_add(world.rules.review.0)
            .ok_or(Error::Overflow)?;
        self.state.social.as_mut().unwrap().last_review = self.state.time;
        // Thinking about learned aims does not require a free physical action
        // slot. Teaching, caregiving or travel cannot indefinitely freeze a
        // developing person's aspirations behind an action cooldown.
        let agency: Vec<_> = self
            .state
            .social
            .as_ref()
            .unwrap()
            .development
            .keys()
            .map(|actor| {
                let control = if *actor == self.state.ruler {
                    decisions::Control::Manual
                } else if self.state.people[self.person_index[actor]].role == Role::Guardian {
                    decisions::Control::Delegated
                } else {
                    decisions::Control::Autonomous
                };
                (*actor, control, self.independent(*actor))
            })
            .collect();
        let social = self.state.social.as_ref().unwrap();
        for (actor, control, independent) in agency {
            self.state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
                .unwrap()
                .develop_aspiration(
                    &social.development[&actor],
                    control,
                    independent,
                    &self
                        .state
                        .components
                        .body(actor)
                        .expect("body exists")
                        .vitals,
                    social.rules.aspiration_margin,
                );
        }

        // These are deliberate local declarations of teaching availability.
        // Distant remembered offers remain dated; no query refreshes them.
        let speakers: Vec<_> = self
            .state
            .people
            .iter()
            .filter(|person| self.social_speech(person.id) && person.role != Role::Guardian)
            .map(|person| person.id)
            .collect();
        let mut local: BTreeMap<AreaId, Vec<PersonId>> = BTreeMap::new();
        let mut declarations: BTreeMap<AreaId, Vec<Offer>> = BTreeMap::new();
        for teacher in &speakers {
            let Location::AtArea(area) = self.state.people[self.person_index[teacher]].location
            else {
                continue;
            };
            local.entry(area).or_default().push(*teacher);
            let mind = &self.state.decisions.as_ref().unwrap().minds[teacher];
            if !mind.offers_teaching() {
                continue;
            }
            let mut topics = BTreeSet::new();
            if !mind.routes.is_empty() {
                topics.insert(InstructionTopic::Routes);
            }
            if let Some(records) = self
                .state
                .skills
                .as_ref()
                .and_then(|world| world.records.get(teacher))
            {
                for (skill, progress) in records {
                    if progress.practice_ms > 0
                        && !matches!(skill, Skill::Invocation | Skill::Combat)
                    {
                        topics.insert(InstructionTopic::Skill(*skill));
                    }
                }
            }
            if !topics.is_empty() {
                declarations.entry(area).or_default().push(Offer {
                    teacher: *teacher,
                    topics,
                    area,
                    heard_at: self.state.time,
                });
            }
        }
        for observers in local.values_mut() {
            observers.sort();
        }
        for offers in declarations.values_mut() {
            offers.sort_by_key(|offer| offer.teacher);
        }
        for observer in &speakers {
            let Location::AtArea(area) = self.state.people[self.person_index[observer]].location
            else {
                continue;
            };
            let Some(declared) = declarations.get(&area) else {
                continue;
            };
            // Attention and remembered offers are bounded. Stable person/time
            // rotation lets a crowded area's different speakers be heard over
            // subsequent reviews without all-pairs storage or off-screen facts.
            let offset = (observer.0.wrapping_add(
                self.state.time.0 / self.state.social.as_ref().unwrap().rules.review.0,
            ) % declared.len() as u64) as usize;
            for offer in declared
                .iter()
                .cycle()
                .skip(offset)
                .take(declared.len().min(64))
            {
                if offer.teacher != *observer {
                    self.state.social.as_mut().unwrap().remember_offer(
                        *observer,
                        offer.clone(),
                        self.state.time,
                    )?;
                }
            }
        }

        for actor in speakers {
            if actor == self.state.ruler || !self.social_speech(actor) {
                continue;
            }
            let world = self.state.social.as_ref().unwrap();
            let record = &world.development[&actor];
            if record.next_choice > self.state.time {
                continue;
            }
            let mind = &self.state.decisions.as_ref().unwrap().minds[&actor];
            let Location::AtArea(area) = self.state.people[self.person_index[&actor]].location
            else {
                continue;
            };
            let nearby = &local[&area];
            let offset = (actor.0 % nearby.len() as u64) as usize;
            let family_peers = nearby
                .iter()
                .cycle()
                .skip(offset)
                .take(nearby.len().min(64))
                .filter(|partner| {
                    **partner != self.state.ruler
                        && self.social_local_pair(actor, **partner)
                        && self.social_adult(**partner)
                })
                .copied()
                .collect();
            let available_teachers = world
                .offers
                .get(&actor)
                .into_iter()
                .flat_map(|offers| offers.keys())
                .filter(|teacher| {
                    **teacher != self.state.ruler && self.social_local_pair(actor, **teacher)
                })
                .copied()
                .collect();
            let own_levels = [
                Skill::Craft,
                Skill::Farming,
                Skill::Medicine,
                Skill::Negotiation,
                Skill::Leadership,
            ]
            .into_iter()
            .map(|skill| (skill, self.own_skill_level(actor, skill)))
            .collect();
            let choice = game_characters::social::ChoiceFacts {
                actor,
                now: self.state.time,
                autonomous: actor != self.state.ruler,
                can_speak: self.social_speech(actor),
                adult: self.social_adult(actor),
                family_peers,
                available_teachers,
                own_levels,
                can_practice_craft: self.social_tool(actor).is_some()
                    && self
                        .social_material(actor, Good::Timber, world.rules.practice_material)
                        .is_some(),
                companionship_partner: self
                    .state
                    .people
                    .iter()
                    .find(|person| {
                        person.id != self.state.ruler && self.social_local_pair(actor, person.id)
                    })
                    .map(|person| person.id),
            };
            let chosen = world.choose_personal(mind, &choice);
            if let Some(action) = chosen {
                match self.start_social(actor, action) {
                    Ok(()) | Err(Error::Unavailable) => {}
                    Err(error) => return Err(error),
                }
            }
            self.state
                .social
                .as_mut()
                .unwrap()
                .finish_personal_choice(actor, self.state.time)?;
        }
        Ok(())
    }
    pub(crate) fn available_social_commands(&self, actor: PersonId) -> Vec<Command> {
        if self.state.social.is_none() || !self.social_speech(actor) {
            return Vec::new();
        }
        let mut commands = Vec::new();
        for person in &self.state.people {
            if !self.social_local_pair(actor, person.id) {
                continue;
            }
            commands.push(Command::Social {
                action: SocialAction::Talk {
                    partner: person.id,
                    topic: SocialTopic::Companionship,
                },
            });
            if self.social_adult(actor) && self.social_adult(person.id) {
                commands.push(Command::Social {
                    action: SocialAction::Talk {
                        partner: person.id,
                        topic: SocialTopic::FamilyPlans,
                    },
                });
            }
        }
        for offer in self
            .state
            .social
            .as_ref()
            .unwrap()
            .offers
            .get(&actor)
            .into_iter()
            .flat_map(|offers| offers.values())
        {
            if self.social_local_pair(actor, offer.teacher) {
                for topic in &offer.topics {
                    commands.push(Command::Social {
                        action: SocialAction::Learn {
                            teacher: offer.teacher,
                            topic: *topic,
                        },
                    });
                }
            }
        }
        if !self.combat_restricts(actor)
            && self.social_tool(actor).is_some()
            && self
                .social_material(
                    actor,
                    Good::Timber,
                    self.state.social.as_ref().unwrap().rules.practice_material,
                )
                .is_some()
        {
            commands.push(Command::Social {
                action: SocialAction::Practice {
                    skill: Skill::Craft,
                },
            });
        }
        commands
    }
    pub(crate) fn social_view(&self, actor: PersonId) -> Option<SocialView> {
        self.state.social.as_ref()?.view(actor)
    }
}

pub(crate) fn validate(state: &CanonicalState) -> Result<(), Error> {
    let Some(world) = &state.social else {
        return Ok(());
    };
    game_characters::social::validate_local(world, state.time)?;
    let fail = || Error::InvalidSave("inconsistent social development".into());
    state.physical.as_ref().ok_or_else(fail)?;
    let decisions = state.decisions.as_ref().ok_or_else(fail)?;
    let people: BTreeMap<_, _> = state
        .people
        .iter()
        .map(|person| (person.id, person))
        .collect();
    if world.development.len() != people.len() {
        return Err(fail());
    }
    for actor in world.development.keys() {
        if !people.contains_key(actor) || !decisions.minds.contains_key(actor) {
            return Err(fail());
        }
    }
    for offers in world.offers.values() {
        if offers
            .values()
            .any(|offer| !state.areas.iter().any(|area| area.id == offer.area))
        {
            return Err(fail());
        }
    }
    let participants = &world.participants;
    for interaction in world.interactions.values() {
        let actors = interaction.actors();
        for actor in &actors {
            let person = people.get(actor).ok_or_else(fail)?;
            let body = state.components.body(*actor).ok_or_else(fail)?;
            if !body.vitals.alive
                || !matches!(person.location, Location::AtArea(_))
                || person.location != people[&actors[0]].location
            {
                return Err(fail());
            }
            let activity = match *interaction {
                Interaction::Talk {
                    first,
                    second,
                    topic,
                    completes,
                } => Activity::Social {
                    partner: if *actor == first { second } else { first },
                    topic,
                    completes,
                },
                Interaction::Instruction {
                    teacher,
                    student,
                    topic,
                    completes,
                    ..
                } => Activity::Instruction {
                    teacher,
                    student,
                    topic,
                    completes,
                },
                Interaction::Practice {
                    skill,
                    tool,
                    completes,
                    ..
                } => Activity::Practice {
                    skill,
                    tool: Some(tool),
                    completes,
                },
            };
            if state.components.activity(*actor).ok_or_else(fail)? != &activity
                || !state
                    .due
                    .get(&interaction.completes())
                    .is_some_and(|due| due.contains(&Due::Physical(*actor)))
            {
                return Err(fail());
            }
        }
        match interaction {
            Interaction::Instruction {
                topic,
                routes,
                mastery_cap,
                tool,
                ..
            } => {
                if *topic == InstructionTopic::Skill(Skill::Invocation)
                    || *mastery_cap
                        > state
                            .skills
                            .as_ref()
                            .map_or(0, |w| w.rules.per_level.0 * u64::from(w.rules.max_level))
                {
                    return Err(fail());
                }
                if *topic != InstructionTopic::Routes && !routes.is_empty() {
                    return Err(fail());
                }
                for (area, neighbors) in routes {
                    if !state.areas.iter().any(|record| record.id == *area)
                        || neighbors.iter().any(|(neighbor, duration)| {
                            !state.areas.iter().any(|record| record.id == *neighbor)
                                || duration.0 == 0
                        })
                    {
                        return Err(fail());
                    }
                }
                if tool.is_some_and(|tool| {
                    state
                        .inventories
                        .get(&tool)
                        .is_none_or(|stock| stock.good != Good::Tool)
                }) {
                    return Err(fail());
                }
            }
            Interaction::Practice { tool, .. }
                if state
                    .inventories
                    .get(tool)
                    .is_none_or(|stock| stock.good != Good::Tool) =>
            {
                return Err(fail());
            }
            _ => {}
        }
    }
    for (actor, activity) in state.components.activities() {
        if matches!(
            activity,
            Activity::Social { .. } | Activity::Instruction { .. } | Activity::Practice { .. }
        ) && !participants.contains_key(&actor)
        {
            return Err(fail());
        }
    }
    Ok(())
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
        let village = sim.state.areas[0].id;
        for person in &mut sim.state.people {
            person.location = Location::AtArea(village);
            person.route.clear();
        }
        sim.state
            .due
            .values_mut()
            .for_each(|due| due.retain(|entry| !matches!(entry, Due::Arrive(_) | Due::Age(_))));
        let rules = &mut sim.state.development.as_mut().unwrap().rules;
        rules.lifespan = SimDuration(5_000_000);
        let ids: Vec<_> = sim.state.people.iter().map(|p| p.id).collect();
        for actor in &ids {
            sim.schedule_age(*actor);
        }
        // Ordinary chosen needs remain real, with sufficient authored supplies.
        for actor in ids {
            sim.interrupt_actor_activity(actor);
            let mind = sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
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
        let food = sim.state.decisions.as_ref().unwrap().minds[&first]
            .knowledge
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
            let mind = sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
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
        let active = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
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
        let birth_at = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
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
        sim.advance(SimDuration(birth_at - sim.state.time.0))
            .unwrap();
        // This authored pair now prefers supporting their actual child. The
        // fixture does not assume that their old intent obliges more births.
        for actor in [first, second] {
            sim.state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
                .unwrap()
                .dream = decisions::Aspiration::SupportOthers;
        }
        sim.state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
            .filter_map(|p| match p.state {
                development::PregnancyState::Born(child)
                    if pair(p.gestator, p.partner) == pair(first, second) =>
                {
                    Some(child)
                }
                _ => None,
            })
            .next_back()
            .unwrap()
    }
    #[test]
    fn ordinary_guidance_is_coupled_timed_and_transmits_only_frozen_remembered_routes() {
        let mut sim = fixture(1);
        let teacher = sim.state.people[3].id;
        let student = sim.state.people[4].id;
        let village = sim.state.areas[0].id;
        let farm = sim.state.areas[1].id;
        let forest = sim.state.areas[2].id;
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&student)
            .unwrap()
            .routes
            .clear();
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&teacher)
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
            *sim.state.components.activity(teacher).expect("body exists"),
            Activity::Instruction { .. }
        ));
        let frozen = sim.state.decisions.as_ref().unwrap().minds[&teacher]
            .routes
            .clone();
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&teacher)
            .unwrap()
            .routes
            .insert(forest, BTreeMap::from([(farm, MINUTE)]));
        sim.advance(SimDuration(59_999)).unwrap();
        assert!(
            !sim.state.decisions.as_ref().unwrap().minds[&student]
                .routes
                .contains_key(&farm)
        );
        sim.advance(SimDuration(1)).unwrap();
        let known = &sim.state.decisions.as_ref().unwrap().minds[&student].routes;
        assert_eq!(known.get(&farm), frozen.get(&farm));
        assert!(!known.contains_key(&forest));
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::InstructionCompleted{teacher:t,student:s,..}if t==teacher&&s==student)));
    }
    #[test]
    fn practical_trials_use_real_materials_and_partial_time_survives_cancellation() {
        let mut sim = fixture(1);
        let actor = sim.state.people[3].id;
        // Local observation is acquired through the ordinary simulation review.
        sim.advance(SimDuration(1)).unwrap();
        cancel(&mut sim, actor);
        let timber = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Timber
                    && stock.location == InventoryLocation::Area(sim.state.areas[0].id)
            })
            .unwrap()
            .id;
        let before = sim.state.inventories[&timber].quantity.0;
        let prior = sim
            .state
            .skills
            .as_ref()
            .unwrap()
            .records
            .get(&actor)
            .and_then(|records| records.get(&Skill::Craft))
            .map_or(0, |progress| progress.practice_ms);
        social(
            &mut sim,
            actor,
            SocialAction::Practice {
                skill: Skill::Craft,
            },
        );
        assert_eq!(sim.state.inventories[&timber].quantity.0, before - 100);
        sim.advance(SimDuration(30_000)).unwrap();
        cancel(&mut sim, actor);
        assert_eq!(
            sim.state.skills.as_ref().unwrap().records[&actor][&Skill::Craft].practice_ms,
            prior + 30_000
        );
        sim.debug_set_personality(
            actor,
            decisions::Personality::restful(),
            decisions::Aspiration::Comfort,
        )
        .unwrap();
        let own = sim.state.skills.as_ref().unwrap().records[&actor][&Skill::Craft].practice_ms;
        sim.advance(SimDuration(30_000)).unwrap();
        assert_eq!(
            sim.state.skills.as_ref().unwrap().records[&actor][&Skill::Craft].practice_ms,
            own
        );
    }
    #[test]
    fn real_tool_removal_interrupts_a_trial_without_later_ghost_practice() {
        let mut sim = fixture(1);
        let actor = sim.state.people[3].id;
        let carrier = sim.state.people[4].id;
        sim.state.social.as_mut().unwrap().rules.practice = SimDuration(180_000);
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
        assert!(sim.state.social.as_ref().unwrap().interactions.is_empty());
        let learned = sim.state.skills.as_ref().unwrap().records[&actor][&Skill::Craft].practice_ms;
        sim.debug_set_personality(
            actor,
            decisions::Personality::restful(),
            decisions::Aspiration::Comfort,
        )
        .unwrap();
        sim.advance(MINUTE).unwrap();
        assert_eq!(
            sim.state.skills.as_ref().unwrap().records[&actor][&Skill::Craft].practice_ms,
            learned
        );
        assert!(!sim.state.events.iter().any(|event| matches!(event.kind, EventKind::PracticeCompleted { actor: worker, .. } if worker == actor)));
    }

    #[test]
    fn unsupported_or_impossible_requests_are_inert_and_refusals_are_actual_answers() {
        let mut sim = fixture(1);
        let actor = sim.state.people[3].id;
        let recipient = sim.state.people[4].id;
        let before = sim.state.clone();
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
        assert_eq!(sim.state, before);
        assert_eq!(
            sim.start_social(
                actor,
                SocialAction::Practice {
                    skill: Skill::Combat
                }
            ),
            Err(Error::Unavailable)
        );
        assert_eq!(sim.state, before);
        sim.state
            .decisions
            .as_mut()
            .unwrap()
            .minds
            .get_mut(&recipient)
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
        assert!(sim.state.events.iter().any(|event|matches!(event.kind,EventKind::SocialRefused{actor:a,recipient:r,..}if a==actor&&r==recipient)));
        assert!(matches!(
            *sim.state
                .components
                .activity(recipient)
                .expect("body exists"),
            Activity::Idle | Activity::Rest
        ));
    }
    #[test]
    fn ordinary_children_remember_chosen_care_and_reach_their_own_adult_family_decisions() {
        let mut sim = fixture(1);
        let first = sim.state.people[3].id;
        let second = sim.state.people[4].id;
        let child = actual_birth(&mut sim, first, second);
        assert!(
            sim.state.decisions.as_ref().unwrap().minds[&child]
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
        assert!(sim.state.social.as_ref().unwrap().development[&child].experiences > 0);
        assert_eq!(
            sim.state.development.as_ref().unwrap().lives[&child].stage,
            LifeStage::Adult
        );
        assert_ne!(
            sim.state.decisions.as_ref().unwrap().minds[&child].dream,
            decisions::Aspiration::Comfort
        );
        assert!(
            sim.state.development.as_ref().unwrap().lives[&child]
                .caregivers
                .iter()
                .any(|id| sim.state.people[sim.person_index[id]].role == Role::Citizen)
        );
        validate(&sim.state).unwrap();
        let restored = Simulation::load(
            &sim.save().unwrap(),
            Execution {
                workers: 4,
                reverse_jobs: false,
            },
        )
        .unwrap();
        assert_eq!(sim.state, restored.state);
    }
    #[test]
    fn ordinary_interactions_are_save_chunk_worker_and_query_independent() {
        fn run(workers: usize, chunked: bool) -> Simulation {
            let mut sim = fixture(workers);
            let first = sim.state.people[3].id;
            let second = sim.state.people[4].id;
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
        assert_eq!(run(1, false).state, run(4, true).state);
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
            (sim.state.people[3].id, sim.state.people[4].id),
            (sim.state.people[5].id, sim.state.people[6].id),
        ];
        let mut children = Vec::new();
        // These additional adults begin this fixture with their own intention;
        // later children develop aims from the actual care and activities below.
        for actor in [parents[1].0, parents[1].1] {
            let mind = sim
                .state
                .decisions
                .as_mut()
                .unwrap()
                .minds
                .get_mut(&actor)
                .unwrap();
            mind.dream = decisions::Aspiration::Parenthood;
            mind.personality.empathy = 650;
            mind.personality.sociability = 1_000;
            mind.personality.discipline = 0;
            mind.personality.ambition = 0;
            mind.personality.hedonism = 400;
            let mut record = baseline(&mind.personality, mind.dream);
            record.next_choice = sim.state.time;
            sim.state
                .social
                .as_mut()
                .unwrap()
                .development
                .insert(actor, record);
        }
        for (gestator, partner) in parents {
            let already_chosen = sim
                .state
                .development
                .as_ref()
                .unwrap()
                .pregnancies
                .values()
                .any(|pregnancy| {
                    pregnancy.gestator == gestator
                        && matches!(
                            pregnancy.state,
                            development::PregnancyState::Gestating { .. }
                        )
                });
            for (actor, other) in [(gestator, partner), (partner, gestator)] {
                sim.debug_actor_command(actor, Command::Cancel).unwrap();
                let mind = sim
                    .state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(&actor)
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
                let active = sim
                    .state
                    .development
                    .as_ref()
                    .unwrap()
                    .pregnancies
                    .values()
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
            let birth_at = sim
                .state
                .development
                .as_ref()
                .unwrap()
                .pregnancies
                .values()
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
            sim.advance(SimDuration(birth_at - sim.state.time.0))
                .unwrap();
            let child = sim
                .state
                .development
                .as_ref()
                .unwrap()
                .pregnancies
                .values()
                .find_map(|pregnancy| match pregnancy.state {
                    development::PregnancyState::Born(child) if pregnancy.gestator == gestator => {
                        Some(child)
                    }
                    _ => None,
                })
                .unwrap();
            children.push(child);
            for actor in [gestator, partner] {
                sim.state
                    .decisions
                    .as_mut()
                    .unwrap()
                    .minds
                    .get_mut(&actor)
                    .unwrap()
                    .dream = decisions::Aspiration::SupportOthers;
            }
        }
        let food = sim
            .state
            .inventories
            .values()
            .find(|stock| {
                stock.good == Good::Food
                    && stock.location == InventoryLocation::Area(sim.state.areas[0].id)
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
            .map(|child| sim.state.development.as_ref().unwrap().lives[child].born.0 + 600_000)
            .max()
            .unwrap();
        if sim.state.time.0 < adult_at {
            sim.advance(SimDuration(adult_at - sim.state.time.0))
                .unwrap();
        }
        for child in &children {
            assert_eq!(
                sim.state.development.as_ref().unwrap().lives[child].stage,
                LifeStage::Adult
            );
            assert_eq!(
                sim.state.decisions.as_ref().unwrap().minds[child].dream,
                decisions::Aspiration::Parenthood,
                "child {child:?} at {:?}: {:?}, body {:?}",
                sim.state.time,
                sim.state.social.as_ref().unwrap().development[child],
                *sim.state.components.activity(*child).expect("body exists"),
            );
            // Institutional rights may emerge independently; personal access
            // permissions are not copied from either parent.
            assert!(!sim.state.inventories[&food].access.contains(child));
        }
        assert_ne!(
            sim.state.development.as_ref().unwrap().lives[&children[0]].reproduction,
            sim.state.development.as_ref().unwrap().lives[&children[1]].reproduction,
            "this seeded pair must have compatible biological roles; pregnancies {:?}",
            sim.state
                .development
                .as_ref()
                .unwrap()
                .pregnancies
                .values()
                .map(|pregnancy| (pregnancy.id, &pregnancy.state))
                .collect::<Vec<_>>()
        );
        // Each new adult obtains chosen access through an actual local promise,
        // rather than inheriting the parents' title or old people's permissions.
        let grantor = sim.state.agreements.as_ref().unwrap().custodians[&food];
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
            let agreement = *sim
                .state
                .agreements
                .as_ref()
                .unwrap()
                .agreements
                .keys()
                .next_back()
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
        let grandchild = sim
            .state
            .development
            .as_ref()
            .unwrap()
            .pregnancies
            .values()
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
            sim.state.development.as_ref().unwrap().lives[&grandchild]
                .parents
                .len(),
            2
        );
        assert!(
            sim.own_skill_view(grandchild)
                .iter()
                .all(|progress| progress.practice_ms == 0)
        );
        assert!(!sim.state.inventories[&food].access.contains(&grandchild));
        assert!(
            sim.state
                .institutions
                .as_ref()
                .unwrap()
                .institutions
                .values()
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
        assert_eq!(sim.state, restored.state);
    }
}
