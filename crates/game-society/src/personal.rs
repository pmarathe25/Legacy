//! Ordinary conversations and instruction prepared from locally available facts.
use game_characters::social::{
    self, ConversationInput, InstructionTopic, Interaction, Rules, SocialAction, SocialTopic, World,
};
use game_core::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct PersonFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub speech: bool,
    pub adult: bool,
    pub practical: bool,
    pub tool: Option<InventoryId>,
    pub materials: BTreeMap<Good, InventoryId>,
    pub farming_access: bool,
    pub routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    pub practice: BTreeMap<Skill, u64>,
    pub empathy: u16,
    pub patience: u16,
    pub affection: i32,
    pub trust: i32,
    pub conversation: Option<ConversationInput>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StartDecision {
    Refused {
        recipient: PersonId,
        reason: &'static str,
    },
    Started {
        interaction: Interaction,
        materials: Option<(PersonId, InventoryId, Quantity)>,
        next_choice: SimTime,
    },
}
fn completes(now: SimTime, duration: SimDuration) -> Result<SimTime, Error> {
    now.0
        .checked_add(duration.0)
        .map(SimTime)
        .ok_or(Error::Overflow)
}
pub fn prepare_start(
    world: &World,
    store: &game_ecs::Store,
    actor: &PersonFacts,
    partner: Option<&PersonFacts>,
    action: SocialAction,
    now: SimTime,
) -> Result<StartDecision, Error> {
    prepare_start_personal(
        world,
        social::personal(store, actor.actor),
        actor,
        partner,
        action,
        now,
    )
}

/// Native transactions supply the current personal row directly.
pub fn prepare_start_personal(
    world: &World,
    own: Option<&social::Personal>,
    actor: &PersonFacts,
    partner: Option<&PersonFacts>,
    action: SocialAction,
    now: SimTime,
) -> Result<StartDecision, Error> {
    if !world.rules.valid() {
        return Err(Error::InvalidConfiguration(
            "invalid social development rules".into(),
        ));
    }
    if !actor.speech || own.is_none() {
        return Err(Error::Unavailable);
    }
    let rules = &world.rules;
    let mut materials = None;
    let interaction = match action {
        SocialAction::Talk { partner: id, topic } => {
            let partner = partner
                .filter(|row| row.actor == id)
                .ok_or(Error::Unavailable)?;
            if !local_pair(actor, partner)
                || topic == SocialTopic::FamilyPlans && (!actor.adult || !partner.adult)
            {
                return Err(Error::Unavailable);
            }
            let mut conversation = partner.conversation.ok_or(Error::Unavailable)?;
            conversation.chosen_companionship =
                own.is_some_and(|row| row.companionship.contains(&id));
            if !social::conversation_willing(topic, conversation) {
                return Ok(StartDecision::Refused {
                    recipient: id,
                    reason: "They prefer not to discuss this now",
                });
            }
            Interaction::Talk {
                first: actor.actor,
                second: id,
                topic,
                completes: completes(now, rules.conversation)?,
            }
        }
        SocialAction::Learn { teacher, topic } => {
            let teacher = partner
                .filter(|row| row.actor == teacher)
                .ok_or(Error::Unavailable)?;
            if !local_pair(actor, teacher) || topic == InstructionTopic::Skill(Skill::Invocation) {
                return Err(Error::Unavailable);
            }
            if i32::from(teacher.empathy + teacher.patience) / 2 + teacher.affection + teacher.trust
                < 400
            {
                return Ok(StartDecision::Refused {
                    recipient: teacher.actor,
                    reason: "They prefer to spend their time elsewhere",
                });
            }
            let mut tool = None;
            let mastery_cap = match topic {
                InstructionTopic::Routes => {
                    if teacher.routes.is_empty() {
                        return Ok(StartDecision::Refused {
                            recipient: teacher.actor,
                            reason: "They do not know routes to teach",
                        });
                    }
                    0
                }
                InstructionTopic::Skill(skill) => {
                    let practice = teacher.practice.get(&skill).copied().unwrap_or(0);
                    if practice == 0 {
                        return Ok(StartDecision::Refused {
                            recipient: teacher.actor,
                            reason: "They have no experience of that skill",
                        });
                    }
                    if matches!(
                        skill,
                        Skill::Craft | Skill::Farming | Skill::Medicine | Skill::Combat
                    ) {
                        if !actor.practical || !teacher.practical || skill == Skill::Combat {
                            return Err(Error::Unavailable);
                        }
                        if matches!(skill, Skill::Craft | Skill::Farming) {
                            tool = Some(teacher.tool.ok_or(Error::Unavailable)?);
                        }
                        if skill == Skill::Farming && !teacher.farming_access {
                            return Err(Error::Unavailable);
                        }
                        let good = match skill {
                            Skill::Craft => Good::Timber,
                            Skill::Farming => Good::Seed,
                            _ => Good::Herb,
                        };
                        materials = Some((
                            teacher.actor,
                            *teacher.materials.get(&good).ok_or(Error::Unavailable)?,
                            rules.practice_material,
                        ));
                    }
                    practice
                }
            };
            Interaction::Instruction {
                teacher: teacher.actor,
                student: actor.actor,
                topic,
                routes: if topic == InstructionTopic::Routes {
                    teacher.routes.clone()
                } else {
                    BTreeMap::new()
                },
                mastery_cap,
                tool,
                completes: completes(now, rules.instruction)?,
            }
        }
        SocialAction::Practice { skill } => {
            if skill != Skill::Craft || !actor.practical {
                return Err(Error::Unavailable);
            }
            let tool = actor.tool.ok_or(Error::Unavailable)?;
            materials = Some((
                actor.actor,
                *actor
                    .materials
                    .get(&Good::Timber)
                    .ok_or(Error::Unavailable)?,
                rules.practice_material,
            ));
            Interaction::Practice {
                actor: actor.actor,
                skill,
                tool,
                completes: completes(now, rules.practice)?,
            }
        }
    };
    let next_choice = completes(interaction.completes(), rules.review)?;
    Ok(StartDecision::Started {
        interaction,
        materials,
        next_choice,
    })
}
pub fn local_pair(first: &PersonFacts, second: &PersonFacts) -> bool {
    first.actor != second.actor
        && first.speech
        && second.speech
        && first.area.is_some()
        && first.area == second.area
}

