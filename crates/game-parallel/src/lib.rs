mod autonomy;
mod cargo;
mod knowledge;
mod lineage;
mod logistics;
mod market;
mod production;
mod travel;
pub use game_parallel_agreements::{
    Agreements, Contract as Agreement, Decision as AgreementDecision,
    Delivery as AgreementDelivery, Event as AgreementEvent, EventKind as AgreementEventKind,
    Policy as AgreementPolicy, Proposal as AgreementProposal, Status as AgreementStatus,
};
pub use game_parallel_agriculture::{Agriculture, CropPhase, Farm, FarmRules, Field};
pub use game_parallel_autonomy::{
    Actor as AutonomousActor, Autonomy, Event as AutonomyEvent, Outcome as AutonomyOutcome,
    Rules as AutonomyRules,
};
pub use game_parallel_autowork::requests as work_requests;
pub use game_parallel_autowork::{Autowork, Policy as WorkPolicy};
pub use game_parallel_care::{Care, Event as CareEvent, Job as CareJob, Start as CareStart};
pub use game_parallel_cargo::{
    Cargo, FoodAidPolicy, FoodAsker, FoodDepositPolicy, FoodEnrollment, FoodGift, FoodRequest,
    FoodSharingPolicy, FoodStore, FoodStoreBlueprint, FoodStoreBuilder, FoodStoreChoice,
    FoodStoreClaim, Holding,
};
pub use game_parallel_civic_auto::{
    CivicAuto, FoundingRules as CivicFoundingRules, Policy as CivicPolicy,
};
pub use game_parallel_conflict::{
    Command as CombatCommand, Conflict, Encounter as CombatEncounter, Event as CombatEvent,
    Fighter as CombatFighter, Order as CombatOrder,
};
pub use game_parallel_dependent_care::{
    DependentCare as Caregiving, Event as CaregivingEvent, Policy as CaregivingPolicy,
    Rules as CaregivingRules,
};
pub use game_parallel_disease::{
    Case as DiseaseCase, Disease, Event as DiseaseEvent, EventKind as DiseaseEventKind,
    Strain as DiseaseStrain, Treatment as DiseaseTreatment,
};
pub use game_parallel_dreams::{Dream, Dreams, Event as DreamEvent, SetDream};
pub use game_parallel_environment::{Environment, Hazard, Season as EnvironmentSeason};
pub use game_parallel_fatigue::{Fatigue, Person as FatiguePerson, Rules as FatigueRules};
pub use game_parallel_goals::{
    Completion as GoalCompletion, Event as GoalEvent, Goal, Goals, SetGoal,
};
pub use game_parallel_goods_cargo::{
    Direction as GoodsDirection, Exchange as GoodsExchange, GoodsCargo, GoodsHolding,
    Transfer as GoodsTransfer,
};
pub use game_parallel_guardians::{
    Guardian, Guardians, Journey as GuardianJourney, ReleaseDecision,
};
pub use game_parallel_institutions::{
    Action as InstitutionAction, AdmissionEvent as InstitutionAdmissionEvent,
    Body as InstitutionBody, Cast as InstitutionVote, Event as InstitutionEvent, Institutions,
    MembershipRequest as InstitutionMembershipRequest, Open as InstitutionOpen,
    Vote as InstitutionRecordedVote,
};
pub use game_parallel_knowledge::{Knowledge, Mind, Record as KnowledgeRecord};
pub use game_parallel_lineage::{
    Event as LineageEvent, EventKind as LineageEventKind, Invocation, InvocationRules,
    InvocationWarning, InvocationWarningKind, Life as PastLife, Lineage,
};
pub use game_parallel_logistics::Shipment;
pub use game_parallel_market::{
    Decision as TradeDecision, Event as MarketEvent, Influence as TradeInfluence, Market,
    Offer as TradeOffer, Outcome as TradeOutcome, Policy as TradePolicy, Proposal as TradeProposal,
    Value as TradeValue,
};
pub use game_parallel_missions::{
    Departure as MissionDeparture, Draft as MissionDraft, Event as MissionEvent,
    EventKind as MissionEventKind, Mission, Missions, Objective as MissionObjective,
    Phase as MissionPhase, Start as MissionStart,
};
pub use game_parallel_nutrition::{
    Nutrients, Nutrition, Person as NutritionPerson, Rules as NutritionRules,
};
pub use game_parallel_opinions::{
    Confrontation as ConfrontationPreference, Event as OpinionEvent, Opinion, Opinions,
    Rules as OpinionRules, Sensitivity as OpinionSensitivity,
};
pub use game_parallel_people::{Person, Region, Rules};
pub use game_parallel_personality::{Person as PersonalityPerson, Personality};
pub use game_parallel_production::{
    Amount, Event as ProductionEvent, EventKind as ProductionEventKind, FoodEffect, FoodEffects,
    FoodProfile, Good, Inventory, Job, JobPhase, Production, Recipe, RecipeCategory,
    Start as StartWork, Stock,
};
pub use game_parallel_property::{
    Claim as PropertyClaim, Estate as PropertyEstate, Inheritance as PropertyInheritance,
    Offer as PropertyOffer, Property, Theft as PropertyTheft,
};
pub use game_parallel_purpose::{
    Event as PurposeEvent, Outcome as PurposeOutcome, Person as PurposePerson, Purpose,
    Rules as PurposeRules,
};
pub use game_parallel_skills::{
    Alias as SkillAlias, Attribution as SkillAttribution, CatalogTraining as SkillCatalogTraining,
    Completion as SkillCompletion, Lesson as SkillLesson, Record as SkillRecord, Rule as SkillRule,
    Skills, TeacherPolicy as SkillTeacherPolicy,
};
pub use game_parallel_social::{
    Encounter, Participant as SocialParticipant, PressurePolicy as SocialPressurePolicy,
    Relationship, Rules as SocialRules, Social,
};
pub use game_parallel_tactics::{
    Battle as TacticalBattle, Map as TacticalMap, Move as TacticalMove,
    Obstacle as TacticalObstacle, Placement as TacticalPlacement, Point as TacticalPoint,
    Rules as TacticalRules, Tactics,
};
pub use game_parallel_transport::{Move, Route, Traveler};
use serde::{Deserialize, Serialize};
mod families;
mod goods_cargo;
pub use game_parallel_families::{
    Families, FamilyEvent, FamilyRules, FamilyTotals, Genome, Member, Pregnancy, Sex,
};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct World {
    #[serde(default = "schema_version")]
    pub version: u32,
    pub tick: u64,
    pub seed: u64,
    pub rules: Rules,
    pub regions: Vec<Region>,
    #[serde(default)]
    pub shipments: Vec<Shipment>,
    #[serde(default)]
    pub routes: Vec<Route>,
    #[serde(default)]
    pub travelers: Vec<Traveler>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub families: Option<Families>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agriculture: Option<Agriculture>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cargo: Option<Cargo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub social: Option<Social>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<Knowledge>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autonomy: Option<Autonomy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production: Option<Production>,
    /// Physiological effects awaiting digestion after real food consumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub food_effects: Option<FoodEffects>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nutrition: Option<Nutrition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goods_cargo: Option<GoodsCargo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market: Option<Market>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agreements: Option<Agreements>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opinions: Option<Opinions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autowork: Option<Autowork>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goals: Option<Goals>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dreams: Option<Dreams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missions: Option<Missions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<Purpose>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personality: Option<Personality>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guardians: Option<Guardians>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<Property>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub institutions: Option<Institutions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub civic_auto: Option<CivicAuto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<Conflict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub care: Option<Care>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caregiving: Option<Caregiving>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<Environment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skills: Option<Skills>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fatigue: Option<Fatigue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tactics: Option<Tactics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disease: Option<Disease>,
}
fn schema_version() -> u32 {
    1
}
impl World {
    game_parallel_social_pressure::world_pressure_method!();
    game_parallel_agreement_host::agreement_methods!();
    game_parallel_validation::world_codec_methods!();
    pub fn step(&mut self) -> Result<(), &'static str> {
        let next = self.tick.checked_add(1).ok_or("tick overflow")?;
        game_parallel_validation::ensure_reference_supported!(self);
        lineage::validate(self).map_err(|_| "invalid lineage state")?;
        let invocation = self
            .lineage
            .as_mut()
            .map(|lineage| lineage.prepare_invocation(self.tick))
            .transpose()?
            .flatten();
        let institutions = game_parallel_institutions::prepare_world(
            self.institutions.as_ref(),
            &self.regions,
            &self.travelers,
            next,
        );
        let guardians = game_parallel_guardians::prepare_world(
            self.guardians.as_ref(),
            self.lineage.is_some(),
            self.lineage.as_ref().and_then(|lineage| lineage.current),
            &self.regions,
            &self.routes,
        )
        .map_err(|_| "invalid guardian state")?;
        game_parallel_validation::validate_step(
            &self.regions,
            &self.travelers,
            &self.rules,
            self.cargo.as_ref(),
            self.agriculture.as_ref(),
        )?;
        let mut busy = self
            .production
            .as_ref()
            .map_or_else(Vec::new, |p| p.busy_people());
        game_parallel_lineage::exclude_controlled(self.lineage.as_ref(), self.tick, &mut busy);
        let production = self
            .production
            .as_ref()
            .map(|p| p.prepare(&self.regions, &self.travelers, self.tick, next))
            .transpose()
            .map_err(|_| "invalid production state")?;
        game_parallel_market_auto::validate_boundary(
            self.production.as_ref(),
            production.as_ref(),
            self.goods_cargo.as_ref(),
            self.market.as_ref(),
            self.tick,
        )?;
        let trade = if let (Some(market), Some(cargo), Some(state), Some(draft)) = (
            &self.market,
            &self.goods_cargo,
            &self.production,
            &production,
        ) {
            game_parallel_market_auto::prepare_with_opinions(
                market,
                cargo,
                draft.people(),
                &state.goods,
                &busy,
                next,
                self.opinions.as_ref(),
            )
            .map_err(|_| "invalid automatic trade")?
        } else {
            None
        };
        if let Some(trade) = &trade {
            busy.extend_from_slice(&trade.actors);
        }
        let work = if let Some(autowork) = &self.autowork {
            let state = self
                .production
                .as_ref()
                .ok_or("autowork requires production")?;
            let people = production
                .as_ref()
                .ok_or("autowork requires production")?
                .people();
            autowork
                .prepare_with_property(people, state, &busy, self.property.as_ref())
                .map_err(|_| "invalid autonomous work")?
        } else {
            None
        };
        if let Some(work) = &work {
            busy.extend_from_slice(&work.actors);
        }
        let social = game_parallel_social::adapter::prepare(
            self.social.as_ref(),
            &self.regions,
            &self.travelers,
            self.seed,
            next,
            &busy,
        )?;
        let knowledge = knowledge::prepare(self, social.as_ref())?;
        if let Some(social) = &social {
            busy.extend(social.events.iter().flat_map(|event| [event.a, event.b]));
        }
        let autonomy = autonomy::prepare(self, &busy)?;
        if let Some(autonomy) = &autonomy {
            busy.extend(autonomy.decisions.iter().map(|decision| decision.person));
        }
        families::step(self, next, &busy)?;
        if let Some(production) = production {
            production.commit(self.production.as_mut().expect("prepared production"));
        }
        if let Some(work) = work {
            let state = self.production.as_mut().expect("prepared production");
            work.reservations
                .commit(&mut state.jobs, &mut state.inventories);
            self.autowork.as_mut().expect("prepared autowork").next_id = work.next_id;
        }
        if let Some(invocation) = invocation {
            self.lineage.as_mut().unwrap().invocation = Some(invocation);
        }
        game_parallel_institutions::commit_optional(institutions, &mut self.institutions);
        lineage::advance(self, next);
        if let Some(guardians) = guardians {
            self.guardians = Some(guardians);
        }
        if let Some(trade) = trade {
            trade.commit(
                self.market.as_mut().expect("prepared market"),
                self.goods_cargo.as_mut().expect("prepared cargo"),
                self.opinions.as_mut(),
            );
        }
        if let Some(social) = social {
            self.social = Some(social);
        }
        if let Some(knowledge) = knowledge {
            self.knowledge = Some(knowledge);
        }
        if let Some(autonomy) = autonomy {
            autonomy::publish(self, autonomy);
        }
        game_parallel_family_host::enroll_births!(&mut *self);
        if let Some(agreements) = &mut self.agreements {
            agreements.advance_in_place(next);
        }
        game_parallel_opinions::advance_optional(
            &mut self.opinions,
            self.agreements.as_ref(),
            self.social
                .as_ref()
                .map_or(&[], |social| social.events.as_slice()),
            next,
        );
        Ok(())
    }
}

#[cfg(test)]
extern crate self as game_parallel;
#[cfg(test)]
#[path = "../tests/support/autonomy.rs"]
mod autonomy_tests;
