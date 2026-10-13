//! Native character workflows over canonical rows and source-owned requests.
use crate::{decisions::*, orchestration::*, people::Person};
mod primitives;
use game_core::*;
use game_ecs::{Component, StableId};
use game_world::{OwnerReadTxn, OwnerSystem, OwnerTxn, Registry, Request, RuntimeSlot};
pub use primitives::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    marker::PhantomData,
};

fn classify_activity(owner: Option<&str>, resting: bool) -> Activity {
    match owner {
        None => Activity::Idle,
        Some("physiology") if resting => Activity::Rest,
        Some("workshop") => Activity::Working,
        _ => Activity::Committed,
    }
}

pub struct ReadPerson(pub PersonId);
impl Request for ReadPerson {
    type Response = Option<Person>;
}
pub struct ReadIdentity(pub PersonId);
impl Request for ReadIdentity {
    type Response = Option<Person>;
}
pub struct ReadKnownRoutes(pub PersonId);
impl Request for ReadKnownRoutes {
    type Response = BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>;
}
pub struct ReadParentChoice {
    pub actor: PersonId,
    pub partner: PersonId,
}
#[derive(Clone, Copy, Debug)]
pub struct ParentChoice {
    pub wants_parenthood: bool,
    pub partner_affection: i32,
    pub partner_trust: i32,
    pub knows_adequate_food: bool,
}
impl Request for ReadParentChoice {
    type Response = Option<ParentChoice>;
}
pub struct ReadRelationships;
impl Request for ReadRelationships {
    type Response = Vec<(PersonId, PersonId, Relationship)>;
}
pub struct ReadParentRelationships;
impl Request for ReadParentRelationships {
    type Response = Vec<(PersonId, PersonId)>;
}
pub struct ReceiveNewborn {
    pub child: PersonId,
    pub inventory: InventoryId,
    pub parents: [PersonId; 2],
    pub gestator: PersonId,
    pub location: Location,
}
impl Request for ReceiveNewborn {
    type Response = ();
}

/// Physiology supplies its effective rules alongside the canonical body.
pub struct ReadPhysical(pub PersonId);
#[derive(Clone, Debug)]
pub struct PhysicalFacts {
    pub vitals: Vitals,
    pub limits: NeedLimits,
}
impl Request for ReadPhysical {
    type Response = Option<PhysicalFacts>;
}
pub struct ReadPlacement(pub PersonId);
#[derive(Clone, Debug)]
pub struct PlacementFacts {
    pub location: Location,
    pub point: Option<ObservationPoint>,
    pub can_resume: bool,
}
impl Request for ReadPlacement {
    type Response = Option<PlacementFacts>;
}
pub struct ReadStage(pub PersonId);
#[derive(Clone, Copy, Debug)]
pub struct StageFacts {
    pub stage: Option<LifeStage>,
    pub independent: bool,
}
impl Request for ReadStage {
    type Response = StageFacts;
}
pub struct ReadPressure(pub PersonId);
#[derive(Clone, Copy, Debug)]
pub struct PressureFacts {
    pub hydration: i64,
    pub exposure: i64,
    pub drink_quantity: Quantity,
}
impl Request for ReadPressure {
    type Response = PressureFacts;
}
pub struct ReadFreedom(pub PersonId);
impl Request for ReadFreedom {
    type Response = bool;
}
pub struct ReadTransportBusy(pub PersonId);
impl Request for ReadTransportBusy {
    type Response = bool;
}
pub struct ReadLocalStock {
    pub actor: PersonId,
    pub area: AreaId,
    pub at_area: bool,
}
#[derive(Clone, Debug, Default)]
pub struct LocalStock {
    pub ground: Vec<crate::perception::StockObservationFacts>,
    pub carried: Vec<crate::perception::StockObservationFacts>,
}
impl Request for ReadLocalStock {
    type Response = LocalStock;
}
pub struct ReadVisibleSites;
impl Request for ReadVisibleSites {
    type Response = Vec<ObservedSite>;
}
pub struct ReadLocalRoutes(pub AreaId);
impl Request for ReadLocalRoutes {
    type Response = BTreeMap<AreaId, SimDuration>;
}
pub struct ReadBearers(pub PersonId);
impl Request for ReadBearers {
    type Response = BTreeSet<PersonId>;
}
pub struct ReceiveRoutes {
    pub actor: PersonId,
    pub routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
}
pub struct ShareRoutes {
    pub student: PersonId,
    pub teacher: PersonId,
}
impl Request for ShareRoutes {
    type Response = u64;
}
pub struct LearnLesson {
    pub teacher: PersonId,
    pub student: PersonId,
    pub knowledge: crate::receivers::LessonKnowledge,
}
impl Request for LearnLesson {
    type Response = u64;
}
impl Request for ReceiveRoutes {
    type Response = ();
}
pub struct DevelopAspiration {
    pub actor: PersonId,
    pub development: crate::social::Development,
    pub margin: u32,
}
impl Request for DevelopAspiration {
    type Response = ();
}
pub struct SetRole {
    pub actor: PersonId,
    pub role: Role,
}
impl Request for SetRole {
    type Response = ();
}
pub struct ReadChildPreference(pub PersonId);
impl Request for ReadChildPreference {
    type Response = Option<(ChildhoodInterest, SimTime)>;
}
pub struct ReadSignalVisible {
    pub observer: PersonId,
    pub child: PersonId,
}
impl Request for ReadSignalVisible {
    type Response = bool;
}
pub struct ReadSignalOcclusion {
    pub observer: PersonId,
    pub child: PersonId,
}
impl Request for ReadSignalOcclusion {
    type Response = bool;
}
pub struct ReadWorkEquipment(pub PersonId);
impl Request for ReadWorkEquipment {
    type Response = bool;
}
pub struct ReadActivity(pub PersonId);
impl Request for ReadActivity {
    type Response = Activity;
}
pub struct ReadResting(pub PersonId);
impl Request for ReadResting {
    type Response = bool;
}
pub struct ReadConversationAvailability(pub PersonId);
impl Request for ReadConversationAvailability {
    type Response = bool;
}