#[derive(Clone, Debug)]
pub struct SpeakerFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub speech: bool,
    pub guardian: bool,
    pub autonomous: bool,
    pub adult: bool,
    pub offers_teaching: bool,
    pub knows_routes: bool,
    pub practice: BTreeMap<Skill, u64>,
}
/// Local declaration indexing and attention rotation belong to Society.
pub fn publish_teaching_offers(
    world: &World,
    store: &mut game_ecs::Store,
    roster: &[SpeakerFacts],
    now: SimTime,
) -> Result<(), Error> {
    let mut declarations: BTreeMap<AreaId, Vec<social::Offer>> = BTreeMap::new();
    for row in roster
        .iter()
        .filter(|row| row.speech && !row.guardian && row.offers_teaching)
    {
        let Some(area) = row.area else { continue };
        let mut topics = BTreeSet::new();
        if row.knows_routes {
            topics.insert(InstructionTopic::Routes);
        }
        for (skill, practice) in &row.practice {
            if *practice > 0 && !matches!(skill, Skill::Invocation | Skill::Combat) {
                topics.insert(InstructionTopic::Skill(*skill));
            }
        }
        if !topics.is_empty() {
            declarations.entry(area).or_default().push(social::Offer {
                teacher: row.actor,
                topics,
                area,
                heard_at: now,
            });
        }
    }
    for offers in declarations.values_mut() {
        offers.sort_by_key(|offer| offer.teacher);
    }
    for observer in roster.iter().filter(|row| row.speech && !row.guardian) {
        let Some(declared) = observer.area.and_then(|area| declarations.get(&area)) else {
            continue;
        };
        let offset = (observer.actor.0.wrapping_add(now.0 / world.rules.review.0)
            % declared.len() as u64) as usize;
        for offer in declared
            .iter()
            .cycle()
            .skip(offset)
            .take(declared.len().min(64))
        {
            if offer.teacher != observer.actor {
                world.remember_offer(store, observer.actor, offer.clone(), now)?;
            }
        }
    }
    Ok(())
}

