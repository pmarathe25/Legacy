//! Chosen companionship, ordinary instruction, and a person's developing aims.
use game_core::{PersonId, SimTime, Skill};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SocialTopic {
    Companionship,
    FamilyPlans,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum InstructionTopic {
    Routes,
    Skill(Skill),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialAction {
    Talk {
        partner: PersonId,
        topic: SocialTopic,
    },
    Learn {
        teacher: PersonId,
        topic: InstructionTopic,
    },
    Practice {
        skill: Skill,
    },
}

pub use crate::decisions::Aspiration;

/// Ordinary adult family decisions use the established mature life stages.
pub fn adult_stage(stage: Option<game_core::LifeStage>) -> bool {
    matches!(
        stage,
        Some(game_core::LifeStage::Adult | game_core::LifeStage::Elder)
    )
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeachingOfferView {
    pub teacher: PersonId,
    pub topics: Vec<InstructionTopic>,
    pub observed_at: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SocialView {
    pub aspirations: Vec<(Aspiration, u32)>,
    pub experiences: u64,
    pub family_intent: Vec<PersonId>,
    pub teaching_offers: Vec<TeachingOfferView>,
    pub explanation: String,
}

use game_core::{AreaId, ChildhoodInterest, InventoryId, MINUTE, Quantity, SimDuration};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub conversation: SimDuration,
    pub instruction: SimDuration,
    pub practice: SimDuration,
    pub review: SimDuration,
    pub practice_material: Quantity,
    pub aspiration_margin: u32,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            conversation: SimDuration(3_600_000),
            instruction: SimDuration(3_600_000),
            practice: SimDuration(3_600_000),
            review: SimDuration(86_400_000),
            practice_material: Quantity(100),
            aspiration_margin: 50,
        }
    }
}
impl Rules {
    pub fn fixture() -> Self {
        Self {
            conversation: MINUTE,
            instruction: MINUTE,
            practice: MINUTE,
            review: SimDuration(300_000),
            ..Self::default()
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Development {
    pub weights: BTreeMap<Aspiration, u32>,
    pub experiences: u64,
    pub explanation: String,
    pub next_choice: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    pub teacher: PersonId,
    pub topics: BTreeSet<InstructionTopic>,
    pub area: AreaId,
    pub heard_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interaction {
    Talk {
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
        completes: SimTime,
    },
    Instruction {
        teacher: PersonId,
        student: PersonId,
        topic: InstructionTopic,
        routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
        mastery_cap: u64,
        tool: Option<InventoryId>,
        completes: SimTime,
    },
    Practice {
        actor: PersonId,
        skill: Skill,
        tool: InventoryId,
        completes: SimTime,
    },
}
impl Interaction {
    pub fn actors(&self) -> Vec<PersonId> {
        match *self {
            Self::Talk { first, second, .. } => vec![first, second],
            Self::Instruction {
                teacher, student, ..
            } => vec![teacher, student],
            Self::Practice { actor, .. } => vec![actor],
        }
    }
    pub fn completes(&self) -> SimTime {
        match *self {
            Self::Talk { completes, .. }
            | Self::Instruction { completes, .. }
            | Self::Practice { completes, .. } => completes,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct World {
    pub rules: Rules,
    pub last_review: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personal {
    pub development: Development,
    pub offers: BTreeMap<PersonId, Offer>,
    pub family_intent: BTreeSet<PersonId>,
    pub companionship: BTreeSet<PersonId>,
    pub approaches: BTreeMap<PersonId, SimTime>,
    pub learned: BTreeSet<(PersonId, InstructionTopic)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionRow {
    pub interaction: Interaction,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub leader: PersonId,
}
pub fn register_components(schema: &mut game_ecs::Schema) -> Result<(), game_core::Error> {
    schema.register::<Personal>("characters/social-personal", |row| {
        if row.development.weights.len() != 4
            || row
                .development
                .weights
                .values()
                .any(|weight| *weight > 10_000)
            || row.development.explanation.len() > 2_000
            || row.offers.len() > 64
        {
            return Err(game_core::Error::InvalidConfiguration(
                "invalid personal social row".into(),
            ));
        }
        Ok(())
    })?;
    schema.register::<InteractionRow>("characters/social-interaction", |row| {
        let interaction = &row.interaction;
        let actors = interaction.actors();
        let valid = actors.iter().all(|id| id.0 > 0)
            && (actors.len() != 2 || actors[0] != actors[1])
            && match interaction {
                Interaction::Talk { .. } => true,
                Interaction::Practice { skill, tool, .. } => *skill == Skill::Craft && tool.0 > 0,
                Interaction::Instruction {
                    topic,
                    routes,
                    tool,
                    ..
                } => {
                    *topic != InstructionTopic::Skill(Skill::Invocation)
                        && (*topic == InstructionTopic::Routes || routes.is_empty())
                        && tool.is_none_or(|tool| tool.0 > 0)
                        && routes.iter().all(|(area, neighbors)| {
                            area.0 > 0
                                && neighbors
                                    .iter()
                                    .all(|(neighbor, duration)| neighbor.0 > 0 && duration.0 > 0)
                        })
                }
            };
        if valid {
            Ok(())
        } else {
            Err(game_core::Error::InvalidConfiguration(
                "invalid social interaction".into(),
            ))
        }
    })?;
    schema.register::<Participant>("characters/social-participant", |row| {
        if row.leader.0 > 0 {
            Ok(())
        } else {
            Err(game_core::Error::Unavailable)
        }
    })
}
pub fn personal(store: &game_ecs::Store, actor: PersonId) -> Option<&Personal> {
    store.get(game_ecs::StableId(actor.0))
}
pub fn personal_mut(store: &mut game_ecs::Store, actor: PersonId) -> Option<&mut Personal> {
    store.get_mut(game_ecs::StableId(actor.0))
}
pub fn personals(store: &game_ecs::Store) -> impl Iterator<Item = (PersonId, &Personal)> {
    store
        .iter::<Personal>()
        .map(|(id, row)| (PersonId(id.0), row))
}
pub fn insert_personal(
    store: &mut game_ecs::Store,
    actor: PersonId,
    row: Personal,
) -> Result<(), game_core::Error> {
    store.insert_or_spawn(game_ecs::StableId(actor.0), row)
}
pub fn interaction(store: &game_ecs::Store, leader: PersonId) -> Option<&Interaction> {
    store
        .get::<InteractionRow>(game_ecs::StableId(leader.0))
        .map(|row| &row.interaction)
}
pub fn interactions(store: &game_ecs::Store) -> impl Iterator<Item = (PersonId, &Interaction)> {
    store
        .iter::<InteractionRow>()
        .map(|(id, row)| (PersonId(id.0), &row.interaction))
}
pub fn participant(store: &game_ecs::Store, actor: PersonId) -> Option<PersonId> {
    store
        .get::<Participant>(game_ecs::StableId(actor.0))
        .map(|row| row.leader)
}
/// Select the linked cancellation operation; exact reservation membership is
/// witnessed by `stage_cancel_interaction` when the operation is prepared.
pub fn cancellation_interaction(
    store: &game_ecs::Store,
    actor: PersonId,
) -> Option<(PersonId, &Interaction)> {
    let leader = participant(store, actor)?;
    Some((leader, interaction(store, leader)?))
}
pub fn participants(store: &game_ecs::Store) -> impl Iterator<Item = (PersonId, PersonId)> {
    store
        .iter::<Participant>()
        .map(|(id, row)| (PersonId(id.0), row.leader))
}
pub fn pair(first: PersonId, second: PersonId) -> (PersonId, PersonId) {
    (first.min(second), first.max(second))
}

/// A caller supplies the relevant temperament values, not a simulation Mind or
/// an omniscient world snapshot. These values are transient decision inputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Temperament {
    pub discipline: u16,
    pub ambition: u16,
    pub sociability: u16,
    pub hedonism: u16,
    pub empathy: u16,
}
impl Temperament {
    pub fn from_personality(personality: &crate::decisions::Personality) -> Self {
        Self {
            discipline: personality.discipline,
            ambition: personality.ambition,
            sociability: personality.sociability,
            hedonism: personality.hedonism,
            empathy: personality.empathy,
        }
    }
    pub fn valid(self) -> bool {
        [
            self.discipline,
            self.ambition,
            self.sociability,
            self.hedonism,
            self.empathy,
        ]
        .into_iter()
        .all(|value| value <= 1_000)
    }
}
impl Rules {
    pub fn valid(&self) -> bool {
        [
            self.conversation.0,
            self.instruction.0,
            self.practice.0,
            self.review.0,
        ]
        .into_iter()
        .all(|duration| duration > 0)
            && (1..=1_000).contains(&self.practice_material.0)
            && self.aspiration_margin <= 1_000
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpbringingEvidence {
    WitnessedBirth,
    ReceivedCare,
    GaveCare,
    ReceivedComfort,
    GaveComfort,
    Produced,
    Played(ChildhoodInterest),
    ReceivedInstruction,
    GaveInstruction,
    Conversation(SocialTopic),
}
impl Development {
    pub fn baseline(
        temperament: Temperament,
        current: Aspiration,
    ) -> Result<Self, game_core::Error> {
        if !temperament.valid() {
            return Err(game_core::Error::InvalidConfiguration(
                "invalid social temperament".into(),
            ));
        }
        let mut weights = BTreeMap::from([
            (Aspiration::Comfort, u32::from(temperament.hedonism) / 2),
            (
                Aspiration::Independence,
                u32::from(temperament.ambition + temperament.discipline) / 4,
            ),
            (
                Aspiration::SupportOthers,
                u32::from(temperament.empathy) / 2,
            ),
            (
                Aspiration::Parenthood,
                u32::from(temperament.empathy + temperament.sociability) / 4,
            ),
        ]);
        *weights.entry(current).or_default() += 100;
        Ok(Self {
            weights,
            explanation: "My aims reflect my temperament and remembered experiences".into(),
            ..Self::default()
        })
    }

    /// The adapter supplies an actually experienced event. No remote person's
    /// behavior or biological genotype is inspected by this transition.
    pub fn observe(&mut self, aspiration: Aspiration, amount: u32, reason: &str) {
        if self.weights.values().any(|weight| *weight >= 9_000) {
            for weight in self.weights.values_mut() {
                *weight /= 2;
            }
        }
        let weight = self.weights.entry(aspiration).or_default();
        *weight = weight.saturating_add(amount).min(10_000);
        self.experiences = self.experiences.saturating_add(1);
        self.explanation = reason.into();
    }
    pub fn observe_event(&mut self, evidence: UpbringingEvidence) {
        match evidence {
            UpbringingEvidence::WitnessedBirth => self.observe(
                Aspiration::Parenthood,
                20,
                "I witnessed a birth and considered raising children",
            ),
            UpbringingEvidence::ReceivedCare => {
                self.observe(
                    Aspiration::Parenthood,
                    20,
                    "Receiving chosen care shaped how I imagine raising children",
                );
                self.observe(
                    Aspiration::SupportOthers,
                    15,
                    "I remember someone choosing to help me",
                );
            }
            UpbringingEvidence::GaveCare => self.observe(
                Aspiration::SupportOthers,
                20,
                "I chose to support a dependent person",
            ),
            UpbringingEvidence::ReceivedComfort => self.observe(
                Aspiration::SupportOthers,
                15,
                "Chosen companionship taught me what support can feel like",
            ),
            UpbringingEvidence::GaveComfort => self.observe(
                Aspiration::SupportOthers,
                15,
                "I chose to comfort another person",
            ),
            UpbringingEvidence::Produced => self.observe(
                Aspiration::Independence,
                30,
                "My chosen practical work made independence more attractive",
            ),
            UpbringingEvidence::Played(interest) => {
                let aspiration = match interest {
                    ChildhoodInterest::Craft | ChildhoodInterest::Movement => {
                        Aspiration::Independence
                    }
                    ChildhoodInterest::Company => Aspiration::SupportOthers,
                    ChildhoodInterest::Food => Aspiration::Comfort,
                };
                self.observe(
                    aspiration,
                    10,
                    "My chosen play changed what I enjoy and hope to do",
                );
            }
            UpbringingEvidence::ReceivedInstruction => self.observe(
                Aspiration::Independence,
                20,
                "Learning from another person gave me more independent choices",
            ),
            UpbringingEvidence::GaveInstruction => self.observe(
                Aspiration::SupportOthers,
                20,
                "I chose to share my experience",
            ),
            UpbringingEvidence::Conversation(topic) => self.observe(
                if topic == SocialTopic::FamilyPlans {
                    Aspiration::Parenthood
                } else {
                    Aspiration::SupportOthers
                },
                30,
                "Our chosen conversation changed how I imagine my future",
            ),
        }
    }
    pub fn preferred_aspiration(&self, current: Aspiration, margin: u32) -> Option<Aspiration> {
        if self.experiences == 0 {
            return None;
        }
        self.weights
            .iter()
            .max_by_key(|(aspiration, weight)| (**weight, std::cmp::Reverse(**aspiration)))
            .filter(|(_, weight)| {
                **weight
                    > self
                        .weights
                        .get(&current)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(margin)
            })
            .map(|(aspiration, _)| *aspiration)
    }
}

/// Own recipient needs, remembered relationship, and locally visible behavior.
/// The adapter checks that the conversation can physically occur before calling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversationInput {
    pub temperament: Temperament,
    pub affection: i32,
    pub trust: i32,
    pub hunger: u64,
    pub meal_relief: u64,
    pub fatigue: u64,
    pub fatigue_limit: u64,
    pub recipient_adult: bool,
    pub sender_adult: bool,
    pub recipient_aspiration: Aspiration,
    pub chosen_companionship: bool,
    pub visible_disruption: i64,
}
pub fn conversation_willing(topic: SocialTopic, input: ConversationInput) -> bool {
    if !input.temperament.valid()
        || !(-1_000..=1_000).contains(&input.affection)
        || !(-1_000..=1_000).contains(&input.trust)
        || input.visible_disruption < 0
    {
        return false;
    }
    if input.hunger >= input.meal_relief
        || input.fatigue >= input.fatigue_limit
        || input.trust < -100
    {
        return false;
    }
    let disruption = i128::from(input.visible_disruption)
        * i128::from(1_000 - i64::from(input.trust).clamp(-1_000, 1_000))
        / 1_000;
    match topic {
        SocialTopic::Companionship => {
            i128::from(input.temperament.sociability + input.temperament.empathy) / 2
                + i128::from(input.affection)
                - disruption
                >= 200
        }
        SocialTopic::FamilyPlans => {
            input.recipient_adult
                && input.sender_adult
                && input.recipient_aspiration == Aspiration::Parenthood
                && input.affection >= 100
                && input.trust >= 0
                && i128::from(input.temperament.sociability) + i128::from(input.affection.max(0))
                    >= disruption
                && input.chosen_companionship
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConversationEffect {
    pub first: PersonId,
    pub second: PersonId,
    pub affection_gain: i32,
    pub trust_gain: i32,
    pub formed_family_intent: bool,
}
impl World {
    pub fn observe(
        &self,
        store: &mut game_ecs::Store,
        actor: PersonId,
        evidence: UpbringingEvidence,
    ) {
        if let Some(row) = personal_mut(store, actor) {
            row.development.observe_event(evidence);
        }
    }
    pub fn conversation_effect(
        &self,
        store: &game_ecs::Store,
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
    ) -> Result<ConversationEffect, game_core::Error> {
        if first == second || personal(store, first).is_none() || personal(store, second).is_none()
        {
            return Err(game_core::Error::Unavailable);
        }
        Ok(ConversationEffect {
            first,
            second,
            affection_gain: 75,
            trust_gain: 25,
            formed_family_intent: topic == SocialTopic::FamilyPlans,
        })
    }
    pub fn finish_conversation(
        &self,
        store: &mut game_ecs::Store,
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
    ) -> Result<ConversationEffect, game_core::Error> {
        let effect = self.conversation_effect(store, first, second, topic)?;
        let a = conversation_personal_after(personal(store, first).unwrap(), second, topic);
        let b = conversation_personal_after(personal(store, second).unwrap(), first, topic);
        let mut batch = game_ecs::Batch::new();
        batch
            .insert(game_ecs::StableId(first.0), a)
            .insert(game_ecs::StableId(second.0), b);
        store.apply_batch(batch)?;
        Ok(effect)
    }
    pub fn remember_offer(
        &self,
        store: &mut game_ecs::Store,
        observer: PersonId,
        offer: Offer,
        now: SimTime,
    ) -> Result<(), game_core::Error> {
        if observer == offer.teacher
            || personal(store, offer.teacher).is_none()
            || offer.heard_at > now
            || offer
                .topics
                .contains(&InstructionTopic::Skill(Skill::Invocation))
        {
            return Err(game_core::Error::Unavailable);
        }
        let known = &mut personal_mut(store, observer)
            .ok_or(game_core::Error::Unavailable)?
            .offers;
        known.insert(offer.teacher, offer);
        while known.len() > 64 {
            let oldest = *known
                .iter()
                .min_by_key(|(teacher, offer)| (offer.heard_at, **teacher))
                .unwrap()
                .0;
            known.remove(&oldest);
        }
        Ok(())
    }
    pub fn view(&self, store: &game_ecs::Store, actor: PersonId) -> Option<SocialView> {
        let row = personal(store, actor)?;
        Some(SocialView {
            aspirations: row
                .development
                .weights
                .iter()
                .map(|(aim, weight)| (*aim, *weight))
                .collect(),
            experiences: row.development.experiences,
            family_intent: row.family_intent.iter().copied().collect(),
            teaching_offers: row
                .offers
                .values()
                .map(|offer| TeachingOfferView {
                    teacher: offer.teacher,
                    topics: offer.topics.iter().copied().collect(),
                    observed_at: offer.heard_at,
                })
                .collect(),
            explanation: row.development.explanation.clone(),
        })
    }
    pub fn prepare_start(
        &self,
        store: &game_ecs::Store,
        leader: PersonId,
        interaction: Interaction,
        now: SimTime,
    ) -> Result<PreparedSocialStart, game_core::Error> {
        if !self.rules.valid() {
            return Err(game_core::Error::InvalidConfiguration(
                "invalid social development rules".into(),
            ));
        }
        if !interaction.actors().contains(&leader) || interaction.completes() <= now {
            return Err(game_core::Error::Unavailable);
        }
        let next = SimTime(
            interaction
                .completes()
                .0
                .checked_add(self.rules.review.0)
                .ok_or(game_core::Error::Overflow)?,
        );
        let mut reads = game_ecs::ReadSet::new();
        if reads
            .read::<InteractionRow>(store, game_ecs::StableId(leader.0))
            .is_some()
        {
            return Err(game_core::Error::Unavailable);
        }
        let mut people = BTreeMap::new();
        for actor in interaction.actors() {
            let id = game_ecs::StableId(actor.0);
            if reads.read::<Participant>(store, id).is_some() {
                return Err(game_core::Error::Unavailable);
            }
            let mut row = reads
                .read::<Personal>(store, id)
                .cloned()
                .ok_or(game_core::Error::Unavailable)?;
            row.development.next_choice = next;
            row.development.explanation =
                "I chose to spend time in this conversation or practice".into();
            if let Interaction::Talk { first, second, .. } = interaction
                && actor == first
            {
                row.approaches.insert(second, now);
            }
            if people.insert(actor, row).is_some() {
                return Err(game_core::Error::Unavailable);
            }
        }
        Ok(PreparedSocialStart {
            rules: self.rules.clone(),
            now,
            leader,
            interaction,
            people,
            reads,
        })
    }
    pub fn start(
        &self,
        store: &mut game_ecs::Store,
        leader: PersonId,
        interaction: Interaction,
        now: SimTime,
    ) -> Result<(), game_core::Error> {
        let mut batch = game_ecs::Batch::new();
        let guard = self
            .prepare_start(store, leader, interaction, now)?
            .stage_into(self, store, now, &mut batch)?;
        guard.validate(self, store, now, &batch)?;
        store.apply_batch(batch)
    }
    pub fn refuse(
        &self,
        store: &mut game_ecs::Store,
        actor: PersonId,
        recipient: PersonId,
        now: SimTime,
        reason: &str,
    ) -> Result<(), game_core::Error> {
        let row = personal_mut(store, actor).ok_or(game_core::Error::Unavailable)?;
        row.approaches.insert(recipient, now);
        row.development.explanation = reason.into();
        Ok(())
    }
    pub fn interrupt(
        &self,
        store: &mut game_ecs::Store,
        actor: PersonId,
    ) -> Result<Vec<PersonId>, game_core::Error> {
        let Some(key) = participant(store, actor) else {
            return Ok(Vec::new());
        };
        let expected = interaction(store, key)
            .cloned()
            .ok_or(game_core::Error::Unavailable)?;
        let actors = expected.actors();
        let mut batch = game_ecs::Batch::new();
        let guard = stage_cancel_interaction(store, key, &expected, &mut batch)?;
        guard.validate(store, &batch)?;
        store.apply_batch(batch)?;
        Ok(actors)
    }
    pub fn record_instruction(
        &self,
        store: &mut game_ecs::Store,
        teacher: PersonId,
        student: PersonId,
        topic: InstructionTopic,
    ) {
        personal_mut(store, student)
            .expect("student social row")
            .learned
            .insert((teacher, topic));
    }
    pub fn reset(
        &mut self,
        store: &mut game_ecs::Store,
        now: SimTime,
    ) -> Result<(), game_core::Error> {
        let mut batch = game_ecs::Batch::new();
        for (id, _) in interactions(store) {
            batch.remove::<InteractionRow>(game_ecs::StableId(id.0));
        }
        for (id, _) in participants(store) {
            batch.remove::<Participant>(game_ecs::StableId(id.0));
        }
        for (id, row) in personals(store) {
            let mut row = row.clone();
            row.development.next_choice = now;
            batch.insert(game_ecs::StableId(id.0), row);
        }
        store.apply_batch(batch)?;
        self.last_review = now;
        Ok(())
    }
}
pub fn validate_local(
    world: &World,
    store: &game_ecs::Store,
    now: SimTime,
) -> Result<(), game_core::Error> {
    let fail = || game_core::Error::InvalidSave("inconsistent social development".into());
    if !world.rules.valid() || world.last_review > now {
        return Err(fail());
    }
    for (id, row) in personals(store) {
        if row.development.weights.len() != 4
            || row
                .development
                .weights
                .values()
                .any(|weight| *weight > 10_000)
            || row.development.explanation.len() > 2_000
            || row.offers.len() > 64
        {
            return Err(fail());
        }
        for other in &row.companionship {
            if *other == id
                || personal(store, *other).is_none_or(|peer| !peer.companionship.contains(&id))
            {
                return Err(fail());
            }
        }
        for other in &row.family_intent {
            if !row.companionship.contains(other)
                || personal(store, *other).is_none_or(|peer| !peer.family_intent.contains(&id))
            {
                return Err(fail());
            }
        }
        for (teacher, offer) in &row.offers {
            if *teacher != offer.teacher
                || *teacher == id
                || personal(store, *teacher).is_none()
                || offer.heard_at > now
                || offer
                    .topics
                    .contains(&InstructionTopic::Skill(Skill::Invocation))
            {
                return Err(fail());
            }
        }
        if row
            .approaches
            .iter()
            .any(|(other, at)| *other == id || personal(store, *other).is_none() || *at > now)
            || row.learned.iter().any(|(teacher, topic)| {
                *teacher == id
                    || personal(store, *teacher).is_none()
                    || *topic == InstructionTopic::Skill(Skill::Invocation)
            })
        {
            return Err(fail());
        }
    }
    let mut expected = BTreeMap::new();
    for (leader, interaction) in interactions(store) {
        let actors = interaction.actors();
        if !actors.contains(&leader)
            || interaction.completes() <= now
            || actors.len() == 2 && actors[0] == actors[1]
        {
            return Err(fail());
        }
        for id in actors {
            if personal(store, id).is_none() || expected.insert(id, leader).is_some() {
                return Err(fail());
            }
        }
        match interaction {
            Interaction::Instruction { topic, routes, .. }
                if *topic == InstructionTopic::Skill(Skill::Invocation)
                    || *topic != InstructionTopic::Routes && !routes.is_empty()
                    || routes
                        .values()
                        .any(|neighbors| neighbors.values().any(|duration| duration.0 == 0)) =>
            {
                return Err(fail());
            }
            Interaction::Practice { skill, .. } if *skill != Skill::Craft => return Err(fail()),
            _ => {}
        }
    }
    if participants(store).collect::<BTreeMap<_, _>>() != expected {
        return Err(fail());
    }
    Ok(())
}

/// Actual available contacts and practice materials. The person's routes,
/// received offers, relationship history and intentions stay in domain state.
#[derive(Clone, Debug)]
pub struct ChoiceFacts {
    pub actor: PersonId,
    pub now: SimTime,
    pub autonomous: bool,
    pub can_speak: bool,
    pub adult: bool,
    pub family_peers: Vec<PersonId>,
    pub available_teachers: BTreeSet<PersonId>,
    pub own_levels: BTreeMap<Skill, u16>,
    pub can_practice_craft: bool,
    pub companionship_partner: Option<PersonId>,
}
impl World {
    pub fn choose_personal<A>(
        &self,
        store: &game_ecs::Store,
        mind: &crate::decisions::Mind<A>,
        facts: &ChoiceFacts,
    ) -> Option<SocialAction> {
        let own = personal(store, facts.actor)?;
        if !facts.autonomous || !facts.can_speak || own.development.next_choice > facts.now {
            return None;
        }
        if mind.dream == Aspiration::Parenthood && facts.adult {
            let mut peers = facts.family_peers.clone();
            peers.sort_by_key(|partner| {
                (
                    own.approaches.get(partner).copied().unwrap_or(SimTime(0)),
                    *partner,
                )
            });
            for partner in peers {
                if !own.family_intent.contains(&partner) {
                    let affectionate = mind
                        .relationships
                        .get(&partner)
                        .is_some_and(|relation| relation.affection >= 100);
                    return Some(SocialAction::Talk {
                        partner,
                        topic: if own.companionship.contains(&partner) && affectionate {
                            SocialTopic::FamilyPlans
                        } else {
                            SocialTopic::Companionship
                        },
                    });
                }
            }
        }
        for offer in own.offers.values() {
            if !facts.available_teachers.contains(&offer.teacher) {
                continue;
            }
            if offer.topics.contains(&InstructionTopic::Routes)
                && !own
                    .learned
                    .contains(&(offer.teacher, InstructionTopic::Routes))
                && mind.routes.len() < 2
            {
                return Some(SocialAction::Learn {
                    teacher: offer.teacher,
                    topic: InstructionTopic::Routes,
                });
            }
            for topic in &offer.topics {
                if let InstructionTopic::Skill(skill) = topic
                    && facts.own_levels.get(skill).copied().unwrap_or(0) == 0
                    && matches!(
                        skill,
                        Skill::Craft
                            | Skill::Farming
                            | Skill::Medicine
                            | Skill::Negotiation
                            | Skill::Leadership
                    )
                {
                    return Some(SocialAction::Learn {
                        teacher: offer.teacher,
                        topic: *topic,
                    });
                }
            }
        }
        if mind.personality.discipline + mind.personality.ambition >= 800
            && facts.can_practice_craft
        {
            return Some(SocialAction::Practice {
                skill: Skill::Craft,
            });
        }
        if mind.personality.sociability >= 500 {
            return facts
                .companionship_partner
                .map(|partner| SocialAction::Talk {
                    partner,
                    topic: SocialTopic::Companionship,
                });
        }
        None
    }
    pub fn finish_personal_choice(
        &self,
        store: &mut game_ecs::Store,
        actor: PersonId,
        now: SimTime,
    ) -> Result<(), game_core::Error> {
        let next = SimTime(
            now.0
                .checked_add(self.rules.review.0)
                .ok_or(game_core::Error::Overflow)?,
        );
        personal_mut(store, actor)
            .ok_or(game_core::Error::Unavailable)?
            .development
            .next_choice = next;
        Ok(())
    }
}

pub fn stage_cancel_interaction(
    store: &game_ecs::Store,
    leader: PersonId,
    expected: &Interaction,
    batch: &mut game_ecs::Batch,
) -> Result<game_ecs::ReadGuard, game_core::Error> {
    use game_ecs::{ReadFootprint, ReadSet, StableId};
    if interaction(store, leader) != Some(expected) {
        return Err(game_core::Error::Unavailable);
    }
    let mut reads = ReadSet::new();
    reads.read::<InteractionRow>(store, StableId(leader.0));
    let members = reads.membership::<Participant>(store, move |_, row| row.leader == leader);
    let actors = expected
        .actors()
        .into_iter()
        .map(|id| StableId(id.0))
        .collect::<BTreeSet<_>>();
    if members != actors {
        return Err(game_core::Error::Unavailable);
    }
    let mut writes = ReadFootprint::new();
    if batch.touches::<InteractionRow>(StableId(leader.0)) {
        return Err(game_core::Error::Unavailable);
    }
    batch.remove::<InteractionRow>(StableId(leader.0));
    writes.protect::<InteractionRow>(StableId(leader.0));
    for id in actors {
        reads.read::<Participant>(store, id);
        if batch.touches::<Participant>(id) {
            return Err(game_core::Error::Unavailable);
        }
        batch.remove::<Participant>(id);
        writes.protect::<Participant>(id);
    }
    reads.bind(store, batch, &writes)
}

#[derive(Clone, Copy, Debug)]
pub struct SpeechFacts {
    pub stationary: bool,
    pub independent: bool,
    pub in_combat: bool,
    pub alive: bool,
    pub health: u64,
    pub idle_or_rest: bool,
    pub receiving_care: bool,
    pub conversation_available: bool,
}
pub fn can_speak(facts: SpeechFacts) -> bool {
    facts.stationary
        && facts.independent
        && !facts.in_combat
        && facts.alive
        && facts.health > 0
        && facts.idle_or_rest
        && !facts.receiving_care
        && facts.conversation_available
}
#[derive(Clone, Copy, Debug)]
pub struct PracticalFacts {
    pub combat_restricted: bool,
    pub fatigue: u64,
    pub fatigue_limit: u64,
}
pub fn can_practice(facts: PracticalFacts) -> bool {
    !facts.combat_restricted && facts.fatigue < facts.fatigue_limit
}
#[derive(Clone, Copy, Debug)]
pub struct InteractionActorFacts {
    pub actor: PersonId,
    pub area: Option<AreaId>,
    pub alive: bool,
    pub health: u64,
    pub in_combat: bool,
    pub practical: PracticalFacts,
}
pub fn interaction_is_practical(interaction: &Interaction) -> bool {
    matches!(
        interaction,
        Interaction::Practice { .. }
            | Interaction::Instruction {
                topic: InstructionTopic::Skill(
                    Skill::Craft | Skill::Farming | Skill::Medicine | Skill::Combat
                ),
                ..
            }
    )
}
pub fn selected_tool(interaction: &Interaction, actor: PersonId) -> Option<InventoryId> {
    match interaction {
        Interaction::Instruction { teacher, tool, .. } if *teacher == actor => *tool,
        Interaction::Practice { tool, .. } => Some(*tool),
        _ => None,
    }
}
pub fn interaction_tool(interaction: &Interaction) -> Option<(PersonId, InventoryId)> {
    match *interaction {
        Interaction::Instruction {
            teacher,
            tool: Some(tool),
            ..
        } => Some((teacher, tool)),
        Interaction::Practice { actor, tool, .. } => Some((actor, tool)),
        _ => None,
    }
}
pub fn interaction_ready(
    interaction: &Interaction,
    facts: &[InteractionActorFacts],
    tool_available: bool,
) -> bool {
    let mut area = None;
    let practical = interaction_is_practical(interaction);
    for actor in interaction.actors() {
        let Some(row) = facts.iter().find(|row| row.actor == actor) else {
            return false;
        };
        if !row.alive
            || row.health == 0
            || row.in_combat
            || row.area.is_none()
            || area.is_some_and(|area| Some(area) != row.area)
            || practical && !can_practice(row.practical)
        {
            return false;
        }
        area = row.area;
    }
    interaction_tool(interaction).is_none()
        || tool_available && facts.iter().all(|row| can_practice(row.practical))
}

pub struct PreparedSocialStart {
    rules: Rules,
    now: SimTime,
    leader: PersonId,
    interaction: Interaction,
    people: BTreeMap<PersonId, Personal>,
    reads: game_ecs::ReadSet,
}
pub struct SocialStartGuard {
    rules: Rules,
    now: SimTime,
    rows: game_ecs::ReadGuard,
}
impl PreparedSocialStart {
    pub fn stage_into(
        self,
        world: &World,
        store: &game_ecs::Store,
        now: SimTime,
        batch: &mut game_ecs::Batch,
    ) -> Result<SocialStartGuard, game_core::Error> {
        if world.rules != self.rules || now != self.now {
            return Err(game_core::Error::Unavailable);
        }
        self.reads.validate(store)?;
        let leader = game_ecs::StableId(self.leader.0);
        if batch.touches::<InteractionRow>(leader)
            || self.people.keys().any(|actor| {
                batch.touches::<Personal>(game_ecs::StableId(actor.0))
                    || batch.touches::<Participant>(game_ecs::StableId(actor.0))
            })
        {
            return Err(game_core::Error::Unavailable);
        }
        let mut writes = game_ecs::ReadFootprint::new();
        for (actor, row) in self.people {
            let id = game_ecs::StableId(actor.0);
            batch.insert(id, row).insert(
                id,
                Participant {
                    leader: self.leader,
                },
            );
            writes.protect::<Personal>(id);
            writes.protect::<Participant>(id);
        }
        batch.insert(
            leader,
            InteractionRow {
                interaction: self.interaction,
            },
        );
        writes.protect::<InteractionRow>(leader);
        Ok(SocialStartGuard {
            rules: self.rules,
            now: self.now,
            rows: self.reads.bind(store, batch, &writes)?,
        })
    }
}
impl SocialStartGuard {
    pub fn row_guard(&self) -> game_ecs::ReadGuard {
        self.rows.clone()
    }
    pub fn validate(
        &self,
        world: &World,
        store: &game_ecs::Store,
        now: SimTime,
        batch: &game_ecs::Batch,
    ) -> Result<(), game_core::Error> {
        if world.rules != self.rules || now != self.now {
            return Err(game_core::Error::Unavailable);
        }
        self.rows.validate(store, batch)
    }
}

pub struct ReconciliationFacts {
    pub leader: PersonId,
    pub actors: Vec<InteractionActorFacts>,
    pub tool_available: bool,
}
pub fn interrupted_interactions(
    store: &game_ecs::Store,
    facts: &[ReconciliationFacts],
) -> Vec<PersonId> {
    let by_leader = facts
        .iter()
        .map(|row| (row.leader, row))
        .collect::<BTreeMap<_, _>>();
    interactions(store)
        .filter(|(leader, interaction)| {
            by_leader
                .get(leader)
                .is_none_or(|row| !interaction_ready(interaction, &row.actors, row.tool_available))
        })
        .map(|(leader, _)| leader)
        .collect()
}

/// Personal development follows the current ruler identity and authored role.
/// A past ruler who now serves as a guardian retains delegated control.
pub fn aspiration_control(
    is_current_ruler: bool,
    role: game_core::Role,
) -> crate::decisions::Control {
    if is_current_ruler {
        crate::decisions::Control::Manual
    } else if role == game_core::Role::Guardian {
        crate::decisions::Control::Delegated
    } else {
        crate::decisions::Control::Autonomous
    }
}

/// Factual variants supplied by an activity binding; Characters selects cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocialActivityKind {
    Other,
    Conversation,
    Instruction,
    Practice,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityCleanup {
    Idle { actor: PersonId },
}
pub fn cleanup_activity(actor: PersonId, kind: SocialActivityKind) -> Option<ActivityCleanup> {
    matches!(
        kind,
        SocialActivityKind::Conversation
            | SocialActivityKind::Instruction
            | SocialActivityKind::Practice
    )
    .then_some(ActivityCleanup::Idle { actor })
}
pub struct PreparedSocialInterruption<A: game_ecs::Component> {
    leader: PersonId,
    expected: Interaction,
    actors: Vec<PersonId>,
    cleanup: Vec<ActivityCleanup>,
    reads: game_ecs::ReadSet,
    marker: std::marker::PhantomData<A>,
}
pub struct SocialInterruptionPublication {
    leader: PersonId,
    expected: Interaction,
    participants: game_ecs::ReadGuard,
    activities: game_ecs::ReadGuard,
    pub actors: Vec<PersonId>,
}
impl World {
    pub fn prepare_interruption<A: game_ecs::Component>(
        &self,
        store: &game_ecs::Store,
        actor: PersonId,
        kinds: &BTreeMap<PersonId, SocialActivityKind>,
    ) -> Result<Option<PreparedSocialInterruption<A>>, game_core::Error> {
        let Some(leader) = participant(store, actor) else {
            return Ok(None);
        };
        let expected = interaction(store, leader)
            .cloned()
            .ok_or(game_core::Error::Unavailable)?;
        let actors = expected.actors();
        let mut reads = game_ecs::ReadSet::new();
        reads.read::<InteractionRow>(store, game_ecs::StableId(leader.0));
        let mut cleanup = Vec::new();
        for actor in &actors {
            reads.read::<Participant>(store, game_ecs::StableId(actor.0));
            reads
                .read::<A>(store, game_ecs::StableId(actor.0))
                .ok_or(game_core::Error::Unavailable)?;
            if let Some(effect) = cleanup_activity(
                *actor,
                *kinds.get(actor).ok_or(game_core::Error::Unavailable)?,
            ) {
                cleanup.push(effect);
            }
        }
        Ok(Some(PreparedSocialInterruption {
            leader,
            expected,
            actors,
            cleanup,
            reads,
            marker: std::marker::PhantomData,
        }))
    }
}
impl<A: game_ecs::Component> PreparedSocialInterruption<A> {
    pub fn cleanup(&self) -> &[ActivityCleanup] {
        &self.cleanup
    }
    pub fn stage_into(
        self,
        store: &game_ecs::Store,
        batch: &mut game_ecs::Batch,
        idle: A,
    ) -> Result<SocialInterruptionPublication, game_core::Error> {
        self.reads.validate(store)?;
        let participants = stage_cancel_interaction(store, self.leader, &self.expected, batch)?;
        let mut writes = game_ecs::ReadFootprint::new();
        writes.protect::<InteractionRow>(game_ecs::StableId(self.leader.0));
        for actor in &self.actors {
            writes.protect::<Participant>(game_ecs::StableId(actor.0));
        }
        for effect in self.cleanup {
            let ActivityCleanup::Idle { actor } = effect;
            let id = game_ecs::StableId(actor.0);
            if batch.touches::<A>(id) {
                return Err(game_core::Error::Unavailable);
            }
            writes.protect::<A>(id);
            batch.insert(id, idle.clone());
        }
        let activities = self.reads.bind(store, batch, &writes)?;
        Ok(SocialInterruptionPublication {
            leader: self.leader,
            expected: self.expected,
            participants,
            activities,
            actors: self.actors,
        })
    }
}
impl SocialInterruptionPublication {
    pub fn validate(
        &self,
        store: &game_ecs::Store,
        batch: &game_ecs::Batch,
    ) -> Result<(), game_core::Error> {
        self.participants.validate(store, batch)?;
        self.activities.validate(store, batch)
    }
    pub fn authorize_completion(
        &self,
        store: &game_ecs::Store,
        batch: &game_ecs::Batch,
        leader: PersonId,
        expected: &Interaction,
    ) -> Result<(), game_core::Error> {
        if self.leader != leader || &self.expected != expected {
            return Err(game_core::Error::Unavailable);
        }
        self.validate(store, batch)
    }
    pub fn row_guards(&self) -> Vec<game_ecs::ReadGuard> {
        vec![self.participants.clone(), self.activities.clone()]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocialActivity {
    Conversation {
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
        tool: InventoryId,
        completes: SimTime,
    },
}
impl Interaction {
    pub fn activity_bindings(&self) -> Vec<(PersonId, SocialActivity)> {
        self.actors()
            .into_iter()
            .map(|actor| {
                let activity = match *self {
                    Self::Talk {
                        first,
                        second,
                        topic,
                        completes,
                    } => SocialActivity::Conversation {
                        partner: if actor == first { second } else { first },
                        topic,
                        completes,
                    },
                    Self::Instruction {
                        teacher,
                        student,
                        topic,
                        completes,
                        ..
                    } => SocialActivity::Instruction {
                        teacher,
                        student,
                        topic,
                        completes,
                    },
                    Self::Practice {
                        skill,
                        tool,
                        completes,
                        ..
                    } => SocialActivity::Practice {
                        skill,
                        tool,
                        completes,
                    },
                };
                (actor, activity)
            })
            .collect()
    }
}

pub fn conversation_personal_after(
    row: &Personal,
    partner: PersonId,
    topic: SocialTopic,
) -> Personal {
    let mut row = row.clone();
    row.companionship.insert(partner);
    if topic == SocialTopic::FamilyPlans {
        row.family_intent.insert(partner);
    }
    row
}

#[cfg(test)]
mod native_tests {
    use super::*;
    fn temperament() -> Temperament {
        Temperament {
            discipline: 500,
            ambition: 500,
            sociability: 800,
            hedonism: 300,
            empathy: 800,
        }
    }
    pub(super) fn fixture() -> (World, game_ecs::Store) {
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        schema.register::<u8>("foreign", |_| Ok(())).unwrap();
        let mut store = game_ecs::Store::with_schema(schema);
        for id in 1..=100 {
            insert_personal(
                &mut store,
                PersonId(id),
                Personal {
                    development: Development::baseline(temperament(), Aspiration::Comfort).unwrap(),
                    ..Default::default()
                },
            )
            .unwrap();
        }
        store.insert_or_spawn(game_ecs::StableId(1), 7u8).unwrap();
        (World::default(), store)
    }
    #[test]
    fn aspiration_control_keeps_ruler_precedence_and_delegated_past_life_guardians() {
        use crate::decisions::Control;
        use game_core::Role;
        for (current_ruler, role, expected) in [
            (true, Role::Ruler, Control::Manual),
            (false, Role::Guardian, Control::Delegated),
            (false, Role::Citizen, Control::Autonomous),
            (true, Role::Guardian, Control::Manual),
            (false, Role::Ruler, Control::Autonomous),
        ] {
            assert_eq!(aspiration_control(current_ruler, role), expected);
        }
    }
    #[test]
    fn paired_intent_native_rows_views_and_foreign_components_roundtrip() {
        let (world, mut store) = fixture();
        let before = store.snapshot().unwrap();
        assert!(
            world
                .finish_conversation(
                    &mut store,
                    PersonId(1),
                    PersonId(101),
                    SocialTopic::FamilyPlans
                )
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
        let effect = world
            .finish_conversation(
                &mut store,
                PersonId(1),
                PersonId(2),
                SocialTopic::FamilyPlans,
            )
            .unwrap();
        assert!(effect.formed_family_intent);
        for (actor, other) in [(PersonId(1), PersonId(2)), (PersonId(2), PersonId(1))] {
            let own = personal(&store, actor).unwrap();
            assert!(own.companionship.contains(&other));
            assert!(own.family_intent.contains(&other));
        }
        assert_eq!(store.get::<u8>(game_ecs::StableId(1)), Some(&7));
        assert_eq!(
            world.view(&store, PersonId(1)).unwrap().family_intent,
            vec![PersonId(2)]
        );
        let snapshot = store.snapshot().unwrap();
        let mut schema = game_ecs::Schema::new();
        register_components(&mut schema).unwrap();
        schema.register::<u8>("foreign", |_| Ok(())).unwrap();
        let restored = game_ecs::Store::restore(schema, snapshot.clone()).unwrap();
        assert_eq!(restored.snapshot().unwrap(), snapshot);
        validate_local(&world, &restored, SimTime(0)).unwrap();
    }
    #[test]
    fn native_offers_bound_attention_and_reject_invalid_learning_before_mutation() {
        let (world, mut store) = fixture();
        for id in 2..=100 {
            world
                .remember_offer(
                    &mut store,
                    PersonId(1),
                    Offer {
                        teacher: PersonId(id),
                        topics: BTreeSet::from([InstructionTopic::Routes]),
                        area: AreaId(200),
                        heard_at: SimTime(id),
                    },
                    SimTime(100),
                )
                .unwrap();
        }
        let row = personal(&store, PersonId(1)).unwrap();
        assert_eq!(row.offers.len(), 64);
        assert_eq!(row.offers.keys().next(), Some(&PersonId(37)));
        let before = store.snapshot().unwrap();
        assert!(
            world
                .remember_offer(
                    &mut store,
                    PersonId(1),
                    Offer {
                        teacher: PersonId(2),
                        topics: BTreeSet::from([InstructionTopic::Skill(Skill::Invocation)]),
                        area: AreaId(200),
                        heard_at: SimTime(100)
                    },
                    SimTime(100)
                )
                .is_err()
        );
        assert_eq!(store.snapshot().unwrap(), before);
    }
    #[test]
    fn cancellation_guards_membership_and_writes_without_touching_other_components() {
        let (world, mut store) = fixture();
        let interaction = Interaction::Talk {
            first: PersonId(1),
            second: PersonId(2),
            topic: SocialTopic::Companionship,
            completes: SimTime(100),
        };
        world
            .start(&mut store, PersonId(1), interaction.clone(), SimTime(0))
            .unwrap();
        validate_local(&world, &store, SimTime(0)).unwrap();
        let mut batch = game_ecs::Batch::new();
        let guard =
            stage_cancel_interaction(&store, PersonId(1), &interaction, &mut batch).unwrap();
        guard.validate(&store, &batch).unwrap();
        batch.insert(
            game_ecs::StableId(3),
            Participant {
                leader: PersonId(1),
            },
        );
        assert!(guard.validate(&store, &batch).is_err());
        let mut batch = game_ecs::Batch::new();
        let guard =
            stage_cancel_interaction(&store, PersonId(1), &interaction, &mut batch).unwrap();
        batch.insert(game_ecs::StableId(1), 8u8);
        guard.validate(&store, &batch).unwrap();
        store.apply_batch(batch).unwrap();
        assert!(interactions(&store).next().is_none());
        assert!(participants(&store).next().is_none());
        assert_eq!(store.get::<u8>(game_ecs::StableId(1)), Some(&8));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temperament() -> Temperament {
        Temperament {
            discipline: 500,
            ambition: 500,
            sociability: 800,
            hedonism: 300,
            empathy: 800,
        }
    }
    fn own() -> Development {
        Development::baseline(temperament(), Aspiration::Comfort).unwrap()
    }
    fn input() -> ConversationInput {
        ConversationInput {
            temperament: temperament(),
            affection: 200,
            trust: 300,
            hunger: 10,
            meal_relief: 300,
            fatigue: 0,
            fatigue_limit: 300,
            recipient_adult: true,
            sender_adult: true,
            recipient_aspiration: Aspiration::Parenthood,
            chosen_companionship: true,
            visible_disruption: 0,
        }
    }
    #[test]
    fn aspirations_require_personal_experience_and_respond_to_different_upbringing() {
        let mut supported = own();
        assert_eq!(
            supported.preferred_aspiration(Aspiration::Comfort, 50),
            None
        );
        supported.observe(Aspiration::Parenthood, 40, "Someone chose to help me");
        assert_eq!(
            supported.preferred_aspiration(Aspiration::Comfort, 50),
            Some(Aspiration::Parenthood)
        );
        let mut practiced = own();
        for _ in 0..20 {
            practiced.observe(Aspiration::Independence, 30, "I practiced a craft");
        }
        assert_eq!(
            practiced.preferred_aspiration(Aspiration::Comfort, 50),
            Some(Aspiration::Independence)
        );
        assert_eq!(supported.experiences, 1);
        assert_eq!(practiced.experiences, 20);
        for _ in 0..100_000 {
            practiced.observe(Aspiration::Independence, 30, "Practice");
        }
        assert!(practiced.weights.values().all(|weight| *weight <= 10_000));
    }
    #[test]
    fn concrete_upbringing_evidence_preserves_distinct_recipient_and_caregiver_learning() {
        let mut child = own();
        let mut caregiver = own();
        let parent_before = child.weights[&Aspiration::Parenthood];
        let support_before = child.weights[&Aspiration::SupportOthers];
        child.observe_event(UpbringingEvidence::ReceivedCare);
        caregiver.observe_event(UpbringingEvidence::GaveCare);
        assert_eq!(child.weights[&Aspiration::Parenthood], parent_before + 20);
        assert_eq!(
            child.weights[&Aspiration::SupportOthers],
            support_before + 15
        );
        assert_eq!(child.experiences, 2);
        assert_eq!(caregiver.weights[&Aspiration::Parenthood], parent_before);
        assert_eq!(
            caregiver.weights[&Aspiration::SupportOthers],
            support_before + 20
        );
        let before = child.weights[&Aspiration::Independence];
        child.observe_event(UpbringingEvidence::Played(ChildhoodInterest::Craft));
        child.observe_event(UpbringingEvidence::ReceivedInstruction);
        assert_eq!(child.weights[&Aspiration::Independence], before + 30);
    }
    #[test]
    fn conversation_consent_uses_own_current_needs_aims_and_chosen_relationship() {
        assert!(conversation_willing(SocialTopic::FamilyPlans, input()));
        let mut declined = input();
        declined.recipient_aspiration = Aspiration::Comfort;
        assert!(!conversation_willing(SocialTopic::FamilyPlans, declined));
        assert!(conversation_willing(SocialTopic::Companionship, declined));
        declined = input();
        declined.chosen_companionship = false;
        assert!(!conversation_willing(SocialTopic::FamilyPlans, declined));
        declined = input();
        declined.recipient_adult = false;
        assert!(!conversation_willing(SocialTopic::FamilyPlans, declined));
        declined = input();
        declined.hunger = declined.meal_relief;
        assert!(!conversation_willing(SocialTopic::Companionship, declined));
        declined = input();
        declined.visible_disruption = 2_000;
        declined.trust = 0;
        assert!(!conversation_willing(SocialTopic::Companionship, declined));
        declined.trust = 1_000;
        assert!(conversation_willing(SocialTopic::Companionship, declined));
    }
}

#[cfg(test)]
mod prepared_start_tests {
    use super::*;
    #[test]
    fn prepared_start_rejects_changed_source_time_rules_and_foreign_overwrite() {
        let (mut world, mut store) = native_tests::fixture();
        let talk = Interaction::Talk {
            first: PersonId(1),
            second: PersonId(2),
            topic: SocialTopic::Companionship,
            completes: SimTime(100),
        };
        let plan = world
            .prepare_start(&store, PersonId(1), talk.clone(), SimTime(0))
            .unwrap();
        world.observe(&mut store, PersonId(2), UpbringingEvidence::GaveCare);
        assert!(
            plan.stage_into(&world, &store, SimTime(0), &mut game_ecs::Batch::new())
                .is_err()
        );
        let before = store.snapshot().unwrap();
        let plan = world
            .prepare_start(&store, PersonId(1), talk.clone(), SimTime(0))
            .unwrap();
        let mut batch = game_ecs::Batch::new();
        let guard = plan
            .stage_into(&world, &store, SimTime(0), &mut batch)
            .unwrap();
        assert!(guard.validate(&world, &store, SimTime(1), &batch).is_err());
        world.rules.review.0 += 1;
        assert!(guard.validate(&world, &store, SimTime(0), &batch).is_err());
        world.rules.review.0 -= 1;
        batch.insert(game_ecs::StableId(3), 9u8);
        guard.validate(&world, &store, SimTime(0), &batch).unwrap();
        batch.insert(
            game_ecs::StableId(2),
            Participant {
                leader: PersonId(3),
            },
        );
        assert!(guard.validate(&world, &store, SimTime(0), &batch).is_err());
        assert_eq!(store.snapshot().unwrap(), before);
        world
            .start(&mut store, PersonId(1), talk, SimTime(0))
            .unwrap();
        assert_eq!(participant(&store, PersonId(2)), Some(PersonId(1)));
    }
    #[test]
    fn readiness_owns_contact_liveness_practical_and_tool_progress_gates() {
        let interaction = Interaction::Instruction {
            teacher: PersonId(1),
            student: PersonId(2),
            topic: InstructionTopic::Skill(Skill::Craft),
            routes: BTreeMap::new(),
            mastery_cap: 500,
            tool: Some(game_core::InventoryId(10)),
            completes: SimTime(100),
        };
        let row = InteractionActorFacts {
            actor: PersonId(1),
            area: Some(game_core::AreaId(50)),
            alive: true,
            health: 100,
            in_combat: false,
            practical: PracticalFacts {
                combat_restricted: false,
                fatigue: 0,
                fatigue_limit: 100,
            },
        };
        let mut facts = [
            row,
            InteractionActorFacts {
                actor: PersonId(2),
                ..row
            },
        ];
        assert!(interaction_ready(&interaction, &facts, true));
        assert!(!interaction_ready(&interaction, &facts, false));
        facts[1].area = Some(game_core::AreaId(51));
        assert!(!interaction_ready(&interaction, &facts, true));
        facts[1].area = row.area;
        facts[1].practical.fatigue = 100;
        assert!(!interaction_ready(&interaction, &facts, true));
        facts[1].practical.fatigue = 0;
        facts[0].health = 0;
        assert!(!interaction_ready(&interaction, &facts, true));
    }
}

#[cfg(test)]
mod adult_stage_tests {
    use super::*;
    #[test]
    fn only_adult_and_elder_stages_admit_adult_social_choices() {
        use game_core::LifeStage;
        for (stage, expected) in [
            (None, false),
            (Some(LifeStage::Infant), false),
            (Some(LifeStage::Child), false),
            (Some(LifeStage::Adult), true),
            (Some(LifeStage::Elder), true),
        ] {
            assert_eq!(adult_stage(stage), expected);
        }
    }
}