macro_rules! candidates {
    ($name:ident, $response:ty) => {
        pub struct $name<C: 'static>(pub PersonId, pub PhantomData<C>);
        impl<C: 'static> Request for $name<C> {
            type Response = $response;
        }
    };
}
candidates!(ReadHealthCandidate, Option<ActionEvidence<C>>);
candidates!(
    ReadProvisionCandidates,
    Vec<(ActionEvidence<C>, game_logistics::Requirement)>
);
candidates!(ReadProjectCandidates, Vec<ProjectOpportunity<C>>);
candidates!(
    ReadPropertyCandidates,
    Vec<(ActionEvidence<C>, InventoryId)>
);
candidates!(ReadCultivationCandidate, Option<ActionEvidence<C>>);
candidates!(ReadForceCandidate, Option<ActionEvidence<C>>);
candidates!(ReadSocialCandidate, Option<C>);
pub struct ReadDeliveryCandidate<C: 'static> {
    pub actor: PersonId,
    pub goal: CareDelivery,
    pub marker: PhantomData<C>,
}
impl<C: 'static> Request for ReadDeliveryCandidate<C> {
    type Response = Option<ActionEvidence<C>>;
}
/// The action's native owner executes it; no application callback controls selection.
pub struct ExecuteIntention<C: 'static> {
    pub actor: PersonId,
    pub command: C,
}
impl<C: 'static> Request for ExecuteIntention<C> {
    type Response = bool;
}
pub struct ReadClaim {
    pub actor: PersonId,
    pub shape: ActionShape,
    pub source: Option<InventoryId>,
}
impl Request for ReadClaim {
    type Response = ClaimFacts;
}
pub struct ReadInventoryGrant(pub InventoryId);
impl Request for ReadInventoryGrant {
    type Response = Option<PersonId>;
}
pub struct ReadSiteGrant(pub SiteId);
impl Request for ReadSiteGrant {
    type Response = Option<PersonId>;
}