pub fn available_actions(
    _world: &World,
    store: &game_ecs::Store,
    actor: &PersonFacts,
    roster: &[PersonFacts],
    combat_restricted: bool,
) -> Vec<SocialAction> {
    available_actions_personal(
        social::personal(store, actor.actor),
        actor,
        roster,
        combat_restricted,
    )
}

pub fn available_actions_personal(
    own: Option<&social::Personal>,
    actor: &PersonFacts,
    roster: &[PersonFacts],
    combat_restricted: bool,
) -> Vec<SocialAction> {
    if !actor.speech {
        return Vec::new();
    }
    let mut actions = Vec::new();
    for other in roster.iter().filter(|other| local_pair(actor, other)) {
        actions.push(SocialAction::Talk {
            partner: other.actor,
            topic: SocialTopic::Companionship,
        });
        if actor.adult && other.adult {
            actions.push(SocialAction::Talk {
                partner: other.actor,
                topic: SocialTopic::FamilyPlans,
            });
        }
    }
    for offer in own.into_iter().flat_map(|row| row.offers.values()) {
        if roster
            .iter()
            .any(|other| other.actor == offer.teacher && local_pair(actor, other))
        {
            actions.extend(offer.topics.iter().map(|topic| SocialAction::Learn {
                teacher: offer.teacher,
                topic: *topic,
            }));
        }
    }
    if !combat_restricted && actor.tool.is_some() && actor.materials.contains_key(&Good::Timber) {
        actions.push(SocialAction::Practice {
            skill: Skill::Craft,
        });
    }
    actions
}

/// Reject nonphysical references before startup changes are published.
pub fn review_due(rules: &Rules, last: SimTime, now: SimTime) -> Result<bool, Error> {
    if now.0 < last.0.saturating_add(rules.review.0) {
        return Ok(false);
    }
    completes(now, rules.review)?;
    Ok(true)
}

pub struct AspirationFacts {
    pub actor: PersonId,
    pub is_current_ruler: bool,
    pub role: Role,
    pub independent: bool,
    pub vitals: Vitals,
}
pub fn develop_aspirations<A: game_ecs::Component>(
    store: &mut game_ecs::Store,
    facts: &[AspirationFacts],
    margin: u32,
) {
    for fact in facts {
        let Some(record) = social::personal(store, fact.actor).map(|row| row.development.clone())
        else {
            continue;
        };
        if let Some(mind) = game_characters::decisions::mind_mut::<A>(store, fact.actor) {
            mind.develop_aspiration(
                &record,
                social::aspiration_control(fact.is_current_ruler, fact.role),
                fact.independent,
                &fact.vitals,
                margin,
            );
        }
    }
}
pub fn review_speakers(
    roster: &[SpeakerFacts],
    store: &game_ecs::Store,
    now: SimTime,
) -> Vec<PersonId> {
    roster
        .iter()
        .filter(|row| {
            row.speech
                && !row.guardian
                && row.autonomous
                && social::personal(store, row.actor)
                    .is_some_and(|own| own.development.next_choice <= now)
        })
        .map(|row| row.actor)
        .collect()
}

pub struct BirthWitness {
    pub actor: PersonId,
    pub same_location: bool,
    pub alive: bool,
}
pub enum PersonalReceipt {
    Born {
        child: PersonId,
        temperament: social::Temperament,
        witnesses: Vec<BirthWitness>,
    },
    Cared {
        caregiver: PersonId,
        child: PersonId,
    },
    Comforted {
        caregiver: PersonId,
        child: PersonId,
    },
    Played {
        actor: PersonId,
        interest: ChildhoodInterest,
    },
    Produced {
        actor: PersonId,
    },
    Instructed {
        teacher: PersonId,
        student: PersonId,
    },
    Socialized {
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
    },
}
pub use crate::personal_receipts::{
    PersonalReceiptPublication, PreparedPersonalReceipts, prepare_receipt, prepare_receipts,
};
pub fn observe_event(
    world: &World,
    store: &mut game_ecs::Store,
    receipt: PersonalReceipt,
    now: SimTime,
) -> Result<(), Error> {
    prepare_receipt(world, store, receipt, now)?.commit(world, store, now)
}

/// Chosen native material identities are facts; the owner decides their joint readiness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CraftAvailability {
    pub tool: Option<InventoryId>,
    pub timber: Option<InventoryId>,
}
impl CraftAvailability {
    pub fn can_practice(self) -> bool {
        self.tool.is_some() && self.timber.is_some()
    }
}

