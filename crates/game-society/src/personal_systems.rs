//! Native personal workflows. Society owns interactions and personal experience;
//! peers receive their own conserved stock, mind, equipment, and action changes.
use crate::{
    personal::{self, PersonalReceipt, StartDecision},
    services::{ReadFamilyIntent, ReadInventoryAccess},
};
use game_cargo::{
    Inventory,
    facts::{ReadInventoryArea, ReadStockBudget, SetToolClaim, ToolClaim},
    receiving::CargoConsume,
};
pub use game_characters::services::ReadSocialCandidate;
use game_characters::{
    decisions::{Activity, Mind, Stocks},
    services::{
        ReadActivity, ReadConversationAvailability, ReadPlacement, ReadStage, ReceiveConversation,
    },
    social::{self, *},
};
use game_conflict::services::ReadCombatStatus;
use game_core::*;
use game_ecs::{Component, StableId};
use game_physiology::services::{ReadBody, ReadCareReceived};
use game_skills::{
    InteractionFacts, SocialFacts,
    systems::{ReadSkillView, ReadSocialLearningFacts},
};
use game_world::{
    OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot,
    actions::{AcquireAction, ReleaseAction},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[cfg(test)]
#[path = "personal_systems_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    SocialRefused {
        actor: PersonId,
        recipient: PersonId,
        reason: String,
    },
    Socialized {
        first: PersonId,
        second: PersonId,
        topic: SocialTopic,
    },
    InstructionCompleted {
        teacher: PersonId,
        student: PersonId,
        topic: InstructionTopic,
    },
    PracticeCompleted {
        actor: PersonId,
        skill: Skill,
    },
}
pub struct ReadFarmingAccess {
    pub actor: PersonId,
    pub area: AreaId,
}
impl Request for ReadFarmingAccess {
    type Response = bool;
}
pub struct ReadVisibleDisruption(pub PersonId);
impl Request for ReadVisibleDisruption {
    type Response = i64;
}
pub struct StartSocial {
    pub actor: PersonId,
    pub action: SocialAction,
}
impl Request for StartSocial {
    type Response = ();
}
pub struct CompleteSocial(pub PersonId);
impl Request for CompleteSocial {
    type Response = ();
}
pub struct CancelSocial(pub PersonId);
impl Request for CancelSocial {
    type Response = ();
}
pub struct ReceivePersonalReceipt(pub PersonalReceipt);
impl Request for ReceivePersonalReceipt {
    type Response = ();
}
/// Birth facts are joined by Society after the source owners admit the child.
pub struct ReceiveBirth {
    pub child: PersonId,
}
impl Request for ReceiveBirth {
    type Response = ();
}
pub struct ReadSocialView(pub PersonId);
impl Request for ReadSocialView {
    type Response = Option<SocialView>;
}
pub struct AvailableSocial(pub PersonId);
impl Request for AvailableSocial {
    type Response = Vec<SocialAction>;
}
struct ReadFacts {
    actor: PersonId,
    sender: PersonId,
}
impl Request for ReadFacts {
    type Response = personal::PersonFacts;
}
struct ReadReady(Interaction);
impl Request for ReadReady {
    type Response = bool;
}
pub enum Input {
    Initialize(Rules),
    Start(StartSocial),
    Complete(PersonId),
    Review,
    ValidateSave,
}
pub struct Systems<C> {
    bind: fn(SocialAction) -> C,
}
impl<C> Systems<C> {
    pub fn new(bind: fn(SocialAction) -> C) -> Self {
        Self { bind }
    }
}
impl<C: Component, E: From<Event> + 'static> OwnerSystem<E> for Systems<C> {
    type Runtime = Option<World>;
    type Input = Input;
    type Output = ();
    fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, E>, input: Input) -> Result<(), Error> {
        match input {
            Input::Initialize(rules) => {
                if tx.runtime().is_some()
                    || !tx.iter::<InteractionRow>().is_empty()
                    || !tx.iter::<Participant>().is_empty()
                {
                    return Err(Error::Unavailable);
                }
                if !rules.valid() {
                    return Err(Error::InvalidConfiguration("invalid social rules".into()));
                }
                let now = tx.now();
                for (id, mind) in tx.iter::<Mind<C>>() {
                    if tx.request(ReadBody(PersonId(id.0)))?.is_none() {
                        return Err(Error::Unavailable);
                    }
                    let mut development = Development::baseline(
                        Temperament::from_personality(&mind.personality),
                        mind.dream,
                    )?;
                    development.next_choice = now;
                    tx.write(
                        id,
                        Personal {
                            development,
                            ..Personal::default()
                        },
                    )?;
                }
                *tx.runtime_mut() = Some(World {
                    rules,
                    last_review: now,
                });
                Ok(())
            }
            Input::Start(input) => start(self, tx, input),
            Input::Complete(actor) => complete(self, tx, CompleteSocial(actor)),
            Input::Review => review(self, tx),
            Input::ValidateSave => validate(self, tx),
        }
    }
}
pub fn register_systems<S: 'static, C: Component, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<World>>,
    bind: fn(SocialAction) -> C,
) -> Result<(), Error> {
    registry.register(Systems::new(bind), slot)?;
    registry.owns::<Systems<C>, Personal>()?;
    registry.owns::<Systems<C>, Participant>()?;
    registry.owns::<Systems<C>, InteractionRow>()?;
    registry.register_request::<Systems<C>, StartSocial>(start::<C, E>)?;
    registry.register_request::<Systems<C>, CompleteSocial>(complete::<C, E>)?;
    registry.register_request::<Systems<C>, CancelSocial>(cancel::<C, E>)?;
    registry.register_request::<Systems<C>, ReceivePersonalReceipt>(receive::<C, E>)?;
    registry.register_request::<Systems<C>, ReceiveBirth>(receive_birth::<C, E>)?;
    registry.register_request::<Systems<C>, ReleaseSocialLease>(release::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadFacts>(facts::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadReady>(ready::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadNativeChoice>(native_choice::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadFamilyIntent>(family_intent::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadSocialView>(view::<C, E>)?;
    registry.register_read_request::<Systems<C>, AvailableSocial>(available::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadSocialCandidate<C>>(candidate::<C, E>)?;
    registry.register_read_request::<Systems<C>, ReadSocialLearningFacts>(learning::<C, E>)
}
fn family_intent<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: ReadFamilyIntent,
) -> Result<bool, Error> {
    if tx.runtime().is_none() {
        return Ok(true);
    }
    Ok(tx
        .read::<Personal>(StableId(q.actor.0))
        .is_some_and(|row| row.family_intent.contains(&q.partner)))
}
fn view<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: ReadSocialView,
) -> Result<Option<SocialView>, Error> {
    if tx.runtime().is_none() {
        return Ok(None);
    }
    Ok(tx.read::<Personal>(StableId(q.0.0)).map(|row| SocialView {
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
    }))
}
fn facts<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: ReadFacts,
) -> Result<personal::PersonFacts, Error> {
    let world = tx.runtime().clone().ok_or(Error::Unavailable)?;
    let mind = tx
        .read::<Mind<C>>(StableId(q.actor.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let body = tx.request(ReadBody(q.actor))?.ok_or(Error::Unavailable)?;
    let stage = tx.request(ReadStage(q.actor))?;
    let sender_stage = tx.request(ReadStage(q.sender))?;
    let combat = tx.request(ReadCombatStatus(q.actor))?;
    let area = tx
        .request(ReadPlacement(q.actor))?
        .and_then(|row| match row.location {
            Location::AtArea(area) => Some(area),
            _ => None,
        });
    let activity = tx.request(ReadActivity(q.actor))?;
    let practical = can_practice(PracticalFacts {
        combat_restricted: combat.restricted,
        fatigue: body.vitals.fatigue,
        fatigue_limit: body.rules.fatigue_limit,
    });
    let speech = can_speak(SpeechFacts {
        stationary: area.is_some(),
        independent: stage.independent,
        in_combat: combat.in_combat,
        alive: body.vitals.alive,
        health: body.vitals.health,
        idle_or_rest: matches!(activity, Activity::Idle | Activity::Rest),
        receiving_care: tx.request(ReadCareReceived(q.actor))?,
        conversation_available: tx.request(ReadConversationAvailability(q.actor))?,
    });
    let stocks = tx
        .read::<Stocks>(StableId(q.actor.0))
        .cloned()
        .unwrap_or_default();
    let mut stock_facts = Vec::new();
    for known in stocks.known.values() {
        let Some(stock) = tx.read::<Inventory>(StableId(known.inventory.0)).cloned() else {
            continue;
        };
        let accessible = tx.request(ReadInventoryAccess {
            actor: q.actor,
            inventory: stock.id,
        })?;
        let tool_available = tx
            .request(ReadStockBudget {
                actor: q.actor,
                inventory: stock.id,
                additional_reservations: Vec::new(),
            })?
            .tool_allocated;
        let stock_area = tx.request(ReadInventoryArea {
            inventory: stock.id,
        })?;
        stock_facts.push(personal::StockFacts {
            inventory: stock.id,
            known_good: known.good,
            known_quantity: known.quantity,
            known_accessible: known.accessible,
            actual_good: stock.good,
            actual_quantity: stock.quantity,
            area: stock_area,
            accessible,
            tool_available,
        });
    }
    let tool = personal::select_tool(stock_facts.iter().map(copy_stock));
    let materials = [Good::Timber, Good::Seed, Good::Herb]
        .into_iter()
        .filter_map(|good| {
            personal::select_material(
                stock_facts.iter().map(copy_stock),
                area,
                good,
                world.rules.practice_material,
            )
            .map(|id| (good, id))
        })
        .collect();
    let farming_access = match area {
        Some(area) => tx.request(ReadFarmingAccess {
            actor: q.actor,
            area,
        })?,
        None => false,
    };
    let practice = tx
        .request(ReadSkillView(q.actor))?
        .into_iter()
        .map(|row| (row.skill, row.practice_ms))
        .collect();
    let relation = mind.relationships.get(&q.sender);
    let affection = relation.map_or(0, |row| row.affection);
    let trust = relation.map_or(0, |row| row.trust);
    let visible_disruption = tx.request(ReadVisibleDisruption(q.sender))?;
    Ok(personal::PersonFacts {
        actor: q.actor,
        area,
        speech,
        adult: adult_stage(stage.stage),
        practical,
        tool,
        materials,
        farming_access,
        routes: mind.routes,
        practice,
        empathy: mind.personality.empathy,
        patience: mind.personality.patience,
        affection,
        trust,
        conversation: Some(ConversationInput {
            temperament: Temperament::from_personality(&mind.personality),
            affection,
            trust,
            hunger: body.vitals.hunger,
            meal_relief: body.rules.meal_relief,
            fatigue: body.vitals.fatigue,
            fatigue_limit: body.rules.fatigue_limit,
            recipient_adult: adult_stage(stage.stage),
            sender_adult: adult_stage(sender_stage.stage),
            recipient_aspiration: mind.dream,
            chosen_companionship: false,
            visible_disruption,
        }),
    })
}
fn copy_stock(row: &personal::StockFacts) -> personal::StockFacts {
    personal::StockFacts {
        inventory: row.inventory,
        known_good: row.known_good,
        known_quantity: row.known_quantity,
        known_accessible: row.known_accessible,
        actual_good: row.actual_good,
        actual_quantity: row.actual_quantity,
        area: row.area,
        accessible: row.accessible,
        tool_available: row.tool_available,
    }
}
fn ready<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: ReadReady,
) -> Result<bool, Error> {
    let mut facts = Vec::new();
    for actor in q.0.actors() {
        let Some(body) = tx.request(ReadBody(actor))? else {
            return Ok(false);
        };
        let combat = tx.request(ReadCombatStatus(actor))?;
        let area = tx
            .request(ReadPlacement(actor))?
            .and_then(|row| match row.location {
                Location::AtArea(area) => Some(area),
                _ => None,
            });
        let lease = tx.request(game_world::actions::ReadAction {
            actor: StableId(actor.0),
        })?;
        if lease
            .is_none_or(|lease| lease.owner != "social" || lease.completes != Some(q.0.completes()))
        {
            return Ok(false);
        }
        facts.push(InteractionActorFacts {
            actor,
            area,
            alive: body.vitals.alive,
            health: body.vitals.health,
            in_combat: combat.in_combat,
            practical: PracticalFacts {
                combat_restricted: combat.restricted,
                fatigue: body.vitals.fatigue,
                fatigue_limit: body.rules.fatigue_limit,
            },
        });
    }
    let equipped = match interaction_tool(&q.0) {
        Some((actor, inventory)) => {
            tx.request(ReadStockBudget {
                actor,
                inventory,
                additional_reservations: Vec::new(),
            })?
            .tool_allocated
        }
        None => true,
    };
    Ok(interaction_ready(&q.0, &facts, equipped))
}
fn start<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    q: StartSocial,
) -> Result<(), Error> {
    let world = tx.runtime().clone().ok_or(Error::Unavailable)?;
    if tx
        .read::<game_characters::Person>(StableId(q.actor.0))
        .is_none()
    {
        return Err(Error::Unavailable);
    }
    let facts = tx.request(ReadFacts {
        actor: q.actor,
        sender: q.actor,
    })?;
    let partner = match q.action {
        SocialAction::Talk { partner, .. } => Some(partner),
        SocialAction::Learn { teacher, .. } => Some(teacher),
        _ => None,
    };
    if partner.is_some_and(|actor| {
        tx.read::<game_characters::Person>(StableId(actor.0))
            .is_none()
    }) {
        return Err(Error::Unavailable);
    }
    let partner = partner
        .map(|actor| {
            tx.request(ReadFacts {
                actor,
                sender: q.actor,
            })
        })
        .transpose()?;
    let own = tx.read::<Personal>(StableId(q.actor.0)).cloned();
    match personal::prepare_start_personal(
        &world,
        own.as_ref(),
        &facts,
        partner.as_ref(),
        q.action,
        tx.now(),
    )? {
        StartDecision::Refused { recipient, reason } => {
            let mut row = own.ok_or(Error::Unavailable)?;
            row.approaches.insert(recipient, tx.now());
            row.development.explanation = reason.into();
            tx.write(StableId(q.actor.0), row)?;
            tx.emit(
                Event::SocialRefused {
                    actor: q.actor,
                    recipient,
                    reason: reason.into(),
                }
                .into(),
            )?;
        }
        StartDecision::Started {
            interaction,
            materials,
            next_choice,
        } => {
            for actor in interaction.actors() {
                cancel(owner, tx, CancelSocial(actor))?;
            }
            if let Some((_, inventory, quantity)) = materials {
                tx.request(CargoConsume {
                    inventory,
                    quantity,
                })?;
            }
            for actor in interaction.actors() {
                let mut row = tx
                    .read::<Personal>(StableId(actor.0))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                row.development.next_choice = next_choice;
                row.development.explanation =
                    "I chose to spend time in this conversation or practice".into();
                if actor == q.actor
                    && let Interaction::Talk { second, .. } = interaction
                {
                    row.approaches.insert(second, tx.now());
                }
                tx.request(AcquireAction {
                    actor: StableId(actor.0),
                    owner: "social".into(),
                    completes: Some(interaction.completes()),
                })?;
                tx.write(StableId(actor.0), row)?;
                tx.write(StableId(actor.0), Participant { leader: q.actor })?;
                if interaction_is_practical(&interaction) {
                    tx.request(game_physiology::systems::SetPhysicalDemand {
                        actor,
                        source: "social".into(),
                        demand: Some(game_physiology::systems::Demand {
                            exerting: true,
                            resting: false,
                            interrupt_at_exhaustion: true,
                        }),
                    })?;
                }
            }
            if let Some((actor, tool)) = interaction_tool(&interaction) {
                tx.request(SetToolClaim {
                    actor,
                    claim: Some(ToolClaim { tool }),
                })?;
            }
            tx.write(StableId(q.actor.0), InteractionRow { interaction })?;
        }
    }
    Ok(())
}
fn cancel<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    q: CancelSocial,
) -> Result<(), Error> {
    let Some(participant) = tx.read::<Participant>(StableId(q.0.0)).copied() else {
        return Ok(());
    };
    let interaction = tx
        .read::<InteractionRow>(StableId(participant.leader.0))
        .cloned()
        .ok_or(Error::Unavailable)?
        .interaction;
    for actor in interaction.actors() {
        if tx
            .read::<Participant>(StableId(actor.0))
            .is_none_or(|row| row.leader != participant.leader)
        {
            return Err(Error::Unavailable);
        }
        tx.remove::<Participant>(StableId(actor.0))?;
        tx.defer(ReleaseSocialLease {
            actor,
            practical: interaction_is_practical(&interaction),
        })?;
    }
    if let Some((actor, _)) = interaction_tool(&interaction) {
        tx.request(SetToolClaim { actor, claim: None })?;
    }
    tx.remove::<InteractionRow>(StableId(participant.leader.0))
}
// Deferred release checks absence of a replacement interaction before releasing.
// A replaced social action must keep its newly acquired lease.
struct ReleaseSocialLease {
    actor: PersonId,
    practical: bool,
}
impl Request for ReleaseSocialLease {
    type Response = ();
}
fn release<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    q: ReleaseSocialLease,
) -> Result<(), Error> {
    if tx.read::<Participant>(StableId(q.actor.0)).is_none() {
        tx.request(ReleaseAction {
            actor: StableId(q.actor.0),
            owner: "social".into(),
        })?;
        if q.practical {
            tx.request(game_physiology::systems::SetPhysicalDemand {
                actor: q.actor,
                source: "social".into(),
                demand: None,
            })?;
        }
    }
    Ok(())
}
pub fn cancel_action<C: Component, E: From<Event> + 'static>(
    tx: &mut OwnerTxn<'_, '_, game_world::actions::Actions<E>, E>,
    actor: StableId,
) -> Result<(), Error> {
    tx.request(CancelSocial(PersonId(actor.0)))
}
fn complete<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    q: CompleteSocial,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    let Some(participant) = tx.read::<Participant>(StableId(q.0.0)).copied() else {
        return Ok(());
    };
    let interaction = tx
        .read::<InteractionRow>(StableId(participant.leader.0))
        .cloned()
        .ok_or(Error::Unavailable)?
        .interaction;
    if interaction.completes() != tx.now() {
        return Ok(());
    }
    if !tx.request(ReadReady(interaction.clone()))? {
        return cancel(owner, tx, CancelSocial(q.0));
    }
    let (event, receipt) = match interaction {
        Interaction::Talk {
            first,
            second,
            topic,
            ..
        } => {
            for (actor, other) in [(first, second), (second, first)] {
                let row = tx
                    .read::<Personal>(StableId(actor.0))
                    .cloned()
                    .ok_or(Error::Unavailable)?;
                tx.write(
                    StableId(actor.0),
                    conversation_personal_after(&row, other, topic),
                )?;
            }
            tx.request(ReceiveConversation(ConversationEffect {
                first,
                second,
                affection_gain: 75,
                trust_gain: 25,
                formed_family_intent: topic == SocialTopic::FamilyPlans,
            }))?;
            (
                Event::Socialized {
                    first,
                    second,
                    topic,
                },
                PersonalReceipt::Socialized {
                    first,
                    second,
                    topic,
                },
            )
        }
        Interaction::Instruction {
            teacher,
            student,
            topic,
            routes,
            ..
        } => {
            let mut row = tx
                .read::<Personal>(StableId(student.0))
                .cloned()
                .ok_or(Error::Unavailable)?;
            row.learned.insert((teacher, topic));
            tx.write(StableId(student.0), row)?;
            if topic == InstructionTopic::Routes {
                tx.request(game_characters::services::ReceiveRoutes {
                    actor: student,
                    routes,
                })?;
            }
            (
                Event::InstructionCompleted {
                    teacher,
                    student,
                    topic,
                },
                PersonalReceipt::Instructed { teacher, student },
            )
        }
        Interaction::Practice { actor, skill, .. } => (
            Event::PracticeCompleted { actor, skill },
            PersonalReceipt::Produced { actor },
        ),
    };
    receive(owner, tx, ReceivePersonalReceipt(receipt))?;
    cancel(owner, tx, CancelSocial(q.0))?;
    tx.emit(event.into())?;
    Ok(())
}
fn observe<C: Component, E: From<Event> + 'static>(
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    actor: PersonId,
    evidence: UpbringingEvidence,
) -> Result<(), Error> {
    if let Some(mut row) = tx.read::<Personal>(StableId(actor.0)).cloned() {
        row.development.observe_event(evidence);
        tx.write(StableId(actor.0), row)?;
    }
    Ok(())
}
fn receive<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    q: ReceivePersonalReceipt,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    match q.0 {
        PersonalReceipt::Born {
            child,
            temperament,
            witnesses,
        } => {
            if child.0 == 0 || tx.read::<Personal>(StableId(child.0)).is_some() {
                return Err(Error::Unavailable);
            }
            let mut development = Development::baseline(temperament, Aspiration::Comfort)?;
            development.next_choice = tx.now();
            tx.write(
                StableId(child.0),
                Personal {
                    development,
                    ..Personal::default()
                },
            )?;
            for witness in witnesses
                .into_iter()
                .filter(|w| w.actor != child && w.same_location && w.alive)
            {
                observe(tx, witness.actor, UpbringingEvidence::WitnessedBirth)?;
            }
        }
        PersonalReceipt::Cared { caregiver, child } => {
            observe(tx, child, UpbringingEvidence::ReceivedCare)?;
            observe(tx, caregiver, UpbringingEvidence::GaveCare)?;
        }
        PersonalReceipt::Comforted { caregiver, child } => {
            observe(tx, child, UpbringingEvidence::ReceivedComfort)?;
            observe(tx, caregiver, UpbringingEvidence::GaveComfort)?;
        }
        PersonalReceipt::Played { actor, interest } => {
            observe(tx, actor, UpbringingEvidence::Played(interest))?
        }
        PersonalReceipt::Produced { actor } => observe(tx, actor, UpbringingEvidence::Produced)?,
        PersonalReceipt::Instructed { teacher, student } => {
            observe(tx, student, UpbringingEvidence::ReceivedInstruction)?;
            observe(tx, teacher, UpbringingEvidence::GaveInstruction)?;
        }
        PersonalReceipt::Socialized {
            first,
            second,
            topic,
        } => {
            observe(tx, first, UpbringingEvidence::Conversation(topic))?;
            observe(tx, second, UpbringingEvidence::Conversation(topic))?;
        }
    }
    Ok(())
}
fn learning<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    _: ReadSocialLearningFacts,
) -> Result<Vec<SocialFacts>, Error> {
    if tx.runtime().is_none() {
        return Ok(Vec::new());
    }
    let mut facts = Vec::new();
    for (_, row) in tx.iter::<InteractionRow>() {
        let can_progress = tx.request(ReadReady(row.interaction.clone()))?;
        let completes = row.interaction.completes();
        let interaction = match row.interaction {
            Interaction::Talk { first, second, .. } => InteractionFacts::Talk { first, second },
            Interaction::Instruction {
                teacher,
                student,
                topic,
                mastery_cap,
                ..
            } => InteractionFacts::Instruction {
                teacher,
                student,
                skill: match topic {
                    InstructionTopic::Skill(skill) => Some(skill),
                    _ => None,
                },
                mastery_cap,
            },
            Interaction::Practice { actor, skill, .. } => {
                InteractionFacts::Practice { actor, skill }
            }
        };
        facts.push(SocialFacts {
            interaction,
            can_progress,
            completes,
        });
    }
    Ok(facts)
}
fn candidate_action<C: Component, E: From<Event> + 'static>(
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    actor: PersonId,
) -> Result<Option<SocialAction>, Error> {
    let Some(world) = tx.runtime().clone() else {
        return Ok(None);
    };
    let Some(own) = tx.read::<Personal>(StableId(actor.0)).cloned() else {
        return Ok(None);
    };
    let Some(mind) = tx.read::<Mind<C>>(StableId(actor.0)).cloned() else {
        return Ok(None);
    };
    let Some(person) = tx
        .read::<game_characters::Person>(StableId(actor.0))
        .cloned()
    else {
        return Ok(None);
    };
    if person.role == Role::Guardian
        || person.role == Role::Ruler
        || own.development.next_choice > tx.now()
    {
        return Ok(None);
    }
    let own_facts = tx.request(ReadFacts {
        actor,
        sender: actor,
    })?;
    if !own_facts.speech {
        return Ok(None);
    }
    let mut peers = Vec::new();
    let mut teachers = BTreeSet::new();
    let mut companionship_partner = None;
    for (_, peer) in tx.iter::<game_characters::Person>() {
        if peer.id == actor
            || matches!(peer.role, Role::Guardian | Role::Ruler)
            || tx.read::<Personal>(StableId(peer.id.0)).is_none()
        {
            continue;
        }
        let at_area = tx.request(ReadPlacement(peer.id))?.is_some_and(|place| matches!(place.location, Location::AtArea(area) if Some(area) == own_facts.area));
        if !at_area {
            continue;
        }
        let peer_facts = tx.request(ReadFacts {
            actor: peer.id,
            sender: actor,
        })?;
        if !peer_facts.speech {
            continue;
        }
        companionship_partner.get_or_insert(peer.id);
        if peer_facts.adult {
            peers.push(peer.id);
        }
        if own.offers.contains_key(&peer.id) {
            teachers.insert(peer.id);
        }
    }
    peers.sort();
    let offset = if peers.is_empty() {
        0
    } else {
        (actor.0 % peers.len() as u64) as usize
    };
    let family_peers = peers
        .iter()
        .cycle()
        .skip(offset)
        .take(peers.len().min(64))
        .copied()
        .collect();
    let own_levels = tx
        .request(ReadSkillView(actor))?
        .into_iter()
        .map(|row| (row.skill, row.level))
        .collect();
    let choice = ChoiceFacts {
        actor,
        now: tx.now(),
        autonomous: true,
        can_speak: own_facts.speech,
        adult: own_facts.adult,
        family_peers,
        available_teachers: teachers,
        own_levels,
        can_practice_craft: own_facts.tool.is_some()
            && own_facts.materials.contains_key(&Good::Timber),
        companionship_partner,
    };
    let Some(action) = world.choose_from_personal(&own, &mind, &choice) else {
        return Ok(None);
    };
    let partner = match action {
        SocialAction::Talk { partner, .. } => Some(partner),
        SocialAction::Learn { teacher, .. } => Some(teacher),
        _ => None,
    };
    let partner = partner
        .map(|actor| {
            tx.request(ReadFacts {
                actor,
                sender: choice.actor,
            })
        })
        .transpose()?;
    match personal::prepare_start_personal(
        &world,
        Some(&own),
        &own_facts,
        partner.as_ref(),
        action.clone(),
        tx.now(),
    ) {
        Ok(_) => Ok(Some(action)),
        Err(Error::Unavailable) => Ok(None),
        Err(error) => Err(error),
    }
}
fn candidate<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: ReadSocialCandidate<C>,
) -> Result<Option<C>, Error> {
    Ok(candidate_action(tx, q.0)?.map(owner.bind))
}
fn review<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
) -> Result<(), Error> {
    let Some(world) = tx.runtime().clone() else {
        return Ok(());
    };
    let now = tx.now();
    if !personal::review_due(&world.rules, world.last_review, now)? {
        return Ok(());
    }
    let mut roster = Vec::new();
    for (id, row) in tx.iter::<Personal>() {
        let actor = PersonId(id.0);
        tx.request(game_characters::services::DevelopAspiration {
            actor,
            development: row.development.clone(),
            margin: world.rules.aspiration_margin,
        })?;
        let Some(mind) = tx.read::<Mind<C>>(id).cloned() else {
            continue;
        };
        let Some(person) = tx.read::<game_characters::Person>(id).cloned() else {
            continue;
        };
        let fact = tx.request(ReadFacts {
            actor,
            sender: actor,
        })?;
        let topics: BTreeSet<_> = (!mind.routes.is_empty())
            .then_some(InstructionTopic::Routes)
            .into_iter()
            .chain(
                fact.practice
                    .iter()
                    .filter(|(skill, practice)| {
                        **practice > 0 && !matches!(skill, Skill::Invocation | Skill::Combat)
                    })
                    .map(|(skill, _)| InstructionTopic::Skill(*skill)),
            )
            .collect();
        roster.push((
            actor,
            fact.area,
            fact.speech,
            person.role != Role::Guardian,
            mind.offers_teaching(),
            topics,
        ));
        tx.write(id, row)?;
    }
    let mut declarations = BTreeMap::<AreaId, Vec<Offer>>::new();
    for (actor, area, speech, not_guardian, teaches, topics) in &roster {
        if *speech
            && *not_guardian
            && *teaches
            && !topics.is_empty()
            && let Some(area) = area
        {
            declarations.entry(*area).or_default().push(Offer {
                teacher: *actor,
                topics: topics.clone(),
                area: *area,
                heard_at: now,
            });
        }
    }
    for offers in declarations.values_mut() {
        offers.sort_by_key(|offer| offer.teacher);
    }
    for (actor, area, speech, not_guardian, _, _) in &roster {
        if !speech || !not_guardian {
            continue;
        }
        let Some(offers) = area.and_then(|area| declarations.get(&area)) else {
            continue;
        };
        let offset =
            (actor.0.wrapping_add(now.0 / world.rules.review.0) % offers.len() as u64) as usize;
        let mut row = tx
            .read::<Personal>(StableId(actor.0))
            .cloned()
            .ok_or(Error::Unavailable)?;
        for offer in offers
            .iter()
            .cycle()
            .skip(offset)
            .take(offers.len().min(64))
            .filter(|offer| offer.teacher != *actor)
        {
            row.offers.insert(offer.teacher, offer.clone());
        }
        while row.offers.len() > 64 {
            let oldest = *row
                .offers
                .iter()
                .min_by_key(|(actor, offer)| (offer.heard_at, **actor))
                .ok_or(Error::Unavailable)?
                .0;
            row.offers.remove(&oldest);
        }
        tx.write(StableId(actor.0), row)?;
    }
    tx.runtime_mut()
        .as_mut()
        .ok_or(Error::Unavailable)?
        .last_review = now;
    for (actor, _, speech, not_guardian, _, _) in roster {
        if !speech || !not_guardian {
            continue;
        }
        if tx
            .read::<game_characters::Person>(StableId(actor.0))
            .is_none_or(|person| person.role == Role::Ruler)
            || tx
                .read::<Personal>(StableId(actor.0))
                .is_none_or(|row| row.development.next_choice > now)
        {
            continue;
        }
        let action = tx.request(ReadNativeChoice(actor))?;
        if let Some(action) = action {
            start(owner, tx, StartSocial { actor, action })?;
        }
        if let Some(mut row) = tx.read::<Personal>(StableId(actor.0)).cloned() {
            row.development.next_choice = SimTime(
                now.0
                    .checked_add(world.rules.review.0)
                    .ok_or(Error::Overflow)?,
            );
            tx.write(StableId(actor.0), row)?;
        }
    }
    Ok(())
}
struct ReadNativeChoice(PersonId);
impl Request for ReadNativeChoice {
    type Response = Option<SocialAction>;
}
fn native_choice<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: ReadNativeChoice,
) -> Result<Option<SocialAction>, Error> {
    candidate_action(tx, q.0)
}
fn validate<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
) -> Result<(), Error> {
    let fail = || Error::InvalidSave("inconsistent native personal social state".into());
    let Some(world) = tx.runtime().clone() else {
        return if tx.iter::<Personal>().is_empty()
            && tx.iter::<InteractionRow>().is_empty()
            && tx.iter::<Participant>().is_empty()
        {
            Ok(())
        } else {
            Err(fail())
        };
    };
    if !world.rules.valid() || world.last_review > tx.now() {
        return Err(fail());
    }
    for (id, row) in tx.iter::<Personal>() {
        let actor = PersonId(id.0);
        if tx.read::<game_characters::Person>(id).is_none()
            || row.development.weights.len() != 4
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
            if *other == actor
                || tx
                    .read::<Personal>(StableId(other.0))
                    .is_none_or(|peer| !peer.companionship.contains(&actor))
            {
                return Err(fail());
            }
        }
        for other in &row.family_intent {
            if !row.companionship.contains(other)
                || tx
                    .read::<Personal>(StableId(other.0))
                    .is_none_or(|peer| !peer.family_intent.contains(&actor))
            {
                return Err(fail());
            }
        }
        for (teacher, offer) in &row.offers {
            if *teacher != offer.teacher
                || *teacher == actor
                || tx.read::<Personal>(StableId(teacher.0)).is_none()
                || offer.heard_at > tx.now()
                || offer
                    .topics
                    .contains(&InstructionTopic::Skill(Skill::Invocation))
            {
                return Err(fail());
            }
        }
        for (other, at) in &row.approaches {
            if *other == actor || *at > tx.now() || tx.read::<Personal>(StableId(other.0)).is_none()
            {
                return Err(fail());
            }
        }
        for (teacher, topic) in &row.learned {
            if *teacher == actor
                || *topic == InstructionTopic::Skill(Skill::Invocation)
                || tx.read::<Personal>(StableId(teacher.0)).is_none()
            {
                return Err(fail());
            }
        }
    }
    let mut expected = BTreeMap::new();
    for (leader, row) in tx.iter::<InteractionRow>() {
        let actors = row.interaction.actors();
        if !actors.contains(&PersonId(leader.0))
            || row.interaction.completes() <= tx.now()
            || !tx.request(ReadReady(row.interaction.clone()))?
        {
            return Err(fail());
        }
        for actor in actors {
            if tx.read::<Personal>(StableId(actor.0)).is_none()
                || expected
                    .insert(
                        StableId(actor.0),
                        Participant {
                            leader: PersonId(leader.0),
                        },
                    )
                    .is_some()
            {
                return Err(fail());
            }
        }
    }
    if tx
        .iter::<Participant>()
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        != expected
    {
        return Err(fail());
    }
    Ok(())
}
pub fn boundary<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<Option<SimTime>, Error> {
    let Some(world) = tx.runtime().clone() else {
        return Ok(None);
    };
    let review = SimTime(
        world
            .last_review
            .0
            .checked_add(world.rules.review.0)
            .ok_or(Error::Overflow)?,
    );
    let next = tx
        .iter::<InteractionRow>()
        .into_iter()
        .map(|(_, row)| row.interaction.completes())
        .chain([review])
        .filter(|at| *at > span.from && *at <= span.to)
        .min();
    Ok(next)
}
pub fn close<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    _: game_world::scheduling::ClockSpan,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    for (id, row) in tx.iter::<InteractionRow>() {
        if row.interaction.completes() <= tx.now() {
            complete(owner, tx, CompleteSocial(PersonId(id.0)))?;
        } else if !tx.request(ReadReady(row.interaction))? {
            cancel(owner, tx, CancelSocial(PersonId(id.0)))?;
        }
    }
    review(owner, tx)
}
/// Register closing after Skills so the final interval earns its actual credit.
pub fn register_schedule<S: 'static, C: Component, E: From<Event> + 'static>(
    registry: &mut Registry<S, E>,
    scheduler: &mut game_world::scheduling::Scheduler<E>,
    order: game_world::scheduling::Order,
) -> Result<(), Error> {
    scheduler.register_owner_phased::<Systems<C>, S>(
        registry,
        order,
        Some(boundary::<C, E>),
        None,
        None,
        Some(close::<C, E>),
    )
}
fn receive_birth<C: Component, E: From<Event> + 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    q: ReceiveBirth,
) -> Result<(), Error> {
    if tx.runtime().is_none() {
        return Ok(());
    }
    let mind = tx
        .read::<Mind<C>>(StableId(q.child.0))
        .cloned()
        .ok_or(Error::Unavailable)?;
    let placement = tx
        .request(ReadPlacement(q.child))?
        .ok_or(Error::Unavailable)?;
    let mut witnesses = Vec::new();
    for (_, person) in tx.iter::<game_characters::Person>() {
        if person.id == q.child {
            continue;
        }
        let same_location = tx
            .request(ReadPlacement(person.id))?
            .is_some_and(|other| other.location == placement.location);
        if same_location {
            witnesses.push(personal::BirthWitness {
                actor: person.id,
                same_location,
                alive: tx
                    .request(ReadBody(person.id))?
                    .is_some_and(|body| body.vitals.alive),
            });
        }
    }
    receive(
        owner,
        tx,
        ReceivePersonalReceipt(PersonalReceipt::Born {
            child: q.child,
            temperament: Temperament::from_personality(&mind.personality),
            witnesses,
        }),
    )
}
fn available<C: Component, E: From<Event> + 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    q: AvailableSocial,
) -> Result<Vec<SocialAction>, Error> {
    if tx.runtime().is_none() || tx.read::<Mind<C>>(StableId(q.0.0)).is_none() {
        return Ok(Vec::new());
    }
    let own = tx.read::<Personal>(StableId(q.0.0)).cloned();
    if own.is_none() {
        return Ok(Vec::new());
    }
    let actor = tx.request(ReadFacts {
        actor: q.0,
        sender: q.0,
    })?;
    if !actor.speech {
        return Ok(Vec::new());
    }
    let mut roster = Vec::new();
    for (id, _) in tx.iter::<Personal>() {
        if id.0 == q.0.0 || tx.read::<Mind<C>>(id).is_none() {
            continue;
        }
        let local = tx.request(ReadPlacement(PersonId(id.0)))?.is_some_and(
            |place| matches!(place.location, Location::AtArea(area) if Some(area) == actor.area),
        );
        if local {
            roster.push(tx.request(ReadFacts {
                actor: PersonId(id.0),
                sender: q.0,
            })?);
        }
    }
    let restricted = tx.request(ReadCombatStatus(q.0))?.restricted;
    Ok(personal::available_actions_personal(
        own.as_ref(),
        &actor,
        &roster,
        restricted,
    ))
}