pub struct LearnProduction {
    pub sequence: u64,
    pub actor: PersonId,
}
impl Request for LearnProduction {
    type Response = ();
}
pub struct ChildPlayed {
    pub actor: PersonId,
    pub interest: ChildhoodInterest,
}
impl Request for ChildPlayed {
    type Response = u64;
}
pub struct ReceiveCare {
    pub actor: PersonId,
    pub child: PersonId,
    pub good: Good,
    pub quantity: Quantity,
    pub event_sequence: u64,
}
impl Request for ReceiveCare {
    type Response = ();
}
pub struct ReceiveComfort {
    pub actor: PersonId,
    pub child: PersonId,
    pub event_sequence: u64,
}
impl Request for ReceiveComfort {
    type Response = ();
}
pub struct ReadMind<C: Component>(pub PersonId, pub PhantomData<C>);
impl<C: Component> Request for ReadMind<C> {
    type Response = Option<Mind<C>>;
}
pub struct ReceiveConversation(pub crate::social::ConversationEffect);
impl Request for ReceiveConversation {
    type Response = ();
}
pub struct SetDisposition {
    pub actor: PersonId,
    pub personality: Personality,
    pub dream: Aspiration,
}
impl Request for SetDisposition {
    type Response = ();
}
pub struct Review;
impl Request for Review {
    type Response = ();
}
pub struct RecordAttempt<C: Component> {
    pub actor: PersonId,
    pub at: SimTime,
    pub plan: Plan<C>,
    pub explanation: Explanation,
    pub accepted: bool,
}
impl<C: Component> Request for RecordAttempt<C> {
    type Response = ();
}
struct ExecutePlan<C: Component> {
    actor: PersonId,
    at: SimTime,
    plan: Plan<C>,
    explanation: Explanation,
}
impl<C: Component> Request for ExecutePlan<C> {
    type Response = ();
}
struct Execution<C>(PhantomData<C>);
impl<C: Component, E: 'static> OwnerSystem<E> for Execution<C> {
    type Runtime = ();
    type Input = ();
    type Output = ();
    fn execute(&self, _: &mut OwnerTxn<'_, '_, Self, E>, _: ()) -> Result<(), Error> {
        Ok(())
    }
}
fn execute_plan<C: Component, E: 'static>(
    _: &Execution<C>,
    tx: &mut OwnerTxn<'_, '_, Execution<C>, E>,
    request: ExecutePlan<C>,
) -> Result<(), Error> {
    let accepted = tx.request(ExecuteIntention {
        actor: request.actor,
        command: request.plan.action.clone(),
    })?;
    tx.defer(RecordAttempt {
        actor: request.actor,
        at: request.at,
        plan: request.plan,
        explanation: request.explanation,
        accepted,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Decision { actor: PersonId, accepted: bool },
    ProductionLearned { actor: PersonId, sequence: u64 },
}
/// Immutable constructors translate native requests into an opaque public command.
pub struct Systems<C> {
    pub command: fn(ActionRequest) -> C,
}
impl<C: Component, E: 'static> OwnerSystem<E> for Systems<C> {
    type Runtime = Option<Runtime>;
    type Input = ();
    type Output = ();
    fn execute(&self, tx: &mut OwnerTxn<'_, '_, Self, E>, _: ()) -> Result<(), Error> {
        self.review(tx, Review)
    }
}

pub fn register_systems<S: 'static, C: Component, E: 'static>(
    registry: &mut Registry<S, E>,
    slot: RuntimeSlot<S, Option<Runtime>>,
    command: fn(ActionRequest) -> C,
) -> Result<(), Error> {
    registry.register(Systems { command }, slot)?;
    registry.register(Execution::<C>(PhantomData), RuntimeSlot::stateless())?;
    registry.register_request::<Execution<C>, ExecutePlan<C>>(execute_plan::<C, E>)?;
    registry.owns::<Systems<C>, Person>()?;
    registry.owns::<Systems<C>, Mind<C>>()?;
    registry.owns::<Systems<C>, Stocks>()?;
    registry.register_read_request::<Systems<C>, ReadPerson>(Systems::<C>::read_person)?;
    registry.register_read_request::<Systems<C>, ReadIdentity>(Systems::<C>::read_identity)?;
    registry.register_read_request::<Systems<C>, ReadMind<C>>(Systems::<C>::read_mind)?;
    registry.register_read_request::<Systems<C>, ReadActivity>(Systems::<C>::read_activity)?;
    registry
        .register_read_request::<Systems<C>, ReadSignalVisible>(Systems::<C>::signal_visible)?;
    registry.register_read_request::<Systems<C>, ReadClaim>(Systems::<C>::read_claim)?;
    registry.register_read_request::<Systems<C>, ReadKnownRoutes>(Systems::<C>::read_routes)?;
    registry
        .register_read_request::<Systems<C>, ReadParentChoice>(Systems::<C>::read_parent_choice)?;
    registry
        .register_read_request::<Systems<C>, ReadRelationships>(Systems::<C>::read_relationships)?;
    registry.register_read_request::<Systems<C>, ReadParentRelationships>(
        Systems::<C>::read_parent_relationships,
    )?;
    registry.register_request::<Systems<C>, ReceiveNewborn>(Systems::<C>::receive_newborn)?;
    registry.register_request::<Systems<C>, LearnProduction>(Systems::<C>::learn_production)?;
    registry.register_request::<Systems<C>, ChildPlayed>(Systems::<C>::child_played)?;
    registry.register_request::<Systems<C>, ReceiveCare>(Systems::<C>::receive_care)?;
    registry.register_request::<Systems<C>, ReceiveComfort>(Systems::<C>::receive_comfort)?;
    registry
        .register_request::<Systems<C>, ReceiveConversation>(Systems::<C>::receive_conversation)?;
    registry.register_request::<Systems<C>, SetDisposition>(Systems::<C>::set_disposition)?;
    registry.register_request::<Systems<C>, ReceiveRoutes>(Systems::<C>::receive_routes)?;
    registry.register_request::<Systems<C>, ShareRoutes>(Systems::<C>::share_routes)?;
    registry.register_request::<Systems<C>, LearnLesson>(Systems::<C>::learn_lesson)?;
    registry.register_request::<Systems<C>, DevelopAspiration>(Systems::<C>::develop_aspiration)?;
    registry.register_request::<Systems<C>, SetRole>(Systems::<C>::set_role)?;
    registry.register_request::<Systems<C>, Review>(Systems::<C>::review)?;
    registry.register_request::<Systems<C>, RecordAttempt<C>>(Systems::<C>::record_attempt)?;
    register_primitives::<S, C, E>(registry)
}

impl<C: Component> Systems<C> {
    pub fn share_routes<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ShareRoutes,
    ) -> Result<u64, Error> {
        self.learn_lesson(
            tx,
            LearnLesson {
                teacher: request.teacher,
                student: request.student,
                knowledge: crate::receivers::LessonKnowledge::Travel {
                    lesson_gain: u64::MAX,
                },
            },
        )
    }
    pub fn learn_lesson<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: LearnLesson,
    ) -> Result<u64, Error> {
        let teacher = tx
            .read::<Mind<C>>(StableId(request.teacher.0))
            .cloned()
            .ok_or(Error::Unavailable)?;
        let id = StableId(request.student.0);
        let mut learner = tx.read::<Mind<C>>(id).cloned().ok_or(Error::Unavailable)?;
        teacher.validate()?;
        let gain =
            crate::receivers::receive_lesson_to_mind(&teacher, &mut learner, request.knowledge)?;
        learner.validate()?;
        tx.write(id, learner)?;
        self.experience(tx, request.teacher, "guidance")?;
        self.experience(tx, request.student, "guidance")?;
        Ok(gain)
    }
    pub fn read_claim<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadClaim,
    ) -> Result<ClaimFacts, Error> {
        Ok(match request.shape {
            ActionShape::Eat(source) => ClaimFacts {
                resource: source.0,
                last_granted: tx.request(ReadInventoryGrant(source))?,
            },
            ActionShape::Care(_) => {
                if let Some(source) = request.source {
                    ClaimFacts {
                        resource: source.0,
                        last_granted: tx.request(ReadInventoryGrant(source))?,
                    }
                } else {
                    ClaimFacts {
                        resource: 0,
                        last_granted: None,
                    }
                }
            }
            ActionShape::Work(site) => ClaimFacts {
                resource: site.0,
                last_granted: tx.request(ReadSiteGrant(site))?,
            },
            _ => ClaimFacts {
                resource: 0,
                last_granted: None,
            },
        })
    }
    pub fn read_identity<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadIdentity,
    ) -> Result<Option<Person>, Error> {
        self.read_person(tx, ReadPerson(request.0))
    }
    pub fn signal_visible<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadSignalVisible,
    ) -> Result<bool, Error> {
        for actor in [request.observer, request.child] {
            if tx.read::<Person>(StableId(actor.0)).is_none()
                || tx.request(ReadPhysical(actor))?.is_none()
            {
                return Ok(false);
            }
        }
        let first = tx.request(ReadPlacement(request.observer))?;
        let second = tx.request(ReadPlacement(request.child))?;
        if !first
            .zip(second)
            .is_some_and(|(first, second)| first.point.is_some() && first.point == second.point)
        {
            return Ok(false);
        }
        tx.request(ReadSignalOcclusion {
            observer: request.observer,
            child: request.child,
        })
    }
    pub fn receive_routes<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ReceiveRoutes,
    ) -> Result<(), Error> {
        let id = StableId(request.actor.0);
        if let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() {
            learn_routes_to_mind(&mut mind, &request.routes);
            tx.write(id, mind)?;
        }
        Ok(())
    }
    pub fn develop_aspiration<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: DevelopAspiration,
    ) -> Result<(), Error> {
        let id = StableId(request.actor.0);
        let Some(person) = tx.read::<Person>(id).cloned() else {
            return Ok(());
        };
        let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() else {
            return Ok(());
        };
        let Some(physical) = tx.request(ReadPhysical(request.actor))? else {
            return Ok(());
        };
        let life = tx.request(ReadStage(request.actor))?;
        mind.develop_aspiration(
            &request.development,
            crate::social::aspiration_control(person.role == Role::Ruler, person.role),
            life.independent,
            &physical.vitals,
            request.margin,
        );
        tx.write(id, mind)
    }
    pub fn set_role<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: SetRole,
    ) -> Result<(), Error> {
        let id = StableId(request.actor.0);
        let mut person = tx.read::<Person>(id).cloned().ok_or(Error::Unavailable)?;
        person.role = request.role;
        person.hunger_exempt = request.role == Role::Guardian;
        person.validate()?;
        tx.write(id, person)
    }
    pub fn read_activity<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadActivity,
    ) -> Result<Activity, Error> {
        let lease = tx.request(game_world::actions::ReadAction {
            actor: StableId(request.0.0),
        })?;
        let resting = tx.request(ReadResting(request.0))?;
        Ok(classify_activity(
            lease.as_ref().map(|lease| lease.owner.as_str()),
            resting,
        ))
    }
    fn activity<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        actor: PersonId,
    ) -> Result<Activity, Error> {
        let lease = tx.request(game_world::actions::ReadAction {
            actor: StableId(actor.0),
        })?;
        let resting = tx.request(ReadResting(actor))?;
        Ok(classify_activity(
            lease.as_ref().map(|lease| lease.owner.as_str()),
            resting,
        ))
    }
    pub fn record_attempt<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: RecordAttempt<C>,
    ) -> Result<(), Error> {
        let id = StableId(request.actor.0);
        let mut mind = tx.read::<Mind<C>>(id).cloned().ok_or(Error::Unavailable)?;
        mind.record_attempt(
            request.at,
            request.plan,
            request.explanation,
            request.accepted,
        )?;
        tx.write(id, mind)?;
        Ok(())
    }
    pub fn read_mind<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadMind<C>,
    ) -> Result<Option<Mind<C>>, Error> {
        Ok(tx.read::<Mind<C>>(StableId(request.0.0)).cloned())
    }
    pub fn read_person<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadPerson,
    ) -> Result<Option<Person>, Error> {
        Ok(tx.read::<Person>(StableId(request.0.0)).cloned())
    }
    pub fn read_routes<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadKnownRoutes,
    ) -> Result<BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>, Error> {
        Ok(tx
            .read::<Mind<C>>(StableId(request.0.0))
            .map_or_else(BTreeMap::new, |mind| mind.routes.clone()))
    }
    pub fn read_parent_choice<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        request: ReadParentChoice,
    ) -> Result<Option<ParentChoice>, Error> {
        let Some(mind) = tx.read::<Mind<C>>(StableId(request.actor.0)).cloned() else {
            return Ok(None);
        };
        let stocks = tx
            .read::<Stocks>(StableId(request.actor.0))
            .ok_or(Error::Unavailable)?;
        let relationship = mind.relationships.get(&request.partner);
        Ok(Some(ParentChoice {
            wants_parenthood: mind.dream == Aspiration::Parenthood,
            partner_affection: relationship.map_or(0, |row| row.affection),
            partner_trust: relationship.map_or(0, |row| row.trust),
            knows_adequate_food: stocks.known.values().any(|stock| {
                stock.good == Good::Food && stock.accessible && stock.quantity.0 >= 3_000
            }),
        }))
    }
    pub fn read_relationships<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        _: ReadRelationships,
    ) -> Result<Vec<(PersonId, PersonId, Relationship)>, Error> {
        Ok(tx
            .iter::<Mind<C>>()
            .into_iter()
            .flat_map(|(id, row)| {
                row.relationships
                    .into_iter()
                    .map(move |(other, relation)| (PersonId(id.0), other, relation))
            })
            .collect())
    }
    pub fn read_parent_relationships<E: 'static>(
        &self,
        tx: &mut OwnerReadTxn<'_, '_, Self, E>,
        _: ReadParentRelationships,
    ) -> Result<Vec<(PersonId, PersonId)>, Error> {
        let mut pairs = BTreeSet::new();
        for (id, mind) in tx.iter::<Mind<C>>() {
            for partner in mind.relationships.keys() {
                if PersonId(id.0) != *partner {
                    pairs.insert(crate::social::pair(PersonId(id.0), *partner));
                }
            }
        }
        Ok(pairs.into_iter().collect())
    }
    pub fn child_played<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ChildPlayed,
    ) -> Result<u64, Error> {
        let id = StableId(request.actor.0);
        let mut gain = 0;
        if let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() {
            let interest = mind.interests.entry(request.interest).or_default();
            gain = 100.min(1_000 - *interest);
            *interest += gain;
            tx.write(id, mind)?;
        }
        self.experience(tx, request.actor, "practice")?;
        Ok(gain)
    }
    pub fn receive_care<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ReceiveCare,
    ) -> Result<(), Error> {
        let id = StableId(request.actor.0);
        if let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() {
            mind.record_care_receipt(
                request.child,
                request.good,
                request.quantity,
                request.event_sequence,
            )?;
            tx.write(id, mind)?;
        }
        self.relationship(tx, request.child, request.actor, 50, 50)?;
        self.experience(tx, request.actor, "gave_care")?;
        self.experience(tx, request.child, "received_care")?;
        Ok(())
    }
    fn relationship<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        observer: PersonId,
        subject: PersonId,
        trust: i32,
        affection: i32,
    ) -> Result<(), Error> {
        let id = StableId(observer.0);
        if let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() {
            let row = mind.relationships.entry(subject).or_insert(Relationship {
                trust: 0,
                affection: 0,
            });
            row.trust = (i64::from(row.trust) + i64::from(trust)).clamp(-1_000, 1_000) as i32;
            row.affection =
                (i64::from(row.affection) + i64::from(affection)).clamp(-1_000, 1_000) as i32;
            tx.write(id, mind)?;
        }
        Ok(())
    }
    fn experience<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        actor: PersonId,
        experience: &str,
    ) -> Result<(), Error> {
        if let Some(expression) = tx.request(crate::temperament::services::LearnExperience {
            actor,
            experience: experience.into(),
        })? {
            let id = StableId(actor.0);
            if let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() {
                expression.apply_to(&mut mind.personality);
                tx.write(id, mind)?;
            }
        }
        Ok(())
    }
    pub fn receive_comfort<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ReceiveComfort,
    ) -> Result<(), Error> {
        self.relationship(tx, request.child, request.actor, 0, 50)?;
        self.experience(tx, request.actor, "gave_care")?;
        self.experience(tx, request.child, "received_care")
    }
    pub fn receive_conversation<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ReceiveConversation,
    ) -> Result<(), Error> {
        let effect = request.0;
        for (actor, other) in [(effect.first, effect.second), (effect.second, effect.first)] {
            self.relationship(tx, actor, other, effect.trust_gain, effect.affection_gain)?;
            self.experience(tx, actor, "company")?;
        }
        Ok(())
    }
    pub fn receive_newborn<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: ReceiveNewborn,
    ) -> Result<(), Error> {
        let child = StableId(request.child.0);
        if tx.read::<Person>(child).is_some()
            || tx.read::<Mind<C>>(child).is_some()
            || tx.read::<Stocks>(child).is_some()
        {
            return Err(Error::Unavailable);
        }
        let person = Person {
            id: request.child,
            role: Role::Citizen,
            inventory: request.inventory,
            work_units: 0,
            work_remainder: 0,
            work_rate: 1_000,
            hunger_exempt: false,
        };
        person.validate()?;
        let disposition = tx.request(crate::temperament::services::NewbornDisposition(
            request.child,
        ))?;
        if tx.runtime().is_some() {
            let mut newborn = tx
                .read::<Mind<C>>(StableId(request.gestator.0))
                .cloned()
                .ok_or(Error::Unavailable)?;
            newborn.interests.clear();
            newborn.routes.clear();
            newborn.last_failure = None;
            newborn.child_signals.clear();
            newborn.sites.clear();
            newborn.values.clear();
            newborn.relationships.clear();
            newborn.dream = Aspiration::Comfort;
            newborn.plan = None;
            newborn.retry_after = tx.now();
            newborn.goal = Goal::Survival;
            if let Some(disposition) = disposition {
                newborn.personality = disposition;
            }
            newborn.explanation = Explanation {
                goal: Goal::Survival,
                need: 0,
                benefit: 0,
                value: 0,
                total: 0,
                blocker: None,
            };
            let birthplace = tx
                .request(ReadPlacement(request.gestator))?
                .and_then(|placement| placement.point);
            for parent in request.parents.into_iter().collect::<BTreeSet<_>>() {
                let presence = tx.request(ReadPlacement(parent))?;
                if birthplace.is_some()
                    && presence.is_some_and(|presence| presence.point == birthplace)
                {
                    let id = StableId(parent.0);
                    if let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() {
                        mind.relationships.insert(
                            request.child,
                            Relationship {
                                trust: 0,
                                affection: i32::from(mind.personality.empathy),
                            },
                        );
                        tx.write(id, mind)?;
                    }
                }
            }
            tx.write(child, newborn)?;
            tx.write(child, Stocks::default())?;
        }
        tx.write(child, person)
    }
    pub fn learn_production<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: LearnProduction,
    ) -> Result<(), Error> {
        let Some(runtime) = tx.runtime().as_ref() else {
            return Ok(());
        };
        if request.sequence <= runtime.last_learning_event {
            return Ok(());
        }
        let concept = runtime.work_concept;
        if let Some(mut mind) = tx.read::<Mind<C>>(StableId(request.actor.0)).cloned() {
            mind.learn_production(concept);
            tx.write(StableId(request.actor.0), mind)?;
        }
        tx.runtime_mut().as_mut().unwrap().last_learning_event = request.sequence;
        self.experience(tx, request.actor, "practice")?;
        Ok(())
    }
    pub fn set_disposition<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        request: SetDisposition,
    ) -> Result<(), Error> {
        if !request.personality.valid() {
            return Err(Error::InvalidConfiguration(
                "invalid personal disposition".into(),
            ));
        }
        let id = StableId(request.actor.0);
        let mut mind = tx.read::<Mind<C>>(id).cloned().ok_or(Error::Unavailable)?;
        mind.personality = request.personality;
        mind.dream = request.dream;
        tx.write(id, mind)
    }
    fn evidence(&self, request: ActionRequest) -> ActionEvidence<C> {
        let (kind, source) = match request {
            ActionRequest::Eat(id) => (ActionKind::Shape(ActionShape::Eat(id)), Some(id)),
            ActionRequest::Move(id) => (ActionKind::Shape(ActionShape::Move(id)), None),
            ActionRequest::Work(id) => (ActionKind::Shape(ActionShape::Work(id)), None),
            ActionRequest::Care(child, source) => {
                (ActionKind::Shape(ActionShape::Care(child)), Some(source))
            }
            ActionRequest::Comfort(child) => (ActionKind::Shape(ActionShape::Comfort(child)), None),
            ActionRequest::Rest => (ActionKind::Shape(ActionShape::Rest), None),
            ActionRequest::Play(_) => (ActionKind::Practice, None),
            ActionRequest::Stop => (ActionKind::Shape(ActionShape::Stop), None),
            ActionRequest::Resume => (ActionKind::Shape(ActionShape::Resume), None),
        };
        ActionEvidence {
            action: (self.command)(request),
            kind,
            risk: Risk::Safe,
            source,
        }
    }
}