pub struct PersonalChoice<'a, A> {
    pub actor: PersonId,
    pub mind: &'a game_characters::decisions::Mind<A>,
    pub now: SimTime,
    pub levels: BTreeMap<Skill, u16>,
    pub craft: CraftAvailability,
}

/// The roster is indexed once per review. Activity changes refresh only their participants.
pub struct SpeakerIndex {
    rows: BTreeMap<PersonId, SpeakerFacts>,
    areas: BTreeMap<AreaId, Vec<PersonId>>,
    order: BTreeMap<AreaId, Vec<PersonId>>,
}
impl SpeakerIndex {
    pub fn new(roster: Vec<SpeakerFacts>) -> Self {
        let mut order = BTreeMap::<AreaId, Vec<PersonId>>::new();
        for row in &roster {
            if let Some(area) = row.area {
                order.entry(area).or_default().push(row.actor);
            }
        }
        let mut areas = BTreeMap::<AreaId, Vec<PersonId>>::new();
        for row in &roster {
            if row.speech
                && !row.guardian
                && let Some(area) = row.area
            {
                areas.entry(area).or_default().push(row.actor);
            }
        }
        for people in areas.values_mut() {
            people.sort();
        }
        Self {
            rows: roster.into_iter().map(|row| (row.actor, row)).collect(),
            areas,
            order,
        }
    }
    pub fn set_speech(&mut self, actor: PersonId, speech: bool) {
        if let Some(row) = self.rows.get_mut(&actor) {
            row.speech = speech;
        }
    }
    pub fn can_choose(&self, store: &game_ecs::Store, actor: PersonId, now: SimTime) -> bool {
        self.rows
            .get(&actor)
            .is_some_and(|row| row.speech && row.autonomous && !row.guardian)
            && social::personal(store, actor).is_some_and(|row| row.development.next_choice <= now)
    }
    pub fn choose<A>(
        &self,
        world: &World,
        store: &game_ecs::Store,
        input: PersonalChoice<'_, A>,
    ) -> Option<SocialAction> {
        let PersonalChoice {
            actor,
            mind,
            now,
            levels,
            craft,
        } = input;
        let row = self.rows.get(&actor)?;
        let nearby = row
            .area
            .and_then(|area| self.areas.get(&area))
            .map_or(&[][..], Vec::as_slice);
        let offset = if nearby.is_empty() {
            0
        } else {
            (actor.0 % nearby.len() as u64) as usize
        };
        let own = social::personal(store, actor)?;
        let facts = social::ChoiceFacts {
            actor,
            now,
            autonomous: row.autonomous,
            can_speak: row.speech,
            adult: row.adult,
            family_peers: nearby
                .iter()
                .cycle()
                .skip(offset)
                .take(nearby.len().min(64))
                .filter(|id| {
                    **id != actor
                        && self
                            .rows
                            .get(id)
                            .is_some_and(|peer| peer.autonomous && peer.speech && peer.adult)
                })
                .copied()
                .collect(),
            available_teachers: own
                .offers
                .keys()
                .filter(|id| {
                    **id != actor
                        && self.rows.get(id).is_some_and(|teacher| {
                            teacher.autonomous && teacher.speech && teacher.area == row.area
                        })
                })
                .copied()
                .collect(),
            own_levels: levels,
            can_practice_craft: craft.can_practice(),
            companionship_partner: self
                .order
                .get(&row.area?)
                .into_iter()
                .flat_map(|people| people.iter())
                .find(|id| {
                    **id != actor
                        && self.rows.get(id).is_some_and(|peer| {
                            peer.autonomous && peer.speech && peer.area == row.area
                        })
                })
                .copied(),
        };
        world.choose_personal(store, mind, &facts)
    }
}

