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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Aspiration {
    Comfort,
    Independence,
    SupportOthers,
    Parenthood,
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
pub struct World {
    pub rules: Rules,
    pub development: BTreeMap<PersonId, Development>,
    pub offers: BTreeMap<PersonId, BTreeMap<PersonId, Offer>>,
    pub family_intent: BTreeSet<(PersonId, PersonId)>,
    pub companionship: BTreeSet<(PersonId, PersonId)>,
    pub approaches: BTreeMap<PersonId, BTreeMap<PersonId, SimTime>>,
    pub learned: BTreeMap<PersonId, BTreeSet<(PersonId, InstructionTopic)>>,
    pub interactions: BTreeMap<PersonId, Interaction>,
    pub participants: BTreeMap<PersonId, PersonId>,
    pub last_review: SimTime,
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
    pub fn observe(&mut self, actor: PersonId, evidence: UpbringingEvidence) {
        if let Some(record) = self.development.get_mut(&actor) {
            record.observe_event(evidence);
        }
    }
    pub fn finish_conversation(
        &mut self,
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
    ) -> Result<ConversationEffect, game_core::Error> {
        if first == second
            || !self.development.contains_key(&first)
            || !self.development.contains_key(&second)
        {
            return Err(game_core::Error::Unavailable);
        }
        self.companionship.insert(pair(first, second));
        if topic == SocialTopic::FamilyPlans {
            self.family_intent.insert(pair(first, second));
        }
        Ok(ConversationEffect {
            first,
            second,
            affection_gain: 75,
            trust_gain: 25,
            formed_family_intent: topic == SocialTopic::FamilyPlans,
        })
    }
    pub fn remember_offer(
        &mut self,
        observer: PersonId,
        offer: Offer,
        now: SimTime,
    ) -> Result<(), game_core::Error> {
        if observer == offer.teacher
            || !self.development.contains_key(&observer)
            || !self.development.contains_key(&offer.teacher)
            || offer.heard_at > now
            || offer
                .topics
                .contains(&InstructionTopic::Skill(Skill::Invocation))
        {
            return Err(game_core::Error::Unavailable);
        }
        let known = self.offers.entry(observer).or_default();
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
}

/// Domain-owned constraints. Physical activity reservations, external reference
/// existence and material conservation are checked by the integrating host.
pub fn validate_local(world: &World, now: SimTime) -> Result<(), game_core::Error> {
    let fail = || game_core::Error::InvalidSave("inconsistent social development".into());
    if !world.rules.valid() || world.last_review > now {
        return Err(fail());
    }
    for record in world.development.values() {
        if record.weights.len() != 4
            || record.weights.values().any(|weight| *weight > 10_000)
            || record.explanation.len() > 2_000
        {
            return Err(fail());
        }
    }
    for relations in [&world.family_intent, &world.companionship] {
        if relations.iter().any(|(first, second)| {
            first >= second
                || !world.development.contains_key(first)
                || !world.development.contains_key(second)
        }) {
            return Err(fail());
        }
    }
    if world
        .family_intent
        .iter()
        .any(|pair| !world.companionship.contains(pair))
    {
        return Err(fail());
    }
    for (observer, offers) in &world.offers {
        if !world.development.contains_key(observer) || offers.len() > 64 {
            return Err(fail());
        }
        for (teacher, offer) in offers {
            if *teacher != offer.teacher
                || observer == teacher
                || !world.development.contains_key(teacher)
                || offer.heard_at > now
                || offer
                    .topics
                    .contains(&InstructionTopic::Skill(Skill::Invocation))
            {
                return Err(fail());
            }
        }
    }
    for (actor, approaches) in &world.approaches {
        if !world.development.contains_key(actor)
            || approaches.iter().any(|(other, at)| {
                actor == other || !world.development.contains_key(other) || *at > now
            })
        {
            return Err(fail());
        }
    }
    for (actor, learned) in &world.learned {
        if !world.development.contains_key(actor)
            || learned.iter().any(|(teacher, topic)| {
                actor == teacher
                    || !world.development.contains_key(teacher)
                    || *topic == InstructionTopic::Skill(Skill::Invocation)
            })
        {
            return Err(fail());
        }
    }
    let mut participants = BTreeMap::new();
    for (key, interaction) in &world.interactions {
        let actors = interaction.actors();
        if actors.len() == 2 && actors[0] == actors[1]
            || !actors.contains(key)
            || interaction.completes() <= now
        {
            return Err(fail());
        }
        for actor in actors {
            if !world.development.contains_key(&actor) || participants.insert(actor, *key).is_some()
            {
                return Err(fail());
            }
        }
        match interaction {
            Interaction::Instruction { topic, routes, .. } => {
                if *topic == InstructionTopic::Skill(Skill::Invocation)
                    || *topic != InstructionTopic::Routes && !routes.is_empty()
                    || routes
                        .values()
                        .any(|neighbors| neighbors.values().any(|duration| duration.0 == 0))
                {
                    return Err(fail());
                }
            }
            Interaction::Practice { skill, .. } if *skill != Skill::Craft => return Err(fail()),
            _ => {}
        }
    }
    if world.participants != participants {
        return Err(fail());
    }
    Ok(())
}

impl World {
    pub fn view(&self, actor: PersonId) -> Option<SocialView> {
        let world = self;
        let own = world.development.get(&actor)?;
        Some(SocialView {
            aspirations: own
                .weights
                .iter()
                .map(|(aim, weight)| (*aim, *weight))
                .collect(),
            experiences: own.experiences,
            family_intent: world
                .family_intent
                .iter()
                .filter_map(|(a, b)| {
                    if *a == actor {
                        Some(*b)
                    } else if *b == actor {
                        Some(*a)
                    } else {
                        None
                    }
                })
                .collect(),
            teaching_offers: world
                .offers
                .get(&actor)
                .into_iter()
                .flat_map(|offers| offers.values())
                .map(|offer| TeachingOfferView {
                    teacher: offer.teacher,
                    topics: offer.topics.iter().copied().collect(),
                    observed_at: offer.heard_at,
                })
                .collect(),
            explanation: own.explanation.clone(),
        })
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
    #[test]
    fn chosen_conversation_returns_relationship_effects_without_assets_or_family_law() {
        let first = PersonId(1);
        let second = PersonId(2);
        let mut world = World::default();
        world.development.insert(first, own());
        world.development.insert(second, own());
        let before = world.clone();
        assert_eq!(
            world.finish_conversation(first, PersonId(3), SocialTopic::FamilyPlans),
            Err(game_core::Error::Unavailable)
        );
        assert_eq!(world, before);
        let effect = world
            .finish_conversation(first, second, SocialTopic::Companionship)
            .unwrap();
        assert_eq!(effect.affection_gain, 75);
        assert_eq!(effect.trust_gain, 25);
        assert!(!effect.formed_family_intent);
        assert!(world.companionship.contains(&pair(first, second)));
        assert!(world.family_intent.is_empty());
        world
            .finish_conversation(first, second, SocialTopic::FamilyPlans)
            .unwrap();
        assert!(world.family_intent.contains(&pair(first, second)));
        validate_local(&world, SimTime(0)).unwrap();
    }
    #[test]
    fn remembered_offers_are_bounded_and_future_or_unauthorized_topics_are_inert() {
        let observer = PersonId(1);
        let mut world = World::default();
        world.development.insert(observer, own());
        for id in 2..=100 {
            world.development.insert(PersonId(id), own());
            world
                .remember_offer(
                    observer,
                    Offer {
                        teacher: PersonId(id),
                        topics: BTreeSet::from([InstructionTopic::Routes]),
                        area: AreaId(1),
                        heard_at: SimTime(id),
                    },
                    SimTime(id),
                )
                .unwrap();
        }
        assert_eq!(world.offers[&observer].len(), 64);
        assert!(!world.offers[&observer].contains_key(&PersonId(2)));
        let before = world.clone();
        assert!(
            world
                .remember_offer(
                    observer,
                    Offer {
                        teacher: PersonId(2),
                        topics: BTreeSet::from([InstructionTopic::Skill(Skill::Invocation)]),
                        area: AreaId(1),
                        heard_at: SimTime(100)
                    },
                    SimTime(100)
                )
                .is_err()
        );
        assert_eq!(world, before);
        assert!(
            world
                .remember_offer(
                    observer,
                    Offer {
                        teacher: PersonId(2),
                        topics: BTreeSet::new(),
                        area: AreaId(1),
                        heard_at: SimTime(101)
                    },
                    SimTime(100)
                )
                .is_err()
        );
        assert_eq!(world, before);
        validate_local(&world, SimTime(100)).unwrap();
    }
    #[test]
    fn personal_view_exposes_own_cached_experiences_without_query_side_effects() {
        let first = PersonId(1);
        let second = PersonId(2);
        let mut world = World::default();
        world.development.insert(first, own());
        world.development.insert(second, own());
        world.development.get_mut(&second).unwrap().observe(
            Aspiration::Parenthood,
            1_000,
            "Private experience",
        );
        let before = world.clone();
        assert_eq!(world.view(first).unwrap().experiences, 0);
        assert_eq!(world.view(second).unwrap().experiences, 1);
        assert!(world.view(PersonId(3)).is_none());
        for _ in 0..20 {
            let _ = world.view(first);
        }
        assert_eq!(world, before);
    }

    #[test]
    fn current_social_state_round_trips_and_validates() {
        let actor = PersonId(1);
        let mut world = World::default();
        world.development.insert(actor, own());
        let bytes = serde_json::to_vec(&world).unwrap();
        let restored: World = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(world, restored);
        validate_local(&restored, SimTime(0)).unwrap();
    }
}