struct NativeDecisionPort<'a, C> {
    owner: &'a Systems<C>,
    facts: PersonalFacts,
    force: Option<ActionEvidence<C>>,
    opportunities: Opportunities<C>,
    deliveries: Vec<(CareDelivery, Option<ActionEvidence<C>>)>,
}
impl<C: Component> DecisionPort<C> for NativeDecisionPort<'_, C> {
    fn facts(&self, _: PersonId) -> PersonalFacts {
        self.facts.clone()
    }
    fn action(&self, request: ActionRequest) -> ActionEvidence<C> {
        self.owner.evidence(request)
    }
    fn force_action(&self, _: PersonId) -> Option<ActionEvidence<C>> {
        self.force.clone()
    }
    fn opportunities(&self, _: PersonId) -> Opportunities<C> {
        self.opportunities.clone()
    }
    fn delivery_action(&self, _: PersonId, goal: &CareDelivery) -> Option<ActionEvidence<C>> {
        self.deliveries
            .iter()
            .find(|(known, _)| known == goal)
            .and_then(|(_, action)| action.clone())
    }
}

#[derive(Default)]
struct NativeObservations {
    observers: BTreeMap<PersonId, ObserverFacts>,
    dependents: BTreeMap<PersonId, DependentFacts>,
    points: BTreeMap<LocationKey, ObservationPoint>,
    visible: BTreeSet<(PersonId, PersonId)>,
    stocks: BTreeMap<PersonId, Vec<StockKnowledge>>,
    sites: Vec<ObservedSite>,
    routes: BTreeMap<AreaId, BTreeMap<AreaId, SimDuration>>,
    bearers: BTreeMap<PersonId, BTreeSet<PersonId>>,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LocationKey([u64; 10]);
impl From<&Location> for LocationKey {
    fn from(location: &Location) -> Self {
        let mut key = [0; 10];
        match location {
            Location::AtArea(area) => {
                key[1] = area.0;
            }
            Location::InTransit {
                from,
                to,
                departed,
                arrives,
                road_start,
            } => {
                key[..5].copy_from_slice(&[1, from.0, to.0, departed.0, arrives.0]);
                if let Some(start) = road_start {
                    key[5..].copy_from_slice(&[
                        1,
                        start.from.0,
                        start.to.0,
                        start.progress.0,
                        start.duration.0,
                    ]);
                }
            }
            Location::StoppedOnRoad {
                from,
                to,
                progress,
                duration,
            } => {
                key[..5].copy_from_slice(&[2, from.0, to.0, progress.0, duration.0]);
            }
            Location::Carried { carrier } => {
                key[0] = 3;
                key[1] = carrier.0;
            }
        }
        Self(key)
    }
}
impl ObservationPort for NativeObservations {
    fn actors(&self) -> Vec<PersonId> {
        self.observers.keys().copied().collect()
    }
    fn observer(&self, actor: PersonId) -> ObserverFacts {
        self.observers[&actor].clone()
    }
    fn co_present(&self, first: &Location, second: &Location, _: SimTime) -> bool {
        self.point(first)
            .is_some_and(|point| self.point(second) == Some(point))
    }
    fn bears(&self, actor: PersonId, child: PersonId) -> bool {
        self.bearers
            .get(&child)
            .is_some_and(|bearers| bearers.contains(&actor))
    }
    fn signal_visible(&self, actor: PersonId, child: PersonId) -> bool {
        self.visible.contains(&(actor, child))
    }
    fn dependent(&self, child: PersonId) -> DependentFacts {
        self.dependents[&child].clone()
    }
    fn stocks(&self, actor: PersonId, _: AreaId, _: bool, _: SimTime) -> Vec<StockKnowledge> {
        self.stocks.get(&actor).cloned().unwrap_or_default()
    }
    fn sites(&self) -> Vec<ObservedSite> {
        self.sites.clone()
    }
    fn routes(&self, area: AreaId) -> BTreeMap<AreaId, SimDuration> {
        self.routes.get(&area).cloned().unwrap_or_default()
    }
}
impl NativeObservations {
    fn point(&self, location: &Location) -> Option<&ObservationPoint> {
        self.points.get(&LocationKey::from(location))
    }
}

impl<C: Component> Systems<C> {
    pub fn review<E: 'static>(
        &self,
        tx: &mut OwnerTxn<'_, '_, Self, E>,
        _: Review,
    ) -> Result<(), Error> {
        let Some(runtime) = tx.runtime().clone() else {
            return Ok(());
        };
        let now = tx.now();
        let mut observed = NativeObservations {
            sites: tx.request(ReadVisibleSites)?,
            ..Default::default()
        };
        let mut facts = BTreeMap::new();
        let mut placements = BTreeMap::new();
        for (id, person) in tx.iter::<Person>() {
            let actor = PersonId(id.0);
            let Some(physical) = tx.request(ReadPhysical(actor))? else {
                continue;
            };
            let Some(placement) = tx.request(ReadPlacement(actor))? else {
                continue;
            };
            let life = tx.request(ReadStage(actor))?;
            let pressure = tx.request(ReadPressure(actor))?;
            let action = tx.request(game_world::actions::ReadAction { actor: id })?;
            let activity = self.activity(tx, actor)?;
            let place = match placement.point {
                Some(ObservationPoint::Area(area)) => Place::Area(area),
                _ if matches!(placement.location, Location::StoppedOnRoad { .. }) => Place::Stopped,
                _ => Place::Moving,
            };
            let own_stock = if let Some(area) = placement.location.cell() {
                tx.request(ReadLocalStock {
                    actor,
                    area,
                    at_area: matches!(placement.location, Location::AtArea(_)),
                })?
            } else {
                LocalStock::default()
            };
            let own_cargo = own_stock
                .carried
                .iter()
                .map(|stock| stock.inventory)
                .collect();
            if let Some(area) = placement.location.cell() {
                let at_area = matches!(placement.location, Location::AtArea(_));
                observed.stocks.insert(
                    actor,
                    crate::perception::observe_stock_facts(
                        actor,
                        area,
                        at_area,
                        now,
                        own_stock.ground,
                        own_stock.carried,
                    ),
                );
                if at_area && !observed.routes.contains_key(&area) {
                    observed
                        .routes
                        .insert(area, tx.request(ReadLocalRoutes(area))?);
                }
            }
            if let Some(point) = placement.point {
                observed
                    .points
                    .insert(LocationKey::from(&placement.location), point);
            }
            let dependent = matches!(life.stage, Some(LifeStage::Infant | LifeStage::Child));
            observed.observers.insert(
                actor,
                ObserverFacts {
                    actor,
                    location: placement.location.clone(),
                    point: placement.point,
                    alive: physical.vitals.alive,
                    health: physical.vitals.health,
                    dependent,
                    meal_size: physical.limits.meal_size,
                },
            );
            if dependent {
                observed
                    .bearers
                    .insert(actor, tx.request(ReadBearers(actor))?);
                observed.dependents.insert(
                    actor,
                    DependentFacts {
                        hunger: physical.vitals.hunger,
                        meal_relief: physical.limits.meal_relief,
                        hunger_harm: physical.limits.hunger_harm,
                        preference: tx.request(ReadChildPreference(actor))?,
                        hydration_urgency: pressure.hydration,
                    },
                );
            }
            let mut limits = physical.limits;
            limits.drink_quantity = pressure.drink_quantity;
            let owner = action.as_ref().map(|slot| slot.owner.as_str());
            let delegated = matches!(owner, Some("conflict" | "lineage" | "missions" | "health"))
                || tx.request(ReadTransportBusy(actor))?;
            let in_dialogue = matches!(owner, Some("army" | "realm" | "property" | "social"));
            facts.insert(
                actor,
                PersonalFacts {
                    actor,
                    now,
                    vitals: physical.vitals,
                    limits,
                    activity,
                    place,
                    control: if person.role == Role::Ruler {
                        Control::Manual
                    } else if delegated {
                        Control::Delegated
                    } else {
                        Control::Autonomous
                    },
                    in_dialogue,
                    independent: life.independent,
                    child: life.stage == Some(LifeStage::Child),
                    hunger_exempt: person.hunger_exempt,
                    work_units: person.work_units,
                    own_cargo,
                    observed_contacts: BTreeSet::new(),
                    can_resume: placement.can_resume,
                    hydration_urgency: pressure.hydration,
                    exposure_urgency: pressure.exposure,
                    freedom_restricted: tx.request(ReadFreedom(actor))?,
                },
            );
            placements.insert(actor, placement);
        }
        // Join only exact co-located dependent cohorts before requesting visibility.
        let mut cohorts: BTreeMap<ObservationPoint, Vec<PersonId>> = BTreeMap::new();
        for child in observed.dependents.keys() {
            if let Some(point) = observed.observers[child].point {
                cohorts.entry(point).or_default().push(*child);
            }
        }
        for (actor, observer) in &observed.observers {
            let Some(point) = observer.point else {
                continue;
            };
            let mut admitted = 0;
            for child in cohorts.get(&point).into_iter().flatten() {
                if !matches!(
                    observer.location,
                    Location::AtArea(_) | Location::StoppedOnRoad { .. }
                ) && !observed.bears(*actor, *child)
                {
                    continue;
                }
                if tx.request(ReadSignalVisible {
                    observer: *actor,
                    child: *child,
                })? {
                    observed.visible.insert((*actor, *child));
                    admitted += 1;
                    if admitted == 64 {
                        break;
                    }
                }
            }
        }
        for observation in prepare_observations(now, &observed) {
            let id = StableId(observation.actor.0);
            let Some(mut mind) = tx.read::<Mind<C>>(id).cloned() else {
                continue;
            };
            let mut stocks = tx.read::<Stocks>(id).cloned().ok_or(Error::Unavailable)?;
            for stock in observation.stocks {
                mind.observe_stock(
                    &mut stocks,
                    now,
                    observation.area,
                    observation.meal_size,
                    stock,
                );
            }
            if let Some(routes) = observation.routes {
                mind.routes.insert(observation.area, routes);
            }
            mind.child_signals
                .retain(|_, signal| signal.location != observation.location);
            mind.child_signals.extend(observation.children);
            mind.reconcile_care_observation(observation.actor);
            mind.sites
                .extend(observation.sites.into_iter().map(|site| (site.site, site)));
            tx.write(id, mind)?;
            tx.write(id, stocks)?;
        }
        let mut intentions = Vec::new();
        let mut claims = BTreeMap::new();
        for (id, mind) in tx.iter::<Mind<C>>() {
            let actor = PersonId(id.0);
            let Some(mut personal) = facts.remove(&actor) else {
                continue;
            };
            let placement = &placements[&actor];
            personal.observed_contacts = observed_contacts(&mind, now, |location, _| {
                observed
                    .point(location)
                    .is_some_and(|point| placement.point.as_ref() == Some(point))
            });
            let stocks = tx.read::<Stocks>(id).cloned().ok_or(Error::Unavailable)?;
            if !mind.needs_candidates(&personal) {
                continue;
            }
            let force = tx.request(ReadForceCandidate::<C>(actor, PhantomData))?;
            let opportunities = if matches!(personal.place, Place::Area(_)) {
                Opportunities {
                    health: tx.request(ReadHealthCandidate::<C>(actor, PhantomData))?,
                    provisions: tx.request(ReadProvisionCandidates::<C>(actor, PhantomData))?,
                    projects: tx.request(ReadProjectCandidates::<C>(actor, PhantomData))?,
                    property: tx.request(ReadPropertyCandidates::<C>(actor, PhantomData))?,
                    cultivation: tx.request(ReadCultivationCandidate::<C>(actor, PhantomData))?,
                }
            } else {
                Opportunities::default()
            };
            let mut deliveries = Vec::new();
            for goal in mind.care_deliveries(&personal) {
                let action = tx.request(ReadDeliveryCandidate::<C> {
                    actor,
                    goal: goal.clone(),
                    marker: PhantomData,
                })?;
                deliveries.push((goal, action));
            }
            let intention = runtime.decide_from_port(
                actor,
                &mind,
                &stocks,
                &NativeDecisionPort {
                    owner: self,
                    facts: personal,
                    force,
                    opportunities,
                    deliveries,
                },
            );
            let claim = if let Some(plan) = &intention.decision.plan {
                tx.request(ReadClaim {
                    actor,
                    shape: plan.shape,
                    source: plan.prospects.source,
                })?
            } else {
                ClaimFacts {
                    resource: 0,
                    last_granted: None,
                }
            };
            claims.insert(actor, claim);
            intentions.push(intention);
        }
        order_intentions(&mut intentions, |intention| claims[&intention.actor]);
        for intention in intentions {
            let Some(plan) = intention.decision.plan else {
                continue;
            };
            tx.defer(ExecutePlan {
                actor: intention.actor,
                at: now,
                plan,
                explanation: intention.decision.explanation,
            })?;
        }
        Ok(())
    }
}