pub struct StockFacts {
    pub inventory: InventoryId,
    pub known_good: Good,
    pub known_quantity: Quantity,
    pub known_accessible: bool,
    pub actual_good: Good,
    pub actual_quantity: Quantity,
    pub area: Option<AreaId>,
    pub accessible: bool,
    pub tool_available: bool,
}
pub fn select_tool(facts: impl IntoIterator<Item = StockFacts>) -> Option<InventoryId> {
    facts
        .into_iter()
        .find(|row| {
            row.known_good == Good::Tool
                && row.known_accessible
                && row.known_quantity.0 >= 1_000
                && row.accessible
                && row.tool_available
        })
        .map(|row| row.inventory)
}
pub fn select_material(
    facts: impl IntoIterator<Item = StockFacts>,
    area: Option<AreaId>,
    good: Good,
    quantity: Quantity,
) -> Option<InventoryId> {
    area?;
    facts
        .into_iter()
        .find(|row| {
            row.known_good == good
                && row.known_accessible
                && row.known_quantity >= quantity
                && row.actual_good == good
                && row.actual_quantity >= quantity
                && row.area == area
                && row.accessible
        })
        .map(|row| row.inventory)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoundActivity {
    Other,
    Talk {
        partner: PersonId,
        topic: SocialTopic,
        completes: SimTime,
    },
    Instruction {
        teacher: PersonId,
        student: PersonId,
        topic: InstructionTopic,
        completes: SimTime,
    },
    Practice {
        skill: Skill,
        tool: Option<InventoryId>,
        completes: SimTime,
    },
}
pub struct ActorReference {
    pub has_mind: bool,
    pub alive: bool,
    pub area: Option<AreaId>,
    pub activity: Option<BoundActivity>,
    pub physical_deadlines: BTreeSet<SimTime>,
}
pub struct ValidationFacts {
    pub human_domains: bool,
    pub people: BTreeSet<PersonId>,
    pub areas: BTreeSet<AreaId>,
    pub actors: BTreeMap<PersonId, ActorReference>,
    pub stocks: BTreeMap<InventoryId, Good>,
    pub practice_per_level: u64,
    pub maximum_level: u16,
}
pub fn validate_context(
    world: &World,
    store: &game_ecs::Store,
    now: SimTime,
    facts: &ValidationFacts,
) -> Result<(), Error> {
    social::validate_local(world, store, now)?;
    let fail = || Error::InvalidSave("inconsistent social development".into());
    if !facts.human_domains || social::personals(store).count() != facts.people.len() {
        return Err(fail());
    }
    for (id, row) in social::personals(store) {
        if !facts.people.contains(&id)
            || facts.actors.get(&id).is_none_or(|actor| !actor.has_mind)
            || row
                .offers
                .values()
                .any(|offer| !facts.areas.contains(&offer.area))
        {
            return Err(fail());
        }
    }
    for (_, interaction) in social::interactions(store) {
        let mut area = None;
        for id in interaction.actors() {
            let actor = facts.actors.get(&id).ok_or_else(fail)?;
            if !facts.people.contains(&id)
                || !actor.alive
                || actor.area.is_none()
                || area.is_some_and(|area| Some(area) != actor.area)
            {
                return Err(fail());
            }
            area = actor.area;
            let expected = match *interaction {
                Interaction::Talk {
                    first,
                    second,
                    topic,
                    completes,
                } => BoundActivity::Talk {
                    partner: if id == first { second } else { first },
                    topic,
                    completes,
                },
                Interaction::Instruction {
                    teacher,
                    student,
                    topic,
                    completes,
                    ..
                } => BoundActivity::Instruction {
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
                } => BoundActivity::Practice {
                    skill,
                    tool: Some(tool),
                    completes,
                },
            };
            if actor.activity.as_ref() != Some(&expected)
                || !actor.physical_deadlines.contains(&interaction.completes())
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
                let cap = facts
                    .practice_per_level
                    .checked_mul(u64::from(facts.maximum_level))
                    .ok_or_else(fail)?;
                if *topic == InstructionTopic::Skill(Skill::Invocation)
                    || *mastery_cap > cap
                    || *topic != InstructionTopic::Routes && !routes.is_empty()
                {
                    return Err(fail());
                }
                if routes.iter().any(|(area, neighbors)| {
                    !facts.areas.contains(area)
                        || neighbors.iter().any(|(neighbor, duration)| {
                            !facts.areas.contains(neighbor) || duration.0 == 0
                        })
                }) {
                    return Err(fail());
                }
                if tool.is_some_and(|id| facts.stocks.get(&id) != Some(&Good::Tool)) {
                    return Err(fail());
                }
            }
            Interaction::Practice { tool, .. } if facts.stocks.get(tool) != Some(&Good::Tool) => {
                return Err(fail());
            }
            _ => {}
        }
    }
    for (id, actor) in &facts.actors {
        if matches!(
            actor.activity,
            Some(
                BoundActivity::Talk { .. }
                    | BoundActivity::Instruction { .. }
                    | BoundActivity::Practice { .. }
            )
        ) && social::participant(store, *id).is_none()
        {
            return Err(fail());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Completion {
    Waiting,
    Interrupted,
    Socialized {
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
    },
    Instructed {
        teacher: PersonId,
        student: PersonId,
        topic: InstructionTopic,
    },
    Practiced {
        actor: PersonId,
        skill: Skill,
    },
}
pub use crate::personal_completion::{
    PersonalCompletionPublication, PreparedPersonalCompletion, prepare_completion,
};
pub fn complete<A: game_ecs::Component>(
    world: &World,
    store: &mut game_ecs::Store,
    actor: PersonId,
    now: SimTime,
    can_progress: bool,
) -> Result<Completion, Error> {
    let (effect, personal, minds) =
        prepare_completion::<A>(world, store, actor, now, can_progress)?;
    let Some(personal) = personal else {
        return Ok(effect);
    };
    let mut batch = game_ecs::Batch::new();
    let personal = personal.stage_into(world, store, now, &mut batch)?;
    let minds = minds.stage_into(store, &mut batch)?;
    personal.validate(world, store, now, &batch)?;
    minds.validate(store, &batch)?;
    store.apply_batch(batch)?;
    Ok(effect)
}
pub fn next_boundary(world: &World, now: SimTime, next: SimTime) -> SimTime {
    SimTime(
        next.0.min(
            world
                .last_review
                .0
                .saturating_add(world.rules.review.0)
                .max(now.0.saturating_add(1)),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_mind() -> game_characters::decisions::Mind<u8> {
        use game_characters::decisions::*;
        Mind {
            interests: BTreeMap::new(),
            routes: BTreeMap::new(),
            last_failure: None,
            child_signals: BTreeMap::new(),
            personality: Personality::industrious(),
            dream: Aspiration::Independence,
            sites: BTreeMap::new(),
            relationships: BTreeMap::new(),
            values: BTreeMap::new(),
            goal: Goal::Rest,
            plan: None,
            explanation: Explanation {
                goal: Goal::Rest,
                need: 0,
                benefit: 0,
                value: 0,
                total: 0,
                blocker: None,
            },
            retry_after: SimTime(0),
        }
    }
    fn fixture() -> (World, game_ecs::Store, PersonFacts, PersonFacts) {
        let mut schema = game_ecs::Schema::new();
        social::register_components(&mut schema).unwrap();
        game_characters::decisions::register_minds::<u8>(&mut schema).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        let temperament = social::Temperament {
            discipline: 500,
            ambition: 500,
            sociability: 800,
            hedonism: 300,
            empathy: 800,
        };
        for id in 1..=3 {
            social::insert_personal(
                &mut store,
                PersonId(id),
                social::Personal {
                    development: social::Development::baseline(
                        temperament,
                        social::Aspiration::Comfort,
                    )
                    .unwrap(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let conversation = ConversationInput {
            temperament,
            affection: 200,
            trust: 300,
            hunger: 10,
            meal_relief: 300,
            fatigue: 0,
            fatigue_limit: 300,
            recipient_adult: true,
            sender_adult: true,
            recipient_aspiration: social::Aspiration::Parenthood,
            chosen_companionship: false,
            visible_disruption: 0,
        };
        for id in 1..=3 {
            game_characters::decisions::insert_mind(&mut store, PersonId(id), test_mind()).unwrap();
        }
        let first = PersonFacts {
            actor: PersonId(1),
            area: Some(AreaId(50)),
            speech: true,
            adult: true,
            practical: true,
            tool: Some(InventoryId(10)),
            materials: BTreeMap::from([(Good::Timber, InventoryId(11))]),
            farming_access: false,
            routes: BTreeMap::new(),
            practice: BTreeMap::from([(Skill::Craft, 500)]),
            empathy: 800,
            patience: 800,
            affection: 200,
            trust: 300,
            conversation: Some(conversation),
        };
        let mut second = first.clone();
        second.actor = PersonId(2);
        (
            World {
                rules: Rules::fixture(),
                last_review: SimTime(0),
            },
            store,
            first,
            second,
        )
    }
    #[test]
    fn chosen_family_plans_and_practical_instruction_use_owner_policy_without_assembly() {
        let (world, mut store, first, mut second) = fixture();
        let family = SocialAction::Talk {
            partner: second.actor,
            topic: SocialTopic::FamilyPlans,
        };
        assert!(matches!(
            prepare_start(
                &world,
                &store,
                &first,
                Some(&second),
                family.clone(),
                SimTime(0)
            )
            .unwrap(),
            StartDecision::Refused { .. }
        ));
        world
            .finish_conversation(
                &mut store,
                first.actor,
                second.actor,
                SocialTopic::Companionship,
            )
            .unwrap();
        assert!(matches!(
            prepare_start(&world, &store, &first, Some(&second), family, SimTime(0)).unwrap(),
            StartDecision::Started { .. }
        ));
        let before = store.snapshot().unwrap();
        let lesson = SocialAction::Learn {
            teacher: second.actor,
            topic: InstructionTopic::Skill(Skill::Craft),
        };
        let decision = prepare_start(
            &world,
            &store,
            &first,
            Some(&second),
            lesson.clone(),
            SimTime(10),
        )
        .unwrap();
        match decision {
            StartDecision::Started {
                interaction:
                    Interaction::Instruction {
                        mastery_cap,
                        tool,
                        completes,
                        ..
                    },
                materials,
                next_choice,
            } => {
                assert_eq!(mastery_cap, 500);
                assert_eq!(tool, Some(InventoryId(10)));
                assert_eq!(
                    materials,
                    Some((second.actor, InventoryId(11), world.rules.practice_material))
                );
                assert_eq!(completes, SimTime(10 + world.rules.instruction.0));
                assert_eq!(next_choice, SimTime(completes.0 + world.rules.review.0));
            }
            _ => panic!("expected instruction"),
        }
        assert_eq!(store.snapshot().unwrap(), before);
        second.practical = false;
        assert_eq!(
            prepare_start(&world, &store, &first, Some(&second), lesson, SimTime(0)),
            Err(Error::Unavailable)
        );
        assert_eq!(
            prepare_start(
                &world,
                &store,
                &first,
                None,
                SocialAction::Practice {
                    skill: Skill::Craft
                },
                SimTime(u64::MAX)
            ),
            Err(Error::Overflow)
        );
    }
    #[test]
    fn teaching_declarations_and_birth_learning_respect_actual_local_witnesses() {
        let (world, mut store, first, second) = fixture();
        let roster = vec![
            SpeakerFacts {
                actor: first.actor,
                area: first.area,
                speech: true,
                guardian: false,
                autonomous: true,
                adult: true,
                offers_teaching: true,
                knows_routes: true,
                practice: BTreeMap::from([
                    (Skill::Craft, 1),
                    (Skill::Invocation, 1),
                    (Skill::Combat, 1),
                ]),
            },
            SpeakerFacts {
                actor: second.actor,
                area: second.area,
                speech: true,
                guardian: false,
                autonomous: true,
                adult: true,
                offers_teaching: false,
                knows_routes: false,
                practice: BTreeMap::new(),
            },
            SpeakerFacts {
                actor: PersonId(3),
                area: Some(AreaId(51)),
                speech: true,
                guardian: false,
                autonomous: true,
                adult: true,
                offers_teaching: false,
                knows_routes: false,
                practice: BTreeMap::new(),
            },
        ];
        publish_teaching_offers(&world, &mut store, &roster, SimTime(10)).unwrap();
        let known = &social::personal(&store, second.actor).unwrap().offers;
        assert_eq!(known.len(), 1);
        assert_eq!(
            known[&first.actor].topics,
            BTreeSet::from([
                InstructionTopic::Routes,
                InstructionTopic::Skill(Skill::Craft)
            ])
        );
        assert!(
            social::personal(&store, PersonId(3))
                .unwrap()
                .offers
                .is_empty()
        );
        let temperament = first.conversation.unwrap().temperament;
        observe_event(
            &world,
            &mut store,
            PersonalReceipt::Born {
                child: PersonId(4),
                temperament,
                witnesses: vec![
                    BirthWitness {
                        actor: first.actor,
                        same_location: true,
                        alive: true,
                    },
                    BirthWitness {
                        actor: second.actor,
                        same_location: false,
                        alive: true,
                    },
                    BirthWitness {
                        actor: PersonId(3),
                        same_location: true,
                        alive: false,
                    },
                ],
            },
            SimTime(20),
        )
        .unwrap();
        assert_eq!(
            social::personal(&store, first.actor)
                .unwrap()
                .development
                .experiences,
            1
        );
        assert_eq!(
            social::personal(&store, second.actor)
                .unwrap()
                .development
                .experiences,
            0
        );
        assert_eq!(
            social::personal(&store, PersonId(3))
                .unwrap()
                .development
                .experiences,
            0
        );
        assert_eq!(
            social::personal(&store, PersonId(4))
                .unwrap()
                .development
                .next_choice,
            SimTime(20)
        );
    }
    #[test]
    fn composed_completion_delivers_taught_routes_and_relationship_gains_to_characters() {
        let (world, mut store, first, second) = fixture();
        let routes =
            BTreeMap::from([(AreaId(50), BTreeMap::from([(AreaId(51), SimDuration(100))]))]);
        let instruction = Interaction::Instruction {
            teacher: first.actor,
            student: second.actor,
            topic: InstructionTopic::Routes,
            routes: routes.clone(),
            mastery_cap: 0,
            tool: None,
            completes: SimTime(100),
        };
        world
            .start(&mut store, first.actor, instruction, SimTime(0))
            .unwrap();
        let before = store.snapshot().unwrap();
        assert_eq!(
            complete::<u8>(&world, &mut store, second.actor, SimTime(99), true).unwrap(),
            Completion::Waiting
        );
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(
            complete::<u8>(&world, &mut store, second.actor, SimTime(100), false).unwrap(),
            Completion::Interrupted
        );
        assert_eq!(store.snapshot().unwrap(), before);
        assert_eq!(
            complete::<u8>(&world, &mut store, second.actor, SimTime(100), true).unwrap(),
            Completion::Instructed {
                teacher: first.actor,
                student: second.actor,
                topic: InstructionTopic::Routes
            }
        );
        assert_eq!(
            game_characters::decisions::mind::<u8>(&store, second.actor)
                .unwrap()
                .routes,
            routes
        );
        assert!(
            social::personal(&store, second.actor)
                .unwrap()
                .learned
                .contains(&(first.actor, InstructionTopic::Routes))
        );
        world.interrupt(&mut store, second.actor).unwrap();
        game_characters::decisions::mind_mut::<u8>(&mut store, first.actor)
            .unwrap()
            .relationships
            .insert(
                second.actor,
                game_characters::decisions::Relationship {
                    affection: 990,
                    trust: 990,
                },
            );
        world
            .start(
                &mut store,
                first.actor,
                Interaction::Talk {
                    first: first.actor,
                    second: second.actor,
                    topic: SocialTopic::Companionship,
                    completes: SimTime(200),
                },
                SimTime(100),
            )
            .unwrap();
        assert_eq!(
            complete::<u8>(&world, &mut store, first.actor, SimTime(200), true).unwrap(),
            Completion::Socialized {
                first: first.actor,
                second: second.actor,
                topic: SocialTopic::Companionship
            }
        );
        let relationship = &game_characters::decisions::mind::<u8>(&store, first.actor)
            .unwrap()
            .relationships[&second.actor];
        assert_eq!((relationship.affection, relationship.trust), (1_000, 1_000));
        let relationship = &game_characters::decisions::mind::<u8>(&store, second.actor)
            .unwrap()
            .relationships[&first.actor];
        assert_eq!((relationship.affection, relationship.trust), (75, 25));
    }
}

#[cfg(test)]
mod craft_availability_tests {
    use super::*;
    #[test]
    fn material_and_tool_availability_are_both_required_for_craft_choice() {
        for (tool, timber, expected) in [
            (None, None, false),
            (Some(InventoryId(1)), None, false),
            (None, Some(InventoryId(2)), false),
            (Some(InventoryId(1)), Some(InventoryId(2)), true),
        ] {
            assert_eq!(CraftAvailability { tool, timber }.can_practice(), expected);
        }
    }
}