pub fn boundary<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerReadTxn<'_, '_, Systems<C>, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<Option<SimTime>, Error> {
    if tx.runtime().is_none() {
        return Ok(None);
    }
    let next = (span.from.0 / REVIEW_INTERVAL.0)
        .checked_add(1)
        .and_then(|tick| tick.checked_mul(REVIEW_INTERVAL.0))
        .ok_or(Error::Overflow)?;
    Ok(Some(SimTime(next)).filter(|next| *next <= span.to))
}
struct WorkStep(Vec<(StableId, crate::people::CumulativeOutput)>);
pub fn prepare<C: Component, E: 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<(), Error> {
    let mut output = Vec::new();
    for (id, person) in tx.iter::<Person>() {
        let actor = PersonId(id.0);
        let at_area = tx
            .request(ReadPlacement(actor))?
            .is_some_and(|place| matches!(place.location, Location::AtArea(_)));
        let body = tx.request(ReadPhysical(actor))?;
        let activity = owner.activity(tx, actor)?;
        let ready = body.is_none_or(|body| {
            body.vitals.alive && body.vitals.health > 0 && activity == Activity::Working
        });
        let equipment = tx.request(ReadWorkEquipment(actor))?;
        let result = crate::people::cumulative_progress(crate::people::CumulativeInput {
            units: person.work_units,
            remainder: person.work_remainder,
            units_per_minute: if at_area && ready && equipment {
                person.work_rate
            } else {
                0
            },
            elapsed_ms: span.elapsed().0,
        })?;
        output.push((id, result));
    }
    tx.share(WorkStep(output))
}
pub fn advance<C: Component, E: 'static>(
    _: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    _: game_world::scheduling::ClockSpan,
) -> Result<(), Error> {
    let output = tx.fact::<WorkStep>().ok_or(Error::Unavailable)?.0.clone();
    for (id, result) in output {
        let mut person = tx.read::<Person>(id).cloned().ok_or(Error::Unavailable)?;
        person.work_units = result.units;
        person.work_remainder = result.remainder;
        tx.write(id, person)?;
    }
    Ok(())
}
pub fn close<C: Component, E: 'static>(
    owner: &Systems<C>,
    tx: &mut OwnerTxn<'_, '_, Systems<C>, E>,
    span: game_world::scheduling::ClockSpan,
) -> Result<(), Error> {
    if span.to.0 % REVIEW_INTERVAL.0 == 0 {
        owner.review(tx, Review)?;
    }
    Ok(())
}
pub fn register_schedule<S: 'static, C: Component, E: 'static>(
    registry: &mut Registry<S, E>,
    scheduler: &mut game_world::scheduling::Scheduler<E>,
    order: game_world::scheduling::Order,
) -> Result<(), Error> {
    scheduler.register_owner_phased::<Systems<C>, S>(
        registry,
        order,
        Some(boundary::<C, E>),
        Some(prepare::<C, E>),
        Some(advance::<C, E>),
        Some(close::<C, E>),
    )
}

#[cfg(test)]
mod tests;
