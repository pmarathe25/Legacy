//! Person-driven economic activity over authoritative dated physical goods.
pub use game_parallel_agreements::OfferPolicy as PromiseOfferPolicy;
pub use game_parallel_agreements::WorkOfferPolicy as WorkPromiseOfferPolicy;
pub use game_parallel_autowork::requests::OfferValue as WorkOfferValue;
pub use game_parallel_economy_building::demolition::{
    Event as DemolitionEvent, Policy as DemolitionPolicy,
};
pub use game_parallel_economy_building::salvage::{Event as SalvageEvent, Policy as SalvagePolicy};
pub use game_parallel_economy_building::{
    Blueprint as BuildingBlueprint, Construction, Event as ConstructionEvent, GoalBuilder,
    Policy as ConstructionPolicy,
};
pub use game_parallel_economy_campaign::{Campaign, ReleaseDecision};
pub use game_parallel_economy_combat::Combat;
pub use game_parallel_economy_confinement::{
    CapturePolicy as ConfinementCapturePolicy, CaptureResponse as ConfinementCaptureResponse,
    Cell as ConfinementCell, CellKind as ConfinementCellKind, Confinement, EscapePreference,
    GuardPolicy as ConfinementGuardPolicy, Hold as ConfinementHold,
    ReleasePolicy as ConfinementReleasePolicy,
};
pub use game_parallel_economy_dynamics::Dynamics;
pub use game_parallel_economy_expeditions::{
    DeliveryAcquisition as MissionDeliveryAcquisition,
    DeliveryPreference as MissionDeliveryPreference, MissionPhase, Missions, Start as MissionStart,
};
use game_parallel_economy_family as family;
pub use game_parallel_economy_family::Families;
pub use game_parallel_economy_fatigue::Fatigue;
pub use game_parallel_economy_goals::{Dream, Dreams, Goal, Goals, Learning};
pub use game_parallel_economy_knowledge::{Knowledge, Misreport};
pub use game_parallel_economy_local_transport::goal::Preference as GoalFetchPreference;
pub use game_parallel_economy_local_transport::{
    Endpoint as LocalEndpoint, Event as LocalTransportEvent, LocalTransport,
    Policy as LocalTransportPolicy, Store as LocalStore,
};
use game_parallel_economy_person as person;
pub use game_parallel_economy_person::travel::{Colocation, Departure, Journey, Route, Travel};
pub use game_parallel_economy_promises::{
    Decision as PromiseDecision, Proposal as PromiseProposal,
};
pub use game_parallel_economy_skills::Skills;
pub use game_parallel_economy_taking::{
    Event as TakingEvent, Intent as TakingIntent, Interpretation as TakingInterpretation,
    Outcome as TakingOutcome, Policy as TakingPolicy, Reaction as TakingReaction,
    Reception as TakingReception, Report as TakingReport, ReportBelief as TakingReportBelief,
    ReportDetails as TakingReportDetails, ReportPolicy as TakingReportPolicy,
    ReportSource as TakingReportSource, Response as TakingResponse, Stance as TakingReportStance,
    Taking, Witness as TakingWitness,
};
pub use game_parallel_economy_work::care::DependentCare;
pub use game_parallel_economy_work::sites::{
    Binding as WorkSiteBinding, Facilities as WorkSites, Requirement as WorkSiteRequirement,
    Site as WorkSite,
};
pub use game_parallel_economy_work::tools::Requirement as WorkTool;
pub use game_parallel_fauna::{
    Animal, Fauna, Pregnancy as AnimalPregnancy, Species as AnimalSpecies,
};
pub use game_parallel_opinions::{
    ConfinementExperienceKind, ConfinementReaction, ObligationDevelopment, Opinion,
    RelationshipCase, ReportReception, Sensitivity as OpinionSensitivity, SubjectiveReaction,
    TeachingReaction, TeachingRole,
};
pub use game_parallel_practices::work::Evaluation as WorkPracticeEvaluation;
pub use game_parallel_practices::work::outcome::Policy as WorkOutcomePolicy;
pub use game_parallel_practices::{
    Belief as PracticeBelief, Case as PracticeCase, Decision as PracticeDecision,
    Mind as PracticeMind, Practices as PersonalPractices, PrecedentReasoning as PracticePrecedent,
    ReportAttitude as PracticeReportAttitude, Reporting as PracticeReporting,
    ValueLearning as PracticeValueLearning,
};
mod animal_care;
mod animal_feelings;
mod animal_handling;
mod animal_restraint_inheritance;
mod construction_planning;
pub use animal_care::{AnimalCare, Config as AnimalCareConfig, Policy as AnimalCarePolicy};
pub use animal_feelings::{
    AnimalFeelings, Mind as AnimalFeelingMind, Opinion as AnimalFeelingOpinion,
};
pub use animal_handling::{Handling as AnimalHandling, Policy as AnimalHandlingPolicy};
pub use game_parallel_animal_restraint::{
    DefensePolicy as AnimalDefensePolicy, Event as AnimalRestraintEvent,
    EventKind as AnimalRestraintEventKind, Hold as AnimalHold, Policy as AnimalCapturePolicy,
    ReleasePolicy as AnimalReleasePolicy, Restraints as AnimalRestraints,
};
pub use game_parallel_fauna::handling::{
    Event as AnimalHandlingEvent, Outcome as AnimalHandlingOutcome,
    Response as AnimalHandlingResponse,
};
pub use game_parallel_fauna::restraint::Response as AnimalRestraintResponse;
mod development;
mod environment;
mod food_search;
mod gathering;
mod giving;
mod giving_discovery;
mod giving_inheritance;
mod giving_practices;
mod hauling;
mod hunting;
mod medical;
mod mission_deliveries;
pub use food_search::{FoodSearch, Preference as FoodSearchPreference};
pub use giving::{
    Asking as FoodAsking, Giving, HelperAsking, Policy as GivingPolicy, Request as FoodRequest,
    Response as FoodResponse, SharingLearning,
};
pub use giving_discovery::Policy as AcquaintanceAsking;
pub use hauling::{
    Asking as HaulingAsking, Hauling, Policy as HaulingPolicy, Request as HaulingRequest,
};
pub use hunting::{
    Config as HuntingConfig, Hunting, Policy as HuntingPolicy,
    RequestResponse as HuntingRequestResponse,
};
pub use medical::{
    AcquaintancePolicy as MedicalAcquaintancePolicy, Medical, Policy as MedicalPolicy,
};
mod purpose;
mod recovery;
pub use game_parallel_purpose::Purpose;
pub use gathering::{Gathering, Node as ResourceNode, Policy as GatherPolicy};
pub use recovery::Rules as RecoveryRules;
mod state;
mod teaching;
use game_parallel_autowork::Autowork;
pub use game_parallel_economy_agriculture::{
    Agriculture, Crop, Field, Phase as CropPhase, Policy as FarmPolicy,
};
pub use game_parallel_environment::{Environment, Hazard, Season};
use game_parallel_lot_projection::agreement;
use game_parallel_lot_runtime::{AdvanceDraft, Events, Runtime};
use game_parallel_lots::Transaction;
use game_parallel_market::Market;
use game_parallel_nutrition::Nutrition;
use game_parallel_opinions::Opinions;
use game_parallel_production_model::FoodEffect;
use rayon::prelude::*;
pub use state::{Agreements, Immortal, Person, Rules, State};
use std::collections::HashSet;
pub struct Economy {
    physical: Runtime,
    seed: u64,
    people: Vec<Person>,
    rules: Rules,
    market: Option<Market>,
    agreements: Option<Agreements>,
    opinions: Option<Opinions>,
    autowork: Option<Autowork>,
    work_tools: Vec<WorkTool>,
    work_sites: Option<WorkSites>,
    construction: Option<Construction>,
    confinement: Option<Confinement>,
    goals: Option<Goals>,
    dreams: Option<Dreams>,
    nutrition: Option<Nutrition>,
    pending_food: Vec<FoodEffect>,
    travel: Option<Travel>,
    family: Option<Families>,
    care: Option<DependentCare>,
    agriculture: Option<Agriculture>,
    dynamics: Option<Dynamics>,
    missions: Option<Missions>,
    campaign: Option<Campaign>,
    skills: Option<Skills>,
    knowledge: Option<Knowledge>,
    combat: Option<game_parallel_economy_combat::Runtime>,
    fatigue: Option<Fatigue>,
    environment: Option<Environment>,
    purpose: Option<Purpose>,
    recovery: Option<RecoveryRules>,
    gathering: Option<Gathering>,
    fauna: Option<Fauna>,
    animal_care: Option<AnimalCare>,
    animal_handling: Option<AnimalHandling>,
    animal_restraints: Option<AnimalRestraints>,
    animal_feelings: Option<AnimalFeelings>,
    taking: Option<Taking>,
    hunting: Option<Hunting>,
    food_search: Option<FoodSearch>,
    giving: Option<Giving>,
    medical: Option<Medical>,
    hauling: Option<Hauling>,
    local_transport: Option<LocalTransport>,
    epoch: u64,
}
enum FaunaDraft {
    Complete(Fauna),
    Quiet {
        lifecycle: game_parallel_fauna::QuietDraft,
        handling: Vec<game_parallel_fauna::handling::Event>,
        reactions: Vec<game_parallel_fauna::restraint::Reaction>,
    },
}

pub struct Draft {
    epoch: u64,
    physical: AdvanceDraft,
    people: Vec<Person>,
    market: Option<Market>,
    agreements: Option<Agreements>,
    opinions: Option<Opinions>,
    autowork: Option<Autowork>,
    work_sites: Option<WorkSites>,
    construction: Option<Construction>,
    confinement: Option<Confinement>,
    goals: Option<Goals>,
    dreams: Option<Dreams>,
    nutrition: Option<Nutrition>,
    pending_food: Vec<FoodEffect>,
    travel: Option<Travel>,
    family: Option<Families>,
    care: Option<DependentCare>,
    agriculture: Option<Agriculture>,
    dynamics: Option<Dynamics>,
    missions: Option<Missions>,
    campaign: Option<Campaign>,
    skills: Option<Skills>,
    knowledge: Option<Knowledge>,
    combat: Option<game_parallel_economy_combat::Draft>,
    fatigue: Option<game_parallel_economy_fatigue::Draft>,
    purpose: Option<Purpose>,
    gathering: Option<Gathering>,
    fauna: Option<FaunaDraft>,
    animal_handling: Option<AnimalHandling>,
    animal_restraints: Option<AnimalRestraints>,
    taking: Option<Taking>,
    gathered: Option<bool>,
    food_search: Option<FoodSearch>,
    giving: Option<Giving>,
    hauling: Option<Hauling>,
    local_transport: Option<LocalTransport>,
    given: Option<bool>,
    treated_other: Option<bool>,
    fed_animal: Option<bool>,
    hunted: Option<bool>,
    handled: Option<bool>,
    animal_captured: Option<bool>,
    animal_released: Option<bool>,
    animal_defended: Option<bool>,
    taking_outcome: Option<TakingOutcome>,
    hunting_births: Vec<HuntingPolicy>,
    animal_care_births: Vec<AnimalCarePolicy>,
    animal_handling_births: Vec<AnimalHandlingPolicy>,
    animal_feelings_update: Option<AnimalFeelings>,
}
impl Draft {
    pub fn events(&self) -> &Events {
        self.physical.events()
    }
}
impl Economy {
    giving_practices::methods!();
    /// Change this person's willingness to build for their own production goals.
    pub fn set_goal_building_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.goal_builders
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no goal building preference")?
            .enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change this person's voluntary preference without changing felt pressure.
    pub fn set_demolition_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.demolition
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no demolition preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's willingness to recover debris without changing their feelings.
    pub fn set_salvage_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.salvaging
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no salvage preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change only this person's voluntary construction preference.
    pub fn set_construction_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .construction
            .clone()
            .ok_or("construction is disabled")?;
        next.policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no construction preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.construction = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Assemble an authored site blueprint using a real local or carried kit.
    pub fn build_site(&mut self, person: u64, blueprint: u32) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::Build { person, blueprint }),
        )?;
        self.commit(draft)?;
        Ok(self.construction.as_ref().is_some_and(|owner| {
            owner
                .events
                .iter()
                .any(|e| e.person == person && e.blueprint == blueprint && e.site.is_some())
        }))
    }
    /// Attempt to destroy an idle local site, without recovering its materials.
    pub fn destroy_site(&mut self, person: u64, site: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::DestroySite { person, site }),
        )?;
        self.commit(draft)?;
        Ok(self
            .work_sites
            .as_ref()
            .is_some_and(|sites| sites.sites.binary_search_by_key(&site, |s| s.id).is_err()))
    }
    /// Recover actual material from local debris into this person's custody.
    pub fn salvage_site(
        &mut self,
        person: u64,
        site: u64,
        good: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::SalvageSite {
                    person,
                    site,
                    good,
                    quantity,
                },
            ),
        )?;
        self.commit(draft)?;
        Ok(self.construction.as_ref().is_some_and(|owner| {
            owner
                .salvage_events
                .iter()
                .any(|event| event.person == person && event.site == site && event.recovered)
        }))
    }
    /// Change one person's willingness to carry goods between local stores.
    pub fn set_local_carrying_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .local_transport
            .clone()
            .ok_or("local transport is disabled")?;
        next.policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no local carrying preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.local_transport = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Choose whether to fetch locally observed supplies for this person's own goals.
    pub fn set_goal_fetch_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self
            .local_transport
            .clone()
            .ok_or("local transport is disabled")?;
        next.goal_fetchers
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no goal-fetching preference")?
            .enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.local_transport = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Move real goods between a controlled person's custody and a local store.
    pub fn move_local_store(
        &mut self,
        person: u64,
        store: u64,
        good: u32,
        quantity: u64,
        deposit: bool,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::LocalStore {
                    person,
                    store,
                    good,
                    quantity,
                    deposit,
                },
            ),
        )?;
        self.commit(draft)?;
        Ok(self.local_transport.as_ref().is_some_and(|owner| {
            owner
                .events
                .iter()
                .any(|event| event.person == person && event.moved)
        }))
    }
    pub fn set_hauling_asking_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.hauling.clone().ok_or("hauling is disabled")?;
        next.asking
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no hauling asking preference")?
            .enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.hauling = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative access; player observations must filter to involved people.
    pub fn hauling_requests(&self) -> &[HaulingRequest] {
        self.hauling
            .as_ref()
            .map_or(&[], |hauling| &hauling.requests)
    }
    /// Ask for ongoing transport help; the carrier remains free to refuse or stop.
    pub fn ask_haul(
        &mut self,
        person: u64,
        carrier: u64,
        source: u64,
        destination: u64,
        good: u32,
        lifetime: u64,
    ) -> Result<(), String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::AskHaul {
                person,
                carrier,
                source,
                destination,
                good,
                lifetime,
            }),
        )?;
        self.commit(draft)?;
        Ok(())
    }
    /// Change this person's inclination to haul; social pressure can still affect their choice.
    pub fn set_hauling_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.hauling.clone().ok_or("hauling is disabled")?;
        next.policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no hauling preference")?
            .willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.hauling = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative access; a player view must filter to their own requests.
    pub fn food_requests(&self) -> &[FoodRequest] {
        self.giving.as_ref().map_or(&[], |giving| &giving.requests)
    }
    /// Change one individual's interpretations without changing anyone else's feelings.
    pub fn set_opinion_sensitivity(
        &mut self,
        sensitivity: OpinionSensitivity,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&sensitivity.person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.opinions.clone().ok_or("opinions are disabled")?;
        if let Some(existing) = next
            .sensitivities_mut()
            .iter_mut()
            .find(|p| p.person == sensitivity.person)
        {
            *existing = sensitivity;
        } else {
            next.sensitivities_mut().push(sensitivity);
        }
        let people: std::collections::HashSet<_> = self.people.par_iter().map(|p| p.id).collect();
        next.validate(&people, self.tick()).map_err(str::to_owned)?;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.opinions = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change the person's report reception without changing their other dispositions.
    pub fn set_food_report_reception(
        &mut self,
        person: u64,
        reception: Option<ReportReception>,
    ) -> Result<(), String> {
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        let mut sensitivity = owner
            .sensitivities
            .iter()
            .find(|s| s.person == person)
            .cloned()
            .unwrap_or_else(|| OpinionSensitivity {
                person,
                ..Default::default()
            });
        sensitivity.report_reception = reception;
        self.set_opinion_sensitivity(sensitivity)
    }
    /// Choose this person's signed response to personally witnessed promise outcomes.
    pub fn set_promise_trust_reactions(
        &mut self,
        person: u64,
        reactions: Option<(i32, i32)>,
    ) -> Result<(), String> {
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        let mut sensitivity = owner
            .sensitivities
            .iter()
            .find(|s| s.person == person)
            .cloned()
            .unwrap_or_else(|| OpinionSensitivity {
                person,
                ..Default::default()
            });
        sensitivity.trust_promise_fulfilled = reactions.map(|row| row.0);
        sensitivity.trust_promise_unfulfilled = reactions.map(|row| row.1);
        self.set_opinion_sensitivity(sensitivity)
    }
    /// Choose this person's own response to a directly experienced event.
    pub fn set_confinement_reaction(
        &mut self,
        person: u64,
        kind: ConfinementExperienceKind,
        value: Option<SubjectiveReaction>,
    ) -> Result<(), String> {
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        let mut sensitivity = owner
            .sensitivities
            .iter()
            .find(|row| row.person == person)
            .cloned()
            .unwrap_or_else(|| OpinionSensitivity {
                person,
                ..Default::default()
            });
        let mut reaction = sensitivity.confinement_reaction.unwrap_or_default();
        match kind {
            ConfinementExperienceKind::Captured => reaction.captured = value.unwrap_or_default(),
            ConfinementExperienceKind::Resisted => reaction.resisted = value.unwrap_or_default(),
            ConfinementExperienceKind::Released => reaction.released = value.unwrap_or_default(),
        }
        sensitivity.confinement_reaction =
            (reaction != ConfinementReaction::default()).then_some(reaction);
        self.set_opinion_sensitivity(sensitivity)
    }
    pub fn confinement_reaction(&self, person: u64) -> Result<Option<ConfinementReaction>, String> {
        if self
            .people
            .binary_search_by_key(&person, |row| row.id)
            .is_err()
        {
            return Err("unknown person".into());
        }
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        Ok(owner
            .sensitivities
            .iter()
            .find(|row| row.person == person)
            .and_then(|row| row.confinement_reaction))
    }
    pub fn promise_trust_reactions(
        &self,
        person: u64,
    ) -> Result<(Option<i32>, Option<i32>), String> {
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        if !self.people.iter().any(|p| p.id == person) {
            return Err("unknown person".into());
        }
        Ok(owner
            .sensitivities
            .iter()
            .find(|s| s.person == person)
            .map_or((None, None), |s| {
                (s.trust_promise_fulfilled, s.trust_promise_unfulfilled)
            }))
    }
    pub fn food_report_reception(&self, person: u64) -> Result<Option<ReportReception>, String> {
        let owner = self.opinions.as_ref().ok_or("opinions are disabled")?;
        if !self.people.iter().any(|p| p.id == person) {
            return Err("unknown person".into());
        }
        Ok(owner
            .sensitivities
            .iter()
            .find(|s| s.person == person)
            .and_then(|s| s.report_reception))
    }
    /// Administrative relationship records; player views filter by observer.
    pub fn opinions(&self) -> &[Opinion] {
        self.opinions.as_ref().map_or(&[], |o| &o.records)
    }
    /// Administrative lookup; player observations expose only the current person's experience.
    pub fn sharing_learning(&self, person: u64) -> Option<&SharingLearning> {
        self.giving
            .as_ref()?
            .learning
            .iter()
            .find(|p| p.person == person)
    }
    /// Ask a nearby person for food; their response is their own later choice.
    pub fn ask_food(&mut self, person: u64, target: u64, lifetime: u64) -> Result<(), String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::AskFood {
                person,
                target,
                lifetime,
            }),
        )?;
        self.commit(draft)?;
        Ok(())
    }
    /// Change an individual's inclination to ask, without binding the recipient.
    pub fn set_food_asking_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.giving.clone().ok_or("food requests are disabled")?;
        let policy = next
            .asking
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no food asking preference")?;
        policy.enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.giving = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's own inclination to provide their configured care.
    pub fn set_animal_care_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.animal_care.clone().ok_or("animal care is disabled")?;
        let policy = next
            .policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no animal care preference")?;
        policy.willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.animal_care = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's own inclination to hunt without creating a duty.
    pub fn set_hunting_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.hunting.clone().ok_or("hunting is disabled")?;
        let policy = next
            .policies
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no hunting preference")?;
        policy.willing = willing;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.hunting = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's own inclination to provide their configured care.
    pub fn set_medical_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.medical.clone().ok_or("medical care is disabled")?;
        if let Some(policy) = next.policies.iter_mut().find(|p| p.person == person) {
            policy.willing = willing;
        } else if let Some(policy) = next.acquaintances.iter_mut().find(|p| p.person == person) {
            policy.willing = willing;
        } else {
            return Err("person has no medical preference".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.medical = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's own inclination to give under their existing preference.
    pub fn set_giving_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.giving.clone().ok_or("giving is disabled")?;
        if let Some(policy) = next.policies.iter_mut().find(|p| p.person == person) {
            policy.willing = willing;
        } else if let Some(policy) = next.responses.iter_mut().find(|p| p.person == person) {
            policy.willing = willing;
            if !willing {
                policy.affinity_threshold = None;
            }
        } else {
            return Err("person has no giving preference".into());
        }
        if !willing && let Some(learning) = next.learning.iter_mut().find(|p| p.person == person) {
            learning.enabled = false;
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.giving = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Decide whether this person acts on, and continues to build, their own sharing experience.
    pub fn set_sharing_learning_choice(
        &mut self,
        person: u64,
        enabled: bool,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.giving.clone().ok_or("giving is disabled")?;
        let learning = next
            .learning
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no sharing experience")?;
        learning.enabled = enabled;
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.giving = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Change one person's inclination to seek food in another area.
    pub fn set_food_search_choice(&mut self, person: u64, enabled: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.food_search.clone().ok_or("food search is disabled")?;
        if let Some(preference) = next.people.iter_mut().find(|p| p.person == person) {
            preference.enabled = enabled;
            if !enabled {
                preference.target = None;
            }
        } else {
            next.people.push(FoodSearchPreference {
                person,
                enabled,
                next_review: 0,
                target: None,
            });
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.food_search = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Administrative resource site access; player views must filter by location.
    pub fn resource_nodes(&self) -> &[ResourceNode] {
        self.gathering.as_ref().map_or(&[], |g| g.nodes.as_slice())
    }
    /// Administrative access; player views must restrict observations to the local area.
    pub fn animals(&self) -> &[Animal] {
        self.fauna
            .as_ref()
            .map_or(&[], |fauna| fauna.animals.as_slice())
    }
    /// The physically present handler of an animal, if any; this is not ownership.
    pub fn animal_handler(&self, animal: u64) -> Option<u64> {
        let owner = self.animal_restraints.as_ref()?;
        owner
            .holds
            .binary_search_by_key(&animal, |hold| hold.animal)
            .ok()
            .map(|index| owner.holds[index].handler)
    }
    pub fn animal_species(&self) -> &[AnimalSpecies] {
        self.fauna
            .as_ref()
            .map_or(&[], |fauna| fauna.species.as_slice())
    }
    /// Administrative access to a person's own feeling; absent means no configured mind.
    pub fn animal_affinity(&self, person: u64, animal: u64) -> Option<i32> {
        self.animal_feelings.as_ref()?.affinity(person, animal)
    }
    /// Change one person's own gathering preference and chosen site.
    pub fn set_gathering_choice(
        &mut self,
        person: u64,
        node: u64,
        willing: bool,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let mut next = self.gathering.clone().ok_or("gathering is disabled")?;
        if !next.nodes.iter().any(|site| site.id == node) {
            return Err("unknown resource site".into());
        }
        if let Some(policy) = next.policies.iter_mut().find(|p| p.person == person) {
            policy.node = node;
            policy.willing = willing;
        } else {
            next.policies.push(GatherPolicy {
                person,
                node,
                willing,
                pressured_by: None,
                pressure_threshold: None,
            });
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.gathering = Some(next);
        self.epoch = epoch;
        Ok(())
    }
    /// Attempt to gather from a local resource site and advance one boundary.
    pub fn gather(&mut self, person: u64, node: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::Gather { person, node }),
        )?;
        let admitted = draft.gathered.unwrap_or(false);
        self.commit(draft)?;
        Ok(admitted)
    }
    /// Give actual carried goods to a local person without requiring a promise.
    pub fn give(
        &mut self,
        person: u64,
        recipient: u64,
        good: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::Give {
                person,
                recipient,
                good,
                quantity,
            }),
        )?;
        let delivered = draft.given.unwrap_or(false);
        self.commit(draft)?;
        Ok(delivered)
    }
    /// Attempt to feed a nearby animal with real carried food.
    pub fn feed_animal(
        &mut self,
        person: u64,
        animal: u64,
        good: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::FeedAnimal {
                    person,
                    animal,
                    good,
                    quantity,
                },
            ),
        )?;
        let fed = draft.fed_animal.unwrap_or(false);
        self.commit(draft)?;
        Ok(fed)
    }
    /// Attempt to hunt a nearby animal; success requires admitted physical yield.
    pub fn hunt_animal(&mut self, person: u64, animal: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(game_parallel_economy_interaction::PhysicalChoice::HuntAnimal { person, animal }),
        )?;
        let hunted = draft.hunted.unwrap_or(false);
        self.commit(draft)?;
        Ok(hunted)
    }
    /// Attempt to take real possessions; opposition and escalation are separate choices.
    pub fn take_possessions(
        &mut self,
        person: u64,
        target: u64,
        good: u32,
        quantity: u64,
        escalate: bool,
    ) -> Result<TakingOutcome, String> {
        game_parallel_economy_campaign::preflight!(self);
        if !self.physical.goods().iter().any(|row| row.id == good) {
            return Err("unknown good".into());
        }
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::TakeFrom {
                    person,
                    target,
                    good,
                    quantity,
                    escalate,
                },
            ),
        )?;
        let outcome = draft
            .taking_outcome
            .ok_or("taking attempt was not selected")?;
        self.commit(draft)?;
        Ok(outcome)
    }
    /// Set one person's standing notice and resistance without implying approval.
    pub fn set_taking_response(
        &mut self,
        person: u64,
        notices: bool,
        resist: bool,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let mut owner = self.taking.clone().unwrap_or_default();
        let responses = owner.responses_mut();
        if let Some(response) = responses.iter_mut().find(|row| row.person == person) {
            response.notices = notices;
            response.resist = resist;
        } else {
            responses.push(TakingResponse {
                person,
                notices,
                resist,
                resist_if_fear_at_least: None,
                resist_if_affinity_at_least: None,
                resist_if_affinity_at_most: None,
            });
        }
        responses.par_sort_unstable_by_key(|row| row.person);
        self.taking = Some(owner);
        self.epoch = epoch;
        Ok(())
    }
    /// Change one person's attention and subjective interpretations of an actor.
    pub fn set_taking_witness(
        &mut self,
        person: u64,
        actor: u64,
        rule: Option<TakingWitness>,
    ) -> Result<(), String> {
        let exists = |id| self.people.binary_search_by_key(&id, |p| p.id).is_ok();
        if !exists(actor)
            || self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            || rule.as_ref().is_some_and(|r| {
                r.person != person
                    || r.actor != actor
                    || r.target
                        .is_some_and(|target| target == actor || !exists(target))
            })
        {
            return Err("invalid personal witnessing preference".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let mut owner = self.taking.clone().unwrap_or_default();
        let witnesses = owner.witnesses_mut();
        let remembered = witnesses
            .iter()
            .find(|row| (row.person, row.actor) == (person, actor))
            .and_then(|row| row.last_seen);
        witnesses.retain(|row| (row.person, row.actor) != (person, actor));
        if let Some(mut rule) = rule {
            // Preference changes cannot create or rewrite an observation.
            rule.last_seen = remembered;
            witnesses.push(rule);
        }
        witnesses.par_sort_unstable_by_key(|row| (row.person, row.actor));
        self.taking = Some(owner);
        self.epoch = epoch;
        Ok(())
    }
    /// Read only this person's preference for a particular actor.
    pub fn taking_witness(&self, person: u64, actor: u64) -> Option<&TakingWitness> {
        self.taking
            .as_ref()?
            .witnesses
            .iter()
            .find(|row| row.person == person && row.actor == actor)
    }
    /// Reports are private to their recipient, and contain statements rather than world facts.
    pub fn taking_reports(&self, person: u64) -> Vec<&TakingReport> {
        self.taking
            .as_ref()
            .map_or_else(Vec::new, |owner| owner.reports.get(person).iter().collect())
    }
    pub fn taking_report_policy(
        &self,
        person: u64,
        recipient: u64,
        actor: u64,
    ) -> Option<&TakingReportPolicy> {
        self.taking
            .as_ref()?
            .report_policies
            .iter()
            .find(|p| (p.person, p.recipient, p.actor) == (person, recipient, actor))
    }
    pub fn set_taking_report_policy(
        &mut self,
        person: u64,
        recipient: u64,
        actor: u64,
        policy: Option<TakingReportPolicy>,
    ) -> Result<(), String> {
        let exists = |id| self.people.binary_search_by_key(&id, |p| p.id).is_ok();
        if person == recipient
            || !exists(recipient)
            || !exists(actor)
            || self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .is_none_or(|i| self.people[i].health == 0)
            || policy.as_ref().is_some_and(|p| {
                (p.person, p.recipient, p.actor) != (person, recipient, actor)
                    || p.details.as_ref().is_some_and(|d| {
                        d.target.is_some_and(|id| id == actor || !exists(id))
                            || d.good
                                .is_some_and(|id| !self.physical.goods().iter().any(|g| g.id == id))
                            || d.quantity == Some(0)
                    })
                    || p.pressure_threshold == Some(0)
                    || p.heard_from.is_some_and(|id| id == person || !exists(id))
                    || (p.source == TakingReportSource::Observed && p.heard_from.is_some())
            })
        {
            return Err("invalid personal reporting preference".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let mut owner = self.taking.clone().unwrap_or_default();
        let policies = owner.report_policies_mut();
        policies.retain(|p| (p.person, p.recipient, p.actor) != (person, recipient, actor));
        if let Some(policy) = policy {
            policies.push(policy);
        }
        policies.par_sort_unstable_by_key(|p| (p.person, p.recipient, p.actor));
        self.taking = Some(owner);
        self.epoch = epoch;
        Ok(())
    }
    pub fn taking_reception(&self, person: u64) -> Option<&TakingReception> {
        self.taking
            .as_ref()?
            .receptions
            .iter()
            .find(|row| row.person == person)
    }
    pub fn set_taking_reception(
        &mut self,
        person: u64,
        rule: Option<TakingReception>,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
            || rule.as_ref().is_some_and(|r| r.person != person)
        {
            return Err("invalid personal report reception".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let mut owner = self.taking.clone().unwrap_or_default();
        let rows = owner.receptions_mut();
        rows.retain(|row| row.person != person);
        if let Some(rule) = rule {
            rows.push(rule);
        }
        rows.par_sort_unstable_by_key(|row| row.person);
        self.taking = Some(owner);
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's voluntary inclination without removing personal pressure.
    pub fn set_taking_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |p| p.id)
            .ok()
            .is_none_or(|i| self.people[i].health == 0)
        {
            return Err("person must be alive".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let mut owner = self.taking.clone().ok_or("taking is disabled")?;
        let policy = owner
            .policies_mut()
            .iter_mut()
            .find(|p| p.person == person)
            .ok_or("person has no taking preference")?;
        policy.willing = willing;
        self.taking = Some(owner);
        self.epoch = epoch;
        Ok(())
    }
    /// Approach a living animal; its own response determines whether handling succeeds.
    pub fn handle_animal(&mut self, person: u64, animal: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::HandleAnimal { person, animal },
            ),
        )?;
        let handled = draft.handled.unwrap_or(false);
        self.commit(draft)?;
        Ok(handled)
    }
    /// Attempt to physically restrain a local animal; it may resist or escape later.
    pub fn capture_animal(&mut self, person: u64, animal: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::CaptureAnimal { person, animal },
            ),
        )?;
        let captured = draft.animal_captured.unwrap_or(false);
        self.commit(draft)?;
        Ok(captured)
    }
    /// Release a local animal from a physical hold, without requiring its handler's consent.
    pub fn release_animal(&mut self, person: u64, animal: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::ReleaseAnimal { person, animal },
            ),
        )?;
        let released = draft.animal_released.unwrap_or(false);
        self.commit(draft)?;
        Ok(released)
    }
    /// Defend a physically held animal for one boundary; report whether the hold survived a blocked release.
    pub fn defend_animal(&mut self, person: u64, animal: u64) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::DefendAnimal { person, animal },
            ),
        )?;
        let defended = draft.animal_defended.unwrap_or(false);
        self.commit(draft)?;
        Ok(defended)
    }
    /// Change a person's capture inclination without erasing personal pressure.
    pub fn set_animal_capture_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .is_none_or(|index| self.people[index].health == 0)
        {
            return Err("person must be capable".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let owner = self
            .animal_restraints
            .as_mut()
            .ok_or("animal restraint is disabled")?;
        let index = owner
            .policies
            .binary_search_by_key(&person, |policy| policy.person)
            .map_err(|_| "person has no animal capture preference")?;
        owner.policies_mut()[index].willing = willing;
        self.epoch = epoch;
        Ok(())
    }
    /// Change a person's willingness to release an animal, retaining their pressure settings.
    pub fn set_animal_release_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        self.set_animal_restraint_choice(person, willing, false)
    }
    /// Change a handler's willingness to oppose release, retaining their pressure settings.
    pub fn set_animal_defense_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        self.set_animal_restraint_choice(person, willing, true)
    }
    fn set_animal_restraint_choice(
        &mut self,
        person: u64,
        willing: bool,
        defense: bool,
    ) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .is_none_or(|index| self.people[index].health == 0)
        {
            return Err("person must be capable".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let owner = self
            .animal_restraints
            .as_mut()
            .ok_or("animal restraint is disabled")?;
        if defense {
            let index = owner
                .defense_policies
                .binary_search_by_key(&person, |policy| policy.person)
                .map_err(|_| "person has no animal defense preference")?;
            owner.defense_policies_mut()[index].willing = willing;
        } else {
            let index = owner
                .release_policies
                .binary_search_by_key(&person, |policy| policy.person)
                .map_err(|_| "person has no animal release preference")?;
            owner.release_policies_mut()[index].willing = willing;
        }
        self.epoch = epoch;
        Ok(())
    }
    /// Change an individual's willingness without removing their personal pressure.
    pub fn set_animal_handling_choice(&mut self, person: u64, willing: bool) -> Result<(), String> {
        if self
            .people
            .binary_search_by_key(&person, |row| row.id)
            .ok()
            .is_none_or(|index| self.people[index].health == 0)
        {
            return Err("person must be capable".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        let owner = self
            .animal_handling
            .as_mut()
            .ok_or("animal handling is disabled")?;
        let index = owner
            .policies
            .binary_search_by_key(&person, |policy| policy.person)
            .map_err(|_| "person has no animal handling preference")?;
        owner.policies[index].willing = willing;
        self.epoch = epoch;
        Ok(())
    }
    /// Attempt treatment; the result reports whether medicine was consumed.
    pub fn treat_other(
        &mut self,
        person: u64,
        patient: u64,
        medicine: u32,
        quantity: u64,
    ) -> Result<bool, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare_interaction(
            None,
            None,
            None,
            Some(
                game_parallel_economy_interaction::PhysicalChoice::TreatOther {
                    person,
                    patient,
                    medicine,
                    quantity,
                },
            ),
        )?;
        let treated = draft.treated_other.unwrap_or(false);
        self.commit(draft)?;
        Ok(treated)
    }
    game_parallel_economy_read::read_method!();
    game_parallel_economy_promises::promise_methods!();
    game_parallel_economy_agriculture::cultivation_methods!();
    game_parallel_economy_person::work_choice_method!();
    game_parallel_economy_interaction::meal_method!();
    game_parallel_economy_interaction::transfer_method!();
    game_parallel_economy_interaction::fulfill_method!();
    game_parallel_economy_interaction::treat_method!();
    game_parallel_economy_claims_output::farm_method!();
    game_parallel_economy_fatigue::rest_method!();
    game_parallel_economy_interaction::combat_methods!();
    game_parallel_economy_goals::request_method!();
    game_parallel_economy_person::departure_method!();
    game_parallel_economy_confinement::escape_method!();
    game_parallel_economy_dynamics::social_choice_method!();
    game_parallel_economy_claims::claim_methods!();
    game_parallel_economy_expeditions::mission_methods!();
    game_parallel_economy_campaign::campaign_methods!();
    game_parallel_economy_interaction::interaction_methods!();
    game_parallel_economy_interaction::work_methods!();
    game_parallel_economy_knowledge::knowledge_methods!();
    pub fn compile(mut state: State) -> Result<Self, String> {
        state.people.par_sort_unstable_by_key(|row| row.id);
        if let Some(travel) = &mut state.travel {
            travel
                .journeys
                .par_sort_unstable_by_key(|journey| journey.person);
        }
        state
            .work_tools
            .par_sort_unstable_by_key(|row| (row.recipe, row.good));
        if state.rules.carrying_capacity == 0
            || state.rules.max_health == 0
            || state.rules.meal_threshold == 0
            || state
                .people
                .par_windows(2)
                .any(|rows| rows[0].id == rows[1].id)
            || state.people.par_iter().any(|person| {
                person.id == 0
                    || person.region == Some(0)
                    || person.health > state.rules.max_health
                    || person
                        .immortal
                        .as_ref()
                        .is_some_and(|kind| kind.recovery_per_tick == 0 || person.hunger != 0)
            })
        {
            return Err("invalid personal economic state".into());
        }
        let physical = Runtime::compile(state.physical)?;
        let people: Vec<_> = state
            .people
            .par_iter()
            .map(|row| (row.id, row.region, row.health, row.hunger))
            .collect();
        let ids: HashSet<_> = people.iter().map(|row| row.0).collect();
        game_parallel_economy_fatigue::validate_world!(state, ids);
        if state
            .recovery
            .as_ref()
            .is_some_and(|rules| rules.interval == 0)
        {
            return Err("invalid recovery rules".into());
        }
        if let Some(purpose) = &state.purpose {
            purpose.validate(&ids).map_err(str::to_owned)?;
        }
        let goods: HashSet<_> = physical.goods().iter().map(|row| row.id).collect();
        if let Some(taking) = &state.taking {
            taking
                .validate(&ids, &goods, physical.tick())
                .map_err(str::to_owned)?;
        }
        game_parallel_economy_skills::validate_world!(state, physical, ids);
        let view = physical.view(physical.tick())?;
        environment::validate(state.environment.as_ref(), &view, &goods)?;
        if let Some(gathering) = &state.gathering {
            gathering.validate(&ids, &goods, &view)?;
        }
        if let Some(fauna) = &state.fauna {
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|holder| match holder.place {
                    game_parallel_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            fauna
                .validate(&regions, &goods, physical.tick())
                .map_err(str::to_owned)?;
            if fauna.animals.par_iter().any(|animal| {
                animal
                    .opinions
                    .iter()
                    .any(|opinion| !ids.contains(&opinion.person))
            }) {
                return Err("animal opinion references an unknown person".into());
            }
        }
        if let Some(care) = &state.animal_care {
            care.validate(&ids, &goods, state.fauna.as_ref())?;
        }
        if let Some(handling) = &mut state.animal_handling {
            handling.validate(&ids, state.fauna.as_ref(), physical.tick())?;
        }
        if let Some(feelings) = &mut state.animal_feelings {
            feelings.normalize_and_validate(&ids, state.fauna.as_ref())?;
        }
        if let Some(hunting) = &state.hunting {
            hunting.validate(&ids, &goods, state.fauna.as_ref())?;
        }
        if let Some(search) = &state.food_search {
            search.validate(&ids, state.knowledge.as_ref(), state.travel.as_ref())?;
        }
        if let Some(giving) = &state.giving {
            giving.validate(&ids, physical.goods(), physical.recipes(), physical.tick())?;
            let requests = state
                .autowork
                .as_ref()
                .and_then(|auto| auto.requests.as_ref());
            if giving.practices.as_ref().is_some_and(|practices| {
                practices.minds.par_iter().any(|mind| {
                    mind.work.as_ref().is_some_and(|work| {
                        work.engagement
                            .iter()
                            .map(|entry| entry.request)
                            .chain(work.experiences.iter().map(|entry| entry.request))
                            .any(|request| {
                                requests.is_none_or(|requests| request >= requests.next_id)
                            })
                    })
                })
            }) {
                return Err("work practice references an unknown request".into());
            }
        }
        if let Some(hauling) = &state.hauling {
            hauling.validate(&ids, &goods, &view, state.travel.as_ref())?;
        }
        if let Some(local) = &mut state.local_transport {
            local.normalize();
            local.validate(&ids, &goods, &view, physical.tick())?;
        } else if view
            .containers
            .par_iter()
            .any(|row| matches!(row.place, game_parallel_lots::Place::Inventory(_)))
        {
            return Err("local store custody requires location metadata".into());
        }
        if let Some(medical) = &state.medical {
            medical.validate(
                &ids,
                &goods,
                state.dynamics.as_ref().and_then(|d| d.disease.as_ref()),
                state.dynamics.as_ref().is_some_and(|d| d.social.is_some()),
            )?;
        }
        let combat = game_parallel_economy_combat::compile_world!(state, physical, view);
        game_parallel_economy_knowledge::validate_world!(state, ids, view, physical);
        game_parallel_economy_dynamics::validate_world!(state, ids, goods, view, physical);
        game_parallel_economy_expeditions::validate_world!(state, physical, view, ids, goods);
        let _ = game_parallel_lot_projection::cargo(&view, state.rules.carrying_capacity)?;
        family::validate_custody!(people, view, ids, physical);
        if let Some(travel) = &state.travel {
            person::travel::validate_world(travel, &state.people, &view, physical.work())?;
        }
        family::validate_care!(state, physical, ids);
        game_parallel_economy_campaign::validate_world!(state, physical, view);
        game_parallel_economy_agriculture::validate_world!(state, physical, view);
        if let Some(market) = &state.market {
            market.validate_index(&people, &goods, physical.tick())?;
        }
        if let Some(agreements) = &state.agreements {
            agreements
                .validate(&ids, &goods, physical.tick())
                .map_err(str::to_owned)?;
            if agreements
                .offering
                .par_iter()
                .any(|p| p.after_work_recipe.is_some())
            {
                let recipes: HashSet<_> = physical.recipes().par_iter().map(|r| r.id).collect();
                if agreements
                    .offering
                    .par_iter()
                    .any(|p| p.after_work_recipe.is_some_and(|id| !recipes.contains(&id)))
                {
                    return Err("unknown remembered-work recipe".into());
                }
            }
        }
        if let Some(opinions) = &state.opinions {
            opinions
                .validate(&ids, physical.tick())
                .map_err(str::to_owned)?;
        }
        if let Some(nutrition) = &state.nutrition {
            nutrition.validate(&ids).map_err(str::to_owned)?;
        }
        game_parallel_lot_diet::validate_pending(&state.pending_food, &ids, physical.tick())?;
        game_parallel_economy_goals::validate(
            state.goals.as_ref(),
            state.dreams.as_ref(),
            state.autowork.is_some(),
            &ids,
            physical.recipes(),
            physical.tick(),
        )?;
        if let Some(sites) = &mut state.work_sites {
            sites.normalize()?;
            let recipes: HashSet<_> = physical.recipes().par_iter().map(|r| r.id).collect();
            let regions: HashSet<_> = view
                .containers
                .par_iter()
                .filter_map(|c| match c.place {
                    game_parallel_lots::Place::Stock { region, .. } => Some(region),
                    _ => None,
                })
                .collect();
            sites.validate(physical.work(), &recipes, &regions)?;
        }
        if let Some(confinement) = &mut state.confinement {
            confinement.normalize();
            confinement.validate(
                &state.people,
                state
                    .work_sites
                    .as_ref()
                    .ok_or("confinement requires physical sites")?,
                physical.tick(),
            )?;
        }
        if let Some(restraints) = &mut state.animal_restraints {
            restraints.normalize_and_validate(
                &state.people,
                state.fauna.as_ref(),
                physical.tick(),
            )?;
            if restraints.holds.par_iter().any(|hold| {
                state
                    .confinement
                    .as_ref()
                    .is_some_and(|owner| owner.held(hold.handler))
            }) {
                return Err("an animal handler cannot be held behind a barrier".into());
            }
        }
        if let Some(claims) = state.dynamics.as_ref().and_then(|d| d.claims.as_ref())
            && (claims.site_claims.par_iter().any(|claim| {
                state
                    .work_sites
                    .as_ref()
                    .is_none_or(|sites| claim.site >= sites.next_id)
            }) || claims.site_events.par_iter().any(|event| {
                state
                    .work_sites
                    .as_ref()
                    .is_none_or(|sites| event.site >= sites.next_id)
            }))
        {
            return Err("invalid personal site claim".into());
        }
        if let Some(owner) = &mut state.construction {
            owner.normalize();
            let sites = state
                .work_sites
                .as_ref()
                .ok_or("construction requires work sites")?;
            owner.validate(&ids, &goods, sites, &view, physical.tick())?;
        } else if view
            .containers
            .par_iter()
            .any(|c| matches!(c.place, game_parallel_lots::Place::Site(_)))
        {
            return Err("site custody requires construction metadata".into());
        }
        if !state.work_tools.is_empty() {
            let recipes: HashSet<_> = physical.recipes().par_iter().map(|r| r.id).collect();
            game_parallel_economy_work::tools::validate(&state.work_tools, &recipes, &goods)?;
        }
        if let Some(auto) = &state.autowork {
            let production = game_parallel_lot_projection::production(
                &physical,
                &view,
                state.rules.max_work_hunger,
            );
            auto.validate(&people, &production)?;
            if auto.requests.as_ref().is_some_and(|requests| {
                requests
                    .memories
                    .par_iter()
                    .any(|m| m.last_success_at.is_some_and(|t| t > physical.tick()))
            }) {
                return Err("work memory is from the future".into());
            }
            game_parallel_economy_work::validate_links(auto, physical.work())?;
        }
        Ok(Self {
            physical,
            seed: state.seed,
            people: state.people,
            rules: state.rules,
            market: state.market,
            agreements: state.agreements,
            opinions: state.opinions,
            autowork: state.autowork,
            work_tools: state.work_tools,
            work_sites: state.work_sites,
            construction: state.construction,
            confinement: state.confinement,
            goals: state.goals,
            dreams: state.dreams,
            nutrition: state.nutrition,
            pending_food: state.pending_food,
            travel: state.travel,
            family: state.family,
            care: state.care,
            agriculture: state.agriculture,
            dynamics: state.dynamics,
            missions: state.missions,
            campaign: state.campaign,
            skills: state.skills,
            knowledge: state.knowledge,
            combat,
            fatigue: state.fatigue,
            environment: state.environment,
            purpose: state.purpose,
            recovery: state.recovery,
            gathering: state.gathering,
            fauna: state.fauna,
            animal_care: state.animal_care,
            animal_handling: state.animal_handling,
            animal_restraints: state.animal_restraints,
            animal_feelings: state.animal_feelings,
            taking: state.taking,
            hunting: state.hunting,
            food_search: state.food_search,
            giving: state.giving,
            medical: state.medical,
            hauling: state.hauling,
            local_transport: state.local_transport,
            epoch: 0,
        })
    }
    pub fn snapshot(&self) -> State {
        State {
            seed: self.seed,
            physical: self.physical.snapshot(),
            people: self.people.clone(),
            rules: self.rules.clone(),
            market: self.market.clone(),
            agreements: self.agreements.clone(),
            opinions: self.opinions.clone(),
            autowork: self.autowork.clone(),
            work_tools: self.work_tools.clone(),
            work_sites: self.work_sites.clone(),
            construction: self.construction.clone(),
            confinement: self.confinement.clone(),
            goals: self.goals.clone(),
            dreams: self.dreams.clone(),
            nutrition: self.nutrition.clone(),
            pending_food: self.pending_food.clone(),
            travel: self.travel.clone(),
            family: self.family.clone(),
            care: self.care.clone(),
            agriculture: self.agriculture.clone(),
            dynamics: self.dynamics.clone(),
            missions: self.missions.clone(),
            campaign: self.campaign.clone(),
            skills: self.skills.clone(),
            knowledge: self.knowledge.clone(),
            combat: self.combat.as_ref().map(|combat| combat.state.clone()),
            fatigue: self.fatigue.clone(),
            environment: self.environment.clone(),
            purpose: self.purpose.clone(),
            recovery: self.recovery.clone(),
            gathering: self.gathering.clone(),
            fauna: self.fauna.clone(),
            animal_care: self.animal_care.clone(),
            animal_handling: self.animal_handling.clone(),
            animal_restraints: self.animal_restraints.clone(),
            animal_feelings: self.animal_feelings.clone(),
            taking: self.taking.clone(),
            hunting: self.hunting.clone(),
            food_search: self.food_search.clone(),
            giving: self.giving.clone(),
            medical: self.medical.clone(),
            hauling: self.hauling.clone(),
            local_transport: self.local_transport.clone(),
        }
    }
    pub fn tick(&self) -> u64 {
        self.physical.tick()
    }
    pub fn prepare(&self) -> Result<Draft, String> {
        self.prepare_interaction(None, None, None, None)
    }
    fn prepare_interaction(
        &self,
        request: Option<game_parallel_economy_interaction::Request>,
        player_work: Option<game_parallel_economy_interaction::WorkAction>,
        player_meal: Option<person::meal::Choice>,
        player_transfer: Option<game_parallel_economy_interaction::PhysicalChoice>,
    ) -> Result<Draft, String> {
        #[cfg(feature = "profile")]
        let mut profile_phase = game_diagnostics::span("economy.setup");
        let tick = self.tick().checked_add(1).ok_or("tick overflow")?;
        if let Some(owner) = &self.confinement {
            let restricted = request.is_some_and(|row| !owner.can_communicate(row.actor, row.target))
                || player_work.is_some_and(|row| owner.held(row.actor()))
                || player_transfer.is_some_and(|row| {
                    owner.held(row.person())
                        && !(matches!(
                            row,
                            game_parallel_economy_interaction::PhysicalChoice::Rest { .. }
                                | game_parallel_economy_interaction::PhysicalChoice::Treat { .. }
                                | game_parallel_economy_interaction::PhysicalChoice::Escape { .. }
                        ) || matches!(
                            row,
                            game_parallel_economy_interaction::PhysicalChoice::AskFood { .. }
                                | game_parallel_economy_interaction::PhysicalChoice::Give { .. }
                                | game_parallel_economy_interaction::PhysicalChoice::TakeFrom { .. }
                                | game_parallel_economy_interaction::PhysicalChoice::TreatOther { .. }
                        ))
                })
                || player_transfer.is_some_and(|row| match row {
                    game_parallel_economy_interaction::PhysicalChoice::Recover {
                        deceased, ..
                    } => owner.held(deceased),
                    game_parallel_economy_interaction::PhysicalChoice::Give {
                        person,
                        recipient,
                        ..
                    } => !owner.can_communicate(person, recipient),
                    game_parallel_economy_interaction::PhysicalChoice::TreatOther {
                        person, patient, ..
                    } => !owner.can_touch(person, patient),
                    game_parallel_economy_interaction::PhysicalChoice::TakeFrom { person, target, .. } => !owner.can_touch(person, target),
                    game_parallel_economy_interaction::PhysicalChoice::AskFood {
                        person,
                        target,
                        ..
                    } => !owner.can_communicate(person, target),
                    game_parallel_economy_interaction::PhysicalChoice::AskHaul {
                        carrier, ..
                    } => owner.held(carrier),
                    _ => false,
                });
            if restricted {
                return Err("person is held behind a physical barrier".into());
            }
        }
        let (player_work, manual_response) = match player_work {
            Some(game_parallel_economy_interaction::WorkAction::Respond {
                actor,
                request,
                accept,
            }) => {
                let person = self
                    .people
                    .binary_search_by_key(&actor, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])
                    .ok_or("unknown person")?;
                let row = self
                    .autowork
                    .as_ref()
                    .and_then(|a| a.requests.as_ref())
                    .and_then(|r| {
                        r.pending
                            .iter()
                            .find(|r| r.id == request && r.worker == actor)
                    })
                    .filter(|r| r.expires >= tick && person.region == Some(r.region))
                    .ok_or("unknown or unavailable personal work request")?;
                if !person.controlled || person.health == 0 {
                    return Err("work response requires a living controlled person".into());
                }
                let action =
                    accept.then_some(game_parallel_economy_interaction::WorkAction::Start {
                        actor,
                        recipe: row.recipe,
                        units: 1,
                    });
                (action, Some((row.clone(), accept)))
            }
            other => (other, None),
        };
        let mut campaign_draft = game_parallel_economy_campaign::prepare_world!(self);
        let view = self.physical.view(tick)?;
        let destroy_site =
            if let Some(game_parallel_economy_interaction::PhysicalChoice::DestroySite {
                person,
                site,
            }) = player_transfer
            {
                let actor = self
                    .people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])
                    .ok_or("unknown person")?;
                let sites = self.work_sites.as_ref().ok_or("work sites are disabled")?;
                let target = sites
                    .sites
                    .binary_search_by_key(&site, |s| s.id)
                    .ok()
                    .map(|i| &sites.sites[i])
                    .ok_or("unknown work site")?;
                if !actor.controlled || actor.health == 0 || actor.region != Some(target.region) {
                    return Err(
                        "site destruction requires a living controlled person at the site".into(),
                    );
                }
                if self.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                }) {
                    return Err("person is too young to destroy a site".into());
                }
                Some((
                    site,
                    sites
                        .bindings
                        .par_iter()
                        .any(|binding| binding.site == site),
                ))
            } else {
                None
            };
        let claims = game_parallel_economy_claims::prepare_index!(self);
        let site_classes = claims
            .as_ref()
            .map_or_else(Vec::new, |index| index.site_classes());
        let manual_taking = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::TakeFrom {
                person,
                target,
                good,
                quantity,
                escalate,
            }) => Some(TakingIntent {
                actor: person,
                target,
                good,
                quantity,
                escalate,
            }),
            _ => None,
        };
        let default_taking = Taking::default();
        #[cfg(feature = "profile")]
        let taking_phase = game_diagnostics::span("economy.setup.taking");
        let mut taking_plan = if self.taking.is_some() || manual_taking.is_some() {
            let mut taking_busy =
                game_parallel_economy_fatigue::resting(self.fatigue.as_ref(), &self.people);
            taking_busy.par_extend(
                self.physical
                    .work()
                    .par_iter()
                    .filter(|row| row.phase == game_parallel_lot_runtime::Phase::Working)
                    .map(|row| row.job.worker),
            );
            taking_busy.par_sort_unstable();
            taking_busy.dedup();
            if manual_taking.is_some_and(|intent| taking_busy.binary_search(&intent.actor).is_ok())
            {
                return Err("a busy person cannot attempt to take possessions".into());
            }
            let mut taking_blocked = self
                .combat
                .as_ref()
                .map_or_else(Vec::new, |combat| combat.actors().to_vec());
            if let Some(row) = request {
                taking_blocked.extend([row.actor, row.target]);
            }
            taking_blocked.extend(player_work.map(|row| row.actor()));
            taking_blocked.extend(player_meal.map(|row| row.person));
            if let Some(choice) = player_transfer
                && manual_taking.is_none()
            {
                use game_parallel_economy_interaction::PhysicalChoice;
                taking_blocked.push(choice.person());
                match choice {
                    PhysicalChoice::Give { recipient, .. } => taking_blocked.push(recipient),
                    PhysicalChoice::TreatOther { patient, .. } => taking_blocked.push(patient),
                    PhysicalChoice::AskFood { target, .. }
                    | PhysicalChoice::Confine { target, .. }
                    | PhysicalChoice::ReleaseHeld { target, .. } => taking_blocked.push(target),
                    PhysicalChoice::AskHaul { carrier, .. } => taking_blocked.push(carrier),
                    _ => {}
                }
            }
            taking_blocked.par_sort_unstable();
            taking_blocked.dedup();
            let locations = Colocation::new(self.travel.as_ref());
            let can_touch = |a, b| {
                self.confinement
                    .as_ref()
                    .is_none_or(|owner| owner.can_touch(a, b))
            };
            Some(
                game_parallel_economy_taking::Context {
                    owner: self.taking.as_ref().unwrap_or(&default_taking),
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    locations: &locations,
                    busy: &taking_busy,
                    blocked: &taking_blocked,
                    can_touch: &can_touch,
                    max_hunger: self.rules.max_work_hunger,
                    manual: manual_taking,
                    action_base: 0,
                    seed: self.seed,
                    tick,
                }
                .prepare()?,
            )
        } else {
            None
        };
        #[cfg(feature = "profile")]
        drop(taking_phase);
        let requested_contests: Vec<_> = taking_plan
            .as_ref()
            .map_or(&[][..], |plan| plan.contests.as_slice())
            .par_iter()
            .map(|contest| (contest.actor, contest.target, contest.region))
            .collect();
        let taking_confrontation = game_parallel_economy_combat::confrontation::select_requested(
            self.combat.as_ref(),
            &self.people,
            &requested_contests,
        )?;
        let confrontation = if self.combat.is_some()
            && self.opinions.as_ref().is_some_and(|opinions| {
                opinions.sensitivities.par_iter().any(|row| {
                    row.confrontation.as_ref().is_some_and(|policy| {
                        policy.enabled && tick.is_multiple_of(policy.interval)
                    })
                })
            }) {
            #[cfg(feature = "profile")]
            let _confrontation_span = game_diagnostics::span("economy.confrontation_select");
            let mut unable_to_initiate =
                game_parallel_economy_fatigue::resting(self.fatigue.as_ref(), &self.people);
            unable_to_initiate.par_extend(
                self.physical
                    .work()
                    .par_iter()
                    .filter(|row| row.phase == game_parallel_lot_runtime::Phase::Working)
                    .map(|row| row.job.worker),
            );
            unable_to_initiate.par_sort_unstable();
            unable_to_initiate.dedup();
            let mut unavailable = Vec::new();
            if let Some(combat) = &self.combat {
                unavailable.extend_from_slice(combat.actors());
            }
            if let Some(plan) = &taking_plan {
                unavailable.extend_from_slice(&plan.actors);
            }
            if let Some(confinement) = &self.confinement {
                unavailable.extend(confinement.holds.iter().map(|hold| hold.person));
            }
            if let Some(row) = request {
                unavailable.extend([row.actor, row.target]);
            }
            unavailable.extend(player_work.map(|row| row.actor()));
            unavailable.extend(player_meal.map(|row| row.person));
            unavailable.extend(player_transfer.map(|row| row.person()));
            if let Some((row, _)) = &manual_response {
                unavailable.push(row.worker);
            }
            if let Some(choice) = player_transfer {
                use game_parallel_economy_interaction::PhysicalChoice;
                match choice {
                    PhysicalChoice::Give { recipient, .. } => unavailable.push(recipient),
                    PhysicalChoice::TreatOther { patient, .. } => unavailable.push(patient),
                    PhysicalChoice::AskFood { target, .. } => unavailable.push(target),
                    PhysicalChoice::AskHaul { carrier, .. } => unavailable.push(carrier),
                    PhysicalChoice::Confine { target, .. } => unavailable.push(target),
                    _ => {}
                }
            }
            unavailable.par_sort_unstable();
            unavailable.dedup();
            game_parallel_economy_combat::confrontation::select(
                self.combat.as_ref(),
                self.opinions.as_ref(),
                &self.people,
                &unavailable,
                &unable_to_initiate,
                tick,
            )
        } else {
            None
        };
        let confrontation = game_parallel_economy_combat::confrontation::combine(
            confrontation,
            taking_confrontation,
        )?;
        let current_fighters = self
            .combat
            .as_ref()
            .map_or(&[][..], |combat| combat.actors());
        let combined_fighters = confrontation.as_ref().map(|selected| {
            let mut fighters = current_fighters.to_vec();
            fighters.extend_from_slice(&selected.actors);
            fighters.par_sort_unstable();
            fighters.dedup();
            fighters
        });
        let initial_fighters = combined_fighters.as_deref().unwrap_or(current_fighters);
        #[cfg(feature = "profile")]
        let people_phase = game_diagnostics::span("economy.setup.people");
        let (fighters, resting, people, meal_candidates, meals, mut busy) = family::prepare_people!(
            self,
            view,
            request,
            player_work,
            player_meal,
            player_transfer,
            initial_fighters,
            taking_plan
                .as_ref()
                .map_or(&[][..], |plan| plan.actors.as_slice())
        );
        #[cfg(feature = "profile")]
        drop(people_phase);
        let mut held: Vec<_> = self
            .confinement
            .as_ref()
            .map_or(&[][..], |owner| owner.holds.as_slice())
            .par_iter()
            .map(|hold| hold.person)
            .collect();
        let manual_guard = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Guard { person, site }) => {
                Some((person, site))
            }
            _ => None,
        };
        if manual_guard.is_some() && self.confinement.is_none() {
            return Err("confinement is disabled".into());
        }
        if manual_guard.is_some_and(|(person, _)| {
            self.physical.work().iter().any(|job| {
                job.job.worker == person && job.phase == game_parallel_lot_runtime::Phase::Working
            })
        }) {
            return Err("a working person cannot guard a cell".into());
        }
        let guard_plan = if let Some(owner) = self
            .confinement
            .as_ref()
            .filter(|owner| manual_guard.is_some() || !owner.guard_policies.is_empty())
        {
            let mut guard_busy = busy.clone();
            if let Some(row) = request {
                guard_busy.extend([row.actor, row.target]);
            }
            if let Some(choice) = player_transfer {
                use game_parallel_economy_interaction::PhysicalChoice;
                let target = match choice {
                    PhysicalChoice::Give { recipient, .. } => Some(recipient),
                    PhysicalChoice::TreatOther { patient, .. } => Some(patient),
                    PhysicalChoice::AskFood { target, .. }
                    | PhysicalChoice::Confine { target, .. }
                    | PhysicalChoice::ReleaseHeld { target, .. } => Some(target),
                    PhysicalChoice::AskHaul { carrier, .. } => Some(carrier),
                    _ => None,
                };
                guard_busy.extend(target);
            }
            guard_busy.par_sort_unstable();
            guard_busy.dedup();
            Some(
                game_parallel_economy_confinement::GuardContext {
                    owner,
                    people: &self.people,
                    sites: self
                        .work_sites
                        .as_ref()
                        .ok_or("confinement requires physical sites")?,
                    busy: &guard_busy,
                    opinions: self.opinions.as_ref(),
                    manual: manual_guard,
                    tick,
                }
                .prepare()?,
            )
        } else {
            None
        };
        if let Some(plan) = &guard_plan
            && !plan.actors.is_empty()
        {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let manual_escape = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Escape { person }) => {
                Some(person)
            }
            _ => None,
        };
        if manual_escape.is_some() && self.confinement.is_none() {
            return Err("confinement is disabled".into());
        }
        let confinement_busy = taking_plan
            .as_ref()
            .filter(|plan| !plan.targets.is_empty())
            .map(|plan| {
                let mut rows = busy.clone();
                game_parallel_economy_work::reserve(&mut rows, plan.targets.iter().copied());
                rows
            });
        let (mut next_confinement, mut escape_actors, escape_efforts) = self
            .confinement
            .as_ref()
            .map(|owner| {
                owner.escape_attempts(
                    &self.people,
                    confinement_busy.as_deref().unwrap_or(&busy),
                    tick,
                    manual_escape,
                    guard_plan
                        .as_ref()
                        .map_or(&[][..], |plan| plan.strengths.as_slice()),
                )
            })
            .transpose()?
            .map_or_else(
                || (None, Vec::new(), Vec::new()),
                |(next, actors, efforts)| (Some(next), actors, efforts),
            );
        if escape_actors.par_windows(2).any(|pair| pair[0] > pair[1]) {
            escape_actors.par_sort_unstable();
        }
        // Escape-only guards resist first, leaving flexible guards' unused effort
        // available for opening attempts without counting any effort twice.
        let mut opening_guards: Vec<_> = guard_plan.as_ref().map_or_else(Vec::new, |plan| {
            plan.opening_strengths
                .par_iter()
                .filter_map(|&(site, strength)| {
                    let escape = escape_efforts
                        .binary_search_by_key(&site, |row| row.0)
                        .ok()
                        .map_or(0, |i| escape_efforts[i].1);
                    let escape_only = plan
                        .escape_only_strengths
                        .binary_search_by_key(&site, |row| row.0)
                        .ok()
                        .map_or(0, |i| plan.escape_only_strengths[i].1);
                    let remaining = strength.saturating_sub(escape.saturating_sub(escape_only));
                    (remaining > 0).then_some((site, remaining))
                })
                .collect()
        });
        if let (Some(owner), Some(plan)) = (&mut next_confinement, guard_plan) {
            owner.events.par_extend(plan.events);
            if owner.events.par_windows(2).any(|pair| {
                (pair[0].person, pair[0].actor, pair[0].kind)
                    > (pair[1].person, pair[1].actor, pair[1].kind)
            }) {
                owner
                    .events
                    .par_sort_unstable_by_key(|event| (event.person, event.actor, event.kind));
            }
        }
        match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Confine {
                person,
                target,
                site,
            }) => {
                if self.physical.work().iter().any(|job| {
                    job.job.worker == person
                        && job.phase == game_parallel_lot_runtime::Phase::Working
                }) {
                    return Err("a working person cannot confine someone".into());
                }
                let owner = next_confinement.as_mut().ok_or("confinement is disabled")?;
                let pressured = owner
                    .capture_responses
                    .binary_search_by_key(&target, |rule| rule.person)
                    .ok()
                    .and_then(|i| owner.capture_responses[i].pressure_threshold)
                    .is_some_and(|threshold| {
                        self.opinions.as_ref().is_some_and(|opinions| {
                            let (fear, obligation) = opinions.index().feelings(target, person);
                            fear.saturating_add(obligation) >= threshold
                        })
                    });
                let captured = owner.capture(
                    game_parallel_economy_confinement::Capture {
                        actor: person,
                        target,
                        site,
                    },
                    &self.people,
                    self.work_sites
                        .as_ref()
                        .ok_or("confinement requires physical sites")?,
                    &busy,
                    pressured,
                    tick,
                )?;
                if captured && held.binary_search(&target).is_err() {
                    held.push(target);
                    held.par_sort_unstable();
                }
            }
            Some(game_parallel_economy_interaction::PhysicalChoice::ReleaseHeld {
                person,
                target,
            }) => {
                if self.physical.work().iter().any(|job| {
                    job.job.worker == person
                        && job.phase == game_parallel_lot_runtime::Phase::Working
                }) {
                    return Err("a working person cannot release someone".into());
                }
                let owner = next_confinement.as_mut().ok_or("confinement is disabled")?;
                if owner.held(target) {
                    let site = owner.site_of(target).ok_or("person is not held")?;
                    let guard = opening_guards.binary_search_by_key(&site, |row| row.0).ok();
                    let opposition = guard.map_or(0, |i| opening_guards[i].1);
                    let effort = owner.person_effort(person);
                    owner.release(
                        person,
                        target,
                        &self.people,
                        self.work_sites
                            .as_ref()
                            .ok_or("confinement requires physical sites")?,
                        tick,
                        opposition,
                    )?;
                    if let Some(i) = guard {
                        opening_guards[i].1 =
                            opening_guards[i].1.saturating_sub(u128::from(effort));
                    }
                } else {
                    self.confinement
                        .clone()
                        .ok_or("confinement is disabled")?
                        .release(
                            person,
                            target,
                            &self.people,
                            self.work_sites
                                .as_ref()
                                .ok_or("confinement requires physical sites")?,
                            tick,
                            0,
                        )?;
                }
            }
            _ => {}
        }
        let has_personal_treatment = self
            .dynamics
            .as_ref()
            .is_some_and(|owner| !owner.treatment.is_empty());
        let needs_held_choices = self.giving.is_some()
            || self.care.is_some()
            || self.medical.is_some()
            || self.dynamics.as_ref().is_some_and(|d| d.social.is_some())
            || matches!(
                player_transfer,
                Some(game_parallel_economy_interaction::PhysicalChoice::TreatOther { .. })
            )
            || has_personal_treatment;
        let busy_before_confinement = if held.is_empty() || !needs_held_choices {
            Vec::new()
        } else {
            busy.par_iter().copied().collect()
        };
        if !held.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, held.iter().copied());
        }
        if let Some(owner) = next_confinement
            .as_mut()
            .filter(|owner| !owner.release_policies.is_empty())
        {
            use game_parallel_economy_interaction::PhysicalChoice;
            let mut reserved_targets: Vec<_> = request.map(|row| row.target).into_iter().collect();
            let target = match player_transfer {
                Some(PhysicalChoice::Give { recipient, .. }) => Some(recipient),
                Some(PhysicalChoice::TreatOther { patient, .. }) => Some(patient),
                Some(PhysicalChoice::AskFood { target, .. })
                | Some(PhysicalChoice::Confine { target, .. })
                | Some(PhysicalChoice::ReleaseHeld { target, .. }) => Some(target),
                Some(PhysicalChoice::AskHaul { carrier, .. }) => Some(carrier),
                _ => None,
            };
            reserved_targets.extend(target);
            if let Some(plan) = &taking_plan {
                reserved_targets.extend_from_slice(&plan.targets);
            }
            reserved_targets.sort_unstable();
            reserved_targets.dedup();
            let actors = game_parallel_economy_confinement::ReleaseContext {
                owner,
                people: &self.people,
                sites: self
                    .work_sites
                    .as_ref()
                    .ok_or("confinement requires physical sites")?,
                busy: &busy,
                reserved_targets: &reserved_targets,
                opening_guards: &opening_guards,
                opinions: self.opinions.as_ref(),
                seed: self.seed,
                tick,
            }
            .prepare();
            game_parallel_economy_work::reserve(&mut busy, actors);
        }
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.coordination");
        }
        let meeting_target = request
            .filter(|row| {
                self.confinement
                    .as_ref()
                    .is_some_and(|owner| owner.held(row.target))
                    && next_confinement.as_ref().is_some_and(|owner| {
                        owner.held(row.target) && owner.can_communicate(row.actor, row.target)
                    })
                    && busy_before_confinement.binary_search(&row.target).is_err()
                    && escape_actors.binary_search(&row.target).is_err()
            })
            .map(|row| row.target);
        let interaction_busy = meeting_target
            .map(|target| game_parallel_economy_work::exclude_sorted(&busy, &[target]));
        let interaction = game_parallel_economy_interaction::prepare(
            request,
            self.dynamics.as_ref().and_then(|d| d.social.as_ref()),
            self.opinions.as_ref(),
            &self.people,
            interaction_busy.as_deref().unwrap_or(&busy),
            (self.physical.work(), self.travel.as_ref()),
            tick,
        )?;
        if let Some(plan) = &interaction {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        if let Some(owner) = next_confinement
            .as_mut()
            .filter(|owner| !owner.capture_policies.is_empty())
        {
            let mut autonomous_busy = busy.clone();
            if let Some(plan) = &taking_plan {
                autonomous_busy.extend_from_slice(&plan.targets);
            }
            if let Some(choice) = player_transfer {
                use game_parallel_economy_interaction::PhysicalChoice;
                let target = match choice {
                    PhysicalChoice::Give { recipient, .. } => Some(recipient),
                    PhysicalChoice::TreatOther { patient, .. } => Some(patient),
                    PhysicalChoice::AskFood { target, .. }
                    | PhysicalChoice::Confine { target, .. }
                    | PhysicalChoice::ReleaseHeld { target, .. } => Some(target),
                    PhysicalChoice::AskHaul { carrier, .. } => Some(carrier),
                    _ => None,
                };
                autonomous_busy.extend(target);
            }
            autonomous_busy.par_sort_unstable();
            autonomous_busy.dedup();
            let mut controlled_unavailable = initial_fighters.to_vec();
            if let Some(plan) = &taking_plan {
                controlled_unavailable.extend_from_slice(&plan.targets);
            }
            controlled_unavailable.extend(held.iter().copied());
            controlled_unavailable.extend(resting.iter().copied());
            controlled_unavailable.extend(meal_candidates.iter().map(|row| row.0));
            controlled_unavailable.extend(
                self.physical
                    .work()
                    .iter()
                    .filter(|job| job.phase == game_parallel_lot_runtime::Phase::Working)
                    .map(|job| job.job.worker),
            );
            controlled_unavailable.extend(request.map(|row| row.actor));
            controlled_unavailable.extend(player_work.map(|row| row.actor()));
            controlled_unavailable.extend(player_meal.map(|row| row.person));
            controlled_unavailable.extend(player_transfer.map(|row| row.person()));
            if let Some((row, _)) = &manual_response {
                controlled_unavailable.push(row.worker);
            }
            controlled_unavailable.par_sort_unstable();
            controlled_unavailable.dedup();
            let outcome = game_parallel_economy_confinement::AutonomousContext {
                owner,
                people: &self.people,
                sites: self
                    .work_sites
                    .as_ref()
                    .ok_or("confinement requires physical sites")?,
                busy: &autonomous_busy,
                controlled_unavailable: &controlled_unavailable,
                opinions: self.opinions.as_ref(),
                seed: self.seed,
                tick,
            }
            .prepare();
            game_parallel_economy_work::reserve(&mut busy, outcome.participants);
            held.par_extend(outcome.captured);
            held.par_sort_unstable();
            held.dedup();
        }
        let (guardian_departures, guardian_actors) = campaign_draft.as_mut().map_or_else(
            || (Vec::new(), Vec::new()),
            |draft| draft.prepare_guardians(&self.people, self.travel.as_ref(), &busy),
        );
        if !guardian_actors.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, guardian_actors);
        }
        let communication = game_parallel_economy_confinement::Communication {
            before: self.confinement.as_ref(),
            after: next_confinement.as_ref(),
        };
        let mut available_held: Vec<_> = if needs_held_choices {
            held.par_iter()
                .copied()
                .filter(|person| {
                    self.confinement
                        .as_ref()
                        .is_some_and(|owner| owner.held(*person))
                        && next_confinement
                            .as_ref()
                            .is_some_and(|owner| owner.held(*person))
                        && busy_before_confinement.binary_search(person).is_err()
                        && escape_actors.binary_search(person).is_err()
                        && interaction
                            .as_ref()
                            .is_none_or(|plan| !plan.actors.contains(person))
                })
                .collect()
        } else {
            Vec::new()
        };
        let care_busy = (self.care.is_some() && !available_held.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, &available_held));
        let care_contact = |a, b| communication.can_touch(a, b);
        let care_stock = |person| !communication.held(person);
        let mut action_base = u64::try_from(meals.actions().len())
            .map_err(|_| "action count overflow")?
            .checked_add(1)
            .ok_or("action count overflow")?;
        if let Some(plan) = &mut taking_plan {
            plan.rebase(action_base).map_err(str::to_owned)?;
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
        }
        let care = family::prepare_care!(
            self,
            people,
            view,
            claims,
            busy,
            meal_candidates,
            action_base,
            tick,
            care_busy.as_deref().unwrap_or(&busy),
            care_contact,
            care_stock
        );
        if !available_held.is_empty()
            && let Some(plan) = &care
            && !plan.actors.is_empty()
        {
            let mut caregivers = plan.actors.clone();
            if caregivers.par_windows(2).any(|pair| pair[0] > pair[1]) {
                caregivers.par_sort_unstable();
            }
            available_held =
                game_parallel_economy_work::exclude_sorted(&available_held, &caregivers);
        }
        let (pickup, mut claim_uses, transfer) =
            family::prepare_provisioning!(self, view, busy, action_base, claims, player_transfer);
        let exchange = game_parallel_economy_exchange::Context {
            market: self.market.as_ref(),
            view: &view,
            capacity: self.rules.carrying_capacity,
            people: &people,
            goods: self.physical.goods(),
            opinions: self.opinions.as_ref(),
        };
        let trade = exchange.trade(&mut busy, action_base)?;
        action_base = action_base
            .checked_add(game_parallel_economy_exchange::actions(trade.as_ref()).len() as u64)
            .ok_or("action count overflow")?;
        let needs_locations = self.agreements.as_ref().is_some_and(|agreements| {
            !agreements.contracts.is_empty()
                || !agreements.offering.is_empty()
                || !agreements.work_offering.is_empty()
        }) || self.giving.is_some()
            || self.medical.is_some()
            || self.hauling.as_ref().is_some_and(|h| !h.asking.is_empty())
            || matches!(
                player_transfer,
                Some(
                    game_parallel_economy_interaction::PhysicalChoice::Give { .. }
                        | game_parallel_economy_interaction::PhysicalChoice::TreatOther { .. }
                        | game_parallel_economy_interaction::PhysicalChoice::AskFood { .. }
                        | game_parallel_economy_interaction::PhysicalChoice::AskHaul { .. }
                )
            );
        let locations = person::travel::Colocation::new(
            needs_locations.then_some(self.travel.as_ref()).flatten(),
        );
        let blocked_receivers: Vec<_> = held
            .par_iter()
            .copied()
            .filter(|person| {
                self.confinement
                    .as_ref()
                    .is_some_and(|owner| !owner.can_receive_supplies(*person))
                    || next_confinement
                        .as_ref()
                        .is_some_and(|owner| !owner.can_receive_supplies(*person))
            })
            .collect();
        let promises = game_parallel_economy_promises::plan_world!(
            self,
            people,
            locations,
            busy,
            tick,
            action_base,
            player_transfer,
            &blocked_receivers
        );
        let proposals = exchange.proposals(&mut busy)?;
        action_base = action_base
            .checked_add(promises.action_count() as u64)
            .ok_or("action count overflow")?;
        let treatment_busy = (has_personal_treatment && !available_held.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, &available_held));
        let treatment = game_parallel_economy_dynamics::prepare_treatment!(
            self,
            treatment_busy.as_deref().unwrap_or(&busy),
            view,
            action_base,
            player_transfer
        );
        action_base = action_base
            .checked_add(treatment.actions.len() as u64)
            .ok_or("action count overflow")?;
        game_parallel_economy_work::reserve(&mut busy, treatment.actors.iter().copied());
        let manual_give = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Give {
                person,
                recipient,
                good,
                quantity,
            }) => Some((person, recipient, good, quantity)),
            _ => None,
        };
        // Work-age restrictions are not themselves an activity. Preserve
        // availability for independently chosen personal interactions.
        let underage = self
            .family
            .as_ref()
            .map_or_else(Vec::new, game_parallel_economy_work::underage);
        let age_exceptions_needed = self.giving.is_some()
            || self.animal_handling.is_some()
            || self.animal_restraints.is_some()
            || self.medical.is_some()
            || matches!(
                player_transfer,
                Some(game_parallel_economy_interaction::PhysicalChoice::TreatOther { .. })
            )
            || self.dynamics.as_ref().is_some_and(|d| d.social.is_some());
        let idle_underage = if age_exceptions_needed && !underage.is_empty() {
            game_parallel_economy_work::exclude_sorted(&underage, &busy)
        } else {
            Vec::new()
        };
        if !underage.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, underage);
        }
        let promise_offers = game_parallel_economy_promises::offers::prepare(
            game_parallel_economy_promises::offers::Context {
                owner: self.agreements.as_ref(),
                people: &self.people,
                opinions: self.opinions.as_ref(),
                requests: self
                    .autowork
                    .as_ref()
                    .and_then(|auto| auto.requests.as_ref()),
                locations: &locations,
                busy: &busy,
                max_hunger: self.rules.max_work_hunger,
                tick,
            },
        )?;
        if !promise_offers.proposals().is_empty() {
            busy.par_extend(promise_offers.proposals().par_iter().map(|p| p.debtor));
            if busy.par_windows(2).any(|rows| rows[0] > rows[1]) {
                busy.par_sort_unstable();
            }
            if busy.par_windows(2).any(|rows| rows[0] == rows[1]) {
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
        let mut reserved_can_give = Vec::new();
        let mut asked_actors = Vec::new();
        let mut next_giving = self.giving.as_ref().map(Giving::clone_without_practices);
        if let Some(giving) = &mut next_giving {
            let mut treated: Vec<_> = if available_held.is_empty() {
                Vec::new()
            } else {
                treatment.actors.par_iter().copied().collect()
            };
            if treated.par_windows(2).any(|pair| pair[0] > pair[1]) {
                treated.par_sort_unstable();
            }
            let mut askable: Vec<_> = available_held
                .par_iter()
                .copied()
                .filter(|person| treated.binary_search(person).is_err())
                .collect();
            if !idle_underage.is_empty() {
                if askable.is_empty() {
                    askable.par_extend(idle_underage.par_iter().copied());
                } else {
                    game_parallel_economy_work::reserve(
                        &mut askable,
                        idle_underage.iter().copied(),
                    );
                }
            }
            let mut asking_busy = (!askable.is_empty())
                .then(|| game_parallel_economy_work::exclude_sorted(&busy, &askable));
            let askers = giving.ask(
                &self.people,
                self.physical.goods(),
                &view,
                (
                    asking_busy.as_deref().unwrap_or(&busy),
                    self.giving
                        .as_ref()
                        .and_then(|owner| owner.practices.as_ref()),
                    &communication,
                ),
                tick,
                &locations,
            )?;
            if let Some(asking_busy) = &mut asking_busy {
                game_parallel_economy_work::reserve(asking_busy, askers.iter().copied());
            }
            let mut request_actors: Vec<_> = if askable.is_empty() {
                Vec::new()
            } else {
                askers.par_iter().copied().collect()
            };
            game_parallel_economy_work::reserve(&mut busy, askers);
            let acquaintances = self
                .dynamics
                .as_ref()
                .and_then(|owner| owner.social.as_ref())
                .map_or(&[][..], |social| social.relationships.as_slice());
            let askers = giving_discovery::prepare(
                giving,
                giving_discovery::Context {
                    people: &self.people,
                    goods: self.physical.goods(),
                    view: &view,
                    busy: asking_busy.as_deref().unwrap_or(&busy),
                    relationships: acquaintances,
                    opinions: self.opinions.as_ref(),
                    locations: &locations,
                    communication: &communication,
                    seed: self.seed,
                    tick,
                },
            )?;
            if !askable.is_empty() {
                request_actors.par_extend(askers.par_iter().copied());
                request_actors.par_sort_unstable();
                asked_actors = request_actors.clone();
                reserved_can_give = askable
                    .into_par_iter()
                    .filter(|person| request_actors.binary_search(person).is_err())
                    .collect();
            }
            game_parallel_economy_work::reserve(&mut busy, askers);
        }
        if let Some(game_parallel_economy_interaction::PhysicalChoice::AskFood {
            person,
            target,
            lifetime,
        }) = player_transfer
        {
            let giving = next_giving.as_mut().ok_or("food requests are disabled")?;
            let actor = self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let recipient = self
                .people
                .binary_search_by_key(&target, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown target")?;
            if !actor.controlled
                || actor.health == 0
                || recipient.health == 0
                || person == target
                || !communication.allows(person, target)
                || locations
                    .region((actor.id, actor.region), (recipient.id, recipient.region))
                    .is_none()
                || lifetime == 0
            {
                return Err(
                    "food request requires a living controlled person and a local recipient".into(),
                );
            }
            let expires_at = tick
                .checked_add(lifetime)
                .ok_or("food request expiry overflow")?;
            giving.requests = giving
                .requests
                .par_iter()
                .filter(|r| r.requester != person)
                .cloned()
                .collect();
            giving.requests.push(FoodRequest {
                requester: person,
                target,
                expires_at,
            });
        }
        let giving_busy = (!reserved_can_give.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, &reserved_can_give));
        let mut giving_plan = giving::Context {
            owner: self.giving.as_ref(),
            missions: self.missions.as_ref(),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            work_requests: self
                .autowork
                .as_ref()
                .and_then(|auto| auto.requests.as_ref()),
            goods: self.physical.goods(),
            view: &view,
            busy: giving_busy.as_deref().unwrap_or(&busy),
            communication: &communication,
            manual: manual_give,
            tick,
            seed: self.seed,
            locations: &locations,
        }
        .prepare(action_base)?;
        action_base = action_base
            .checked_add(giving_plan.actions.len() as u64)
            .ok_or("action count overflow")?;
        if !giving_plan.actors.is_empty() {
            busy.par_extend(giving_plan.actors.par_iter().copied());
            busy.par_sort_unstable();
            busy = busy
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect();
        }
        let free_underage = if idle_underage.is_empty()
            || (asked_actors.is_empty() && giving_plan.actors.is_empty())
        {
            idle_underage.clone()
        } else {
            let mut occupied = asked_actors.clone();
            occupied.par_extend(giving_plan.actors.par_iter().copied());
            occupied.par_sort_unstable();
            game_parallel_economy_work::exclude_sorted(&idle_underage, &occupied)
        };
        let manual_medical = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::TreatOther {
                person,
                patient,
                medicine,
                quantity,
            }) => Some((person, patient, medicine, quantity)),
            _ => None,
        };
        let needs_medical = manual_medical.is_some()
            || self
                .medical
                .as_ref()
                .is_some_and(|owner| !owner.policies.is_empty() || !owner.acquaintances.is_empty());
        if let Some((person, _, _, _)) = manual_medical
            && self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .any(|m| m.id == person && m.age < f.rules.min_work_age)
            })
        {
            return Err("person is too young to provide medical care".into());
        }
        // Care requires actual contact and each captive's action must still be free.
        let mut held_committed = Vec::new();
        if !held.is_empty() && needs_medical {
            held_committed.par_extend(treatment.actors.par_iter().copied());
            held_committed.extend(asked_actors.iter().copied());
            held_committed.par_extend(giving_plan.actors.par_iter().copied());
            held_committed.par_sort_unstable();
        }
        let held_patients: Vec<_> = if needs_medical && !held.is_empty() {
            held.par_iter()
                .copied()
                .filter(|person| {
                    let controlled = self
                        .people
                        .binary_search_by_key(person, |p| p.id)
                        .ok()
                        .is_some_and(|i| self.people[i].controlled);
                    let child = self.family.as_ref().is_some_and(|family| {
                        family
                            .members
                            .binary_search_by_key(person, |m| m.id)
                            .ok()
                            .is_some_and(|i| family.members[i].age < family.rules.min_work_age)
                    });
                    self.confinement
                        .as_ref()
                        .is_some_and(|owner| owner.held(*person))
                        && next_confinement
                            .as_ref()
                            .is_some_and(|owner| owner.held(*person))
                        && (controlled || child || available_held.binary_search(person).is_ok())
                        && escape_actors.binary_search(person).is_err()
                        && held_committed.binary_search(person).is_err()
                        && interaction
                            .as_ref()
                            .is_none_or(|plan| !plan.actors.contains(person))
                })
                .collect()
        } else {
            Vec::new()
        };
        let held_medical_actors: Vec<_> = if needs_medical {
            available_held
                .par_iter()
                .copied()
                .filter(|person| held_patients.binary_search(person).is_ok())
                .filter(|person| {
                    self.family.as_ref().is_none_or(|family| {
                        family
                            .members
                            .binary_search_by_key(person, |m| m.id)
                            .ok()
                            .is_none_or(|i| family.members[i].age >= family.rules.min_work_age)
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let medical_busy = (!held_medical_actors.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, &held_medical_actors));
        let blocked_holds = if needs_medical {
            game_parallel_economy_work::exclude_sorted(&held, &held_patients)
        } else {
            Vec::new()
        };
        let mut available_patients = held_patients;
        if needs_medical && !free_underage.is_empty() {
            game_parallel_economy_work::reserve(
                &mut available_patients,
                free_underage.iter().copied(),
            );
        }
        let mut medical_blocked: Vec<_> = if needs_medical {
            treatment
                .actors
                .par_iter()
                .copied()
                .chain(fighters.par_iter().copied())
                .chain(meal_candidates.par_iter().map(|row| row.0))
                .chain(blocked_holds.par_iter().copied())
                .collect()
        } else {
            Vec::new()
        };
        if needs_medical {
            medical_blocked.extend(request.map(|choice| choice.actor));
            medical_blocked.extend(player_work.map(|choice| choice.actor()));
            medical_blocked.extend(player_transfer.and_then(|choice| {
                (!matches!(
                    choice,
                    game_parallel_economy_interaction::PhysicalChoice::Rest { .. }
                ))
                .then_some(choice.person())
            }));
            medical_blocked.par_sort_unstable();
            medical_blocked.dedup();
        }
        let medical_plan = medical::Context {
            owner: self.medical.as_ref(),
            disease: self.dynamics.as_ref().and_then(|d| d.disease.as_ref()),
            social: self.dynamics.as_ref().and_then(|d| d.social.as_ref()),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            goods: self.physical.goods(),
            view: &view,
            busy: medical_busy.as_deref().unwrap_or(&busy),
            blocked: &medical_blocked,
            available_patients: &available_patients,
            communication: &communication,
            resting: &resting,
            seed: self.seed,
            tick,
            max_health: self.rules.max_health,
            manual: manual_medical,
            locations: &locations,
        }
        .prepare(action_base)?;
        action_base = action_base
            .checked_add(medical_plan.actions.len() as u64)
            .ok_or("action count overflow")?;
        if !medical_plan.actors.is_empty() {
            busy.par_extend(medical_plan.actors.par_iter().copied());
            busy.par_sort_unstable();
            busy = busy
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect();
        }
        let manual_restraint = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::CaptureAnimal {
                person,
                animal,
            }) => Some(game_parallel_animal_restraint::Manual::Capture { person, animal }),
            Some(game_parallel_economy_interaction::PhysicalChoice::ReleaseAnimal {
                person,
                animal,
            }) => Some(game_parallel_animal_restraint::Manual::Release { person, animal }),
            Some(game_parallel_economy_interaction::PhysicalChoice::DefendAnimal {
                person,
                animal,
            }) => Some(game_parallel_animal_restraint::Manual::Defend { person, animal }),
            _ => None,
        };
        let manual_restraint_target = manual_restraint.map(|choice| match choice {
            game_parallel_animal_restraint::Manual::Capture { animal, .. }
            | game_parallel_animal_restraint::Manual::Release { animal, .. }
            | game_parallel_animal_restraint::Manual::Defend { animal, .. } => animal,
        });
        let restraint_target: Vec<_> = manual_restraint_target.into_iter().collect();
        if player_transfer.is_some_and(|choice| {
            matches!(
                choice,
                game_parallel_economy_interaction::PhysicalChoice::CaptureAnimal { .. }
                    | game_parallel_economy_interaction::PhysicalChoice::ReleaseAnimal { .. }
                    | game_parallel_economy_interaction::PhysicalChoice::DefendAnimal { .. }
            ) && self.physical.work().iter().any(|work| {
                work.job.worker == choice.person()
                    && work.phase == game_parallel_lot_runtime::Phase::Working
            })
        }) {
            return Err("animal restraint requires an available person".into());
        }
        let mut animal_care_plan = animal_care::Context {
            owner: self.animal_care.as_ref(),
            fauna: self.fauna.as_ref(),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            busy: &busy,
            blocked_animals: &restraint_target,
            max_hunger: self.rules.max_work_hunger,
            seed: self.seed,
            tick,
        }
        .prepare(action_base)?;
        action_base = action_base
            .checked_add(animal_care_plan.actions.len() as u64)
            .ok_or("action count overflow")?;
        if !animal_care_plan.actors.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, animal_care_plan.actors.iter().copied());
        }
        let manual_hunt = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::HuntAnimal {
                person,
                animal,
            }) => Some((person, animal)),
            _ => None,
        };
        if let Some((person, _)) = manual_hunt
            && (self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .any(|m| m.id == person && m.age < f.rules.min_work_age)
            }) || self.physical.work().iter().any(|w| {
                w.job.worker == person && w.phase == game_parallel_lot_runtime::Phase::Working
            }))
        {
            return Err("hunting requires an available person".into());
        }
        let mut feed_targets = animal_care_plan.targets();
        if let Some(game_parallel_economy_interaction::PhysicalChoice::FeedAnimal {
            animal, ..
        }) = player_transfer
        {
            feed_targets.push(animal);
            feed_targets.par_sort_unstable();
            feed_targets.dedup();
        }
        let manual_handling = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::HandleAnimal {
                person,
                animal,
            }) => Some((person, animal)),
            _ => None,
        };
        if manual_handling.is_some_and(|(person, _)| {
            self.physical.work().iter().any(|work| {
                work.job.worker == person && work.phase == game_parallel_lot_runtime::Phase::Working
            })
        }) {
            return Err("handling requires an available person".into());
        }
        let has_capture_policies = self.animal_restraints.as_ref().is_some_and(|owner| {
            !owner.policies.is_empty()
                || !owner.release_policies.is_empty()
                || !owner.defense_policies.is_empty()
        });
        let mut restraint_blocked =
            if self.animal_restraints.is_some() || manual_restraint.is_some() {
                feed_targets.clone()
            } else {
                Vec::new()
            };
        restraint_blocked.extend(manual_hunt.map(|(_, animal)| animal));
        restraint_blocked.extend(manual_handling.map(|(_, animal)| animal));
        if restraint_blocked
            .par_windows(2)
            .any(|pair| pair[0] >= pair[1])
        {
            restraint_blocked.par_sort_unstable();
            restraint_blocked.dedup();
        }
        let restraint_children = if has_capture_policies && !free_underage.is_empty() {
            game_parallel_economy_work::exclude_sorted(&free_underage, &medical_plan.actors)
        } else {
            Vec::new()
        };
        let restraint_busy = (!restraint_children.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, &restraint_children));
        let restraint_affinity = |person, animal| {
            self.animal_feelings
                .as_ref()
                .and_then(|feelings| feelings.affinity(person, animal))
                .unwrap_or(0)
        };
        #[cfg(feature = "profile")]
        let restraint_timer = game_diagnostics::span("economy.animal_restraint");
        let restraint_plan = game_parallel_animal_restraint::Context {
            owner: self.animal_restraints.as_ref(),
            fauna: self.fauna.as_ref(),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            affinity: self
                .animal_feelings
                .as_ref()
                .map(|_| &restraint_affinity as &(dyn Fn(u64, u64) -> i32 + Sync)),
            busy: restraint_busy.as_deref().unwrap_or(&busy),
            blocked: &restraint_blocked,
            max_hunger: self.rules.max_work_hunger,
            seed: self.seed,
            tick,
            manual: manual_restraint,
        }
        .prepare()?;
        #[cfg(feature = "profile")]
        drop(restraint_timer);
        if !restraint_plan.actors.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, restraint_plan.actors.iter().copied());
        }
        let animal_released = manual_restraint.map(|choice| match choice {
            game_parallel_animal_restraint::Manual::Release { person, animal } => {
                restraint_plan.state.as_ref().is_some_and(|owner| {
                    owner.events.iter().any(|event| {
                        event.animal == animal
                            && event.actor == Some(person)
                            && event.kind == AnimalRestraintEventKind::Released
                    })
                })
            }
            _ => false,
        });
        let mut next_animal_restraints = restraint_plan.state;
        feed_targets.par_extend(restraint_plan.reserved.par_iter().copied());
        if feed_targets.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            feed_targets.par_sort_unstable();
            feed_targets.dedup();
        }
        let has_handling_policies = self
            .animal_handling
            .as_ref()
            .is_some_and(|owner| !owner.policies.is_empty());
        let mut handling_blocked = if has_handling_policies || manual_handling.is_some() {
            feed_targets.clone()
        } else {
            Vec::new()
        };
        if let Some((_, animal)) = manual_hunt {
            handling_blocked.push(animal);
            handling_blocked.par_sort_unstable();
            handling_blocked.dedup();
        }
        let handling_children = if has_handling_policies && !free_underage.is_empty() {
            let mut occupied = medical_plan.actors.clone();
            game_parallel_economy_work::reserve(
                &mut occupied,
                restraint_plan.actors.iter().copied(),
            );
            game_parallel_economy_work::exclude_sorted(&free_underage, &occupied)
        } else {
            Vec::new()
        };
        let handling_busy = (!handling_children.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, &handling_children));
        let handling_plan = animal_handling::Context {
            owner: self.animal_handling.as_ref(),
            fauna: self.fauna.as_ref(),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            feelings: self.animal_feelings.as_ref(),
            busy: handling_busy.as_deref().unwrap_or(&busy),
            blocked: &handling_blocked,
            max_hunger: self.rules.max_work_hunger,
            seed: self.seed,
            tick,
            manual: manual_handling,
        }
        .prepare()?;
        if !handling_plan.actors.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, handling_plan.actors.iter().copied());
        }
        let handled = manual_handling.map(|(person, animal)| {
            handling_plan.events.iter().any(|event| {
                event.person == person
                    && event.animal == animal
                    && event.outcome == AnimalHandlingOutcome::Accepted
            })
        });
        let next_animal_handling = (self.animal_handling.is_some() || manual_handling.is_some())
            .then(|| AnimalHandling {
                inherit_births: self
                    .animal_handling
                    .as_ref()
                    .is_some_and(|owner| owner.inherit_births),
                policies: self
                    .animal_handling
                    .as_ref()
                    .map_or_else(Vec::new, |owner| owner.policies.clone()),
                events: handling_plan.events.clone(),
            });
        feed_targets.par_extend(handling_plan.events.par_iter().filter_map(|event| {
            (event.outcome != AnimalHandlingOutcome::Unavailable).then_some(event.animal)
        }));
        if feed_targets.par_windows(2).any(|pair| pair[0] >= pair[1]) {
            feed_targets.par_sort_unstable();
            feed_targets.dedup();
        }
        let hunting_plan = hunting::Context {
            owner: self.hunting.as_ref(),
            fauna: self.fauna.as_ref(),
            people: &self.people,
            opinions: self.opinions.as_ref(),
            animal_feelings: self.animal_feelings.as_ref(),
            view: &view,
            requests: self
                .giving
                .as_ref()
                .map_or(&[], |giving| giving.requests.as_slice()),
            busy: &busy,
            feed_targets: &feed_targets,
            max_hunger: self.rules.max_work_hunger,
            seed: self.seed,
            tick,
            manual: manual_hunt,
        }
        .prepare(action_base)?;
        action_base = action_base
            .checked_add(hunting_plan.yields.len() as u64)
            .ok_or("action count overflow")?;
        if !hunting_plan.actors.is_empty() {
            game_parallel_economy_work::reserve(&mut busy, hunting_plan.actors.iter().copied());
        }
        let mut next_hauling = self.hauling.clone();
        if let Some(hauling) = &mut next_hauling {
            hauling.requests = hauling
                .requests
                .par_iter()
                .filter(|r| r.expires_at >= tick)
                .cloned()
                .collect();
            let askers = hauling.ask(&self.people, &busy, &locations, tick)?;
            if !askers.is_empty() {
                busy.par_extend(askers.into_par_iter());
                busy.par_sort_unstable();
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
        if let Some(game_parallel_economy_interaction::PhysicalChoice::AskHaul {
            person,
            carrier,
            source,
            destination,
            good,
            lifetime,
        }) = player_transfer
        {
            let hauling = next_hauling.as_mut().ok_or("hauling is disabled")?;
            let actor = self
                .people
                .binary_search_by_key(&person, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown person")?;
            let recipient = self
                .people
                .binary_search_by_key(&carrier, |p| p.id)
                .ok()
                .map(|i| &self.people[i])
                .ok_or("unknown carrier")?;
            let travel = self.travel.as_ref().ok_or("travel is disabled")?;
            let routes =
                game_parallel_economy_person::travel::routes(travel, &[source, destination]);
            if !actor.controlled
                || actor.health == 0
                || recipient.health == 0
                || person == carrier
                || lifetime == 0
                || locations
                    .region((actor.id, actor.region), (recipient.id, recipient.region))
                    .is_none()
                || source == destination
                || !routes.contains_key(&(source, destination))
                || !routes.contains_key(&(destination, source))
                || view
                    .container(game_parallel_lots::Place::Stock {
                        region: source,
                        good,
                    })
                    .is_none()
                || view
                    .container(game_parallel_lots::Place::Stock {
                        region: destination,
                        good,
                    })
                    .is_none()
            {
                return Err(
                    "hauling request requires a nearby person and valid route and stocks".into(),
                );
            }
            let expires_at = tick
                .checked_add(lifetime)
                .ok_or("hauling request expiry overflow")?;
            hauling.requests = hauling
                .requests
                .par_iter()
                .filter(|r| r.requester != person)
                .cloned()
                .collect();
            hauling.requests.push(HaulingRequest {
                requester: person,
                carrier,
                source,
                destination,
                good,
                expires_at,
            });
        }
        let hauling_plan = self
            .hauling
            .as_ref()
            .map(|owner| {
                hauling::Context {
                    owner,
                    travel: self.travel.as_ref().ok_or("hauling needs travel routes")?,
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    view: &view,
                    busy: &busy,
                    max_hunger: self.rules.max_work_hunger,
                    tick,
                    seed: self.seed,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &hauling_plan {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
            claim_uses.par_extend(plan.uses.par_iter().copied());
            if !plan.actors.is_empty() {
                busy.par_extend(plan.actors.par_iter().copied());
                busy.par_sort_unstable();
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
        }
        let manual_local = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::LocalStore {
                person,
                store,
                good,
                quantity,
                deposit,
            }) => {
                if self.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                }) || self.physical.work().iter().any(|w| {
                    w.job.worker == person && w.phase == game_parallel_lot_runtime::Phase::Working
                }) {
                    return Err("person cannot carry locally while underage or working".into());
                }
                Some(game_parallel_economy_local_transport::Manual {
                    person,
                    store,
                    good,
                    quantity,
                    deposit,
                })
            }
            _ => None,
        };
        if manual_local.is_some() && self.local_transport.is_none() {
            return Err("local transport is disabled".into());
        }
        let local_plan = self
            .local_transport
            .as_ref()
            .map(|owner| {
                game_parallel_economy_local_transport::Context {
                    owner,
                    people: &self.people,
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: self.rules.max_work_hunger,
                    manual: manual_local,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &local_plan {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
            claim_uses.par_extend(plan.uses.par_iter().copied());
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let mut expedition_acquisition = game_parallel_economy_expeditions::acquisition::Context {
            missions: self.missions.as_ref(),
            people: &self.people,
            view: &view,
            opinions: self.opinions.as_ref(),
            claims: claims.as_ref(),
            busy: &busy,
            max_hunger: self.rules.max_work_hunger,
        }
        .prepare(action_base)?;
        if !expedition_acquisition.actors.is_empty() {
            action_base = action_base
                .checked_add(expedition_acquisition.actions.len() as u64)
                .ok_or("action count overflow")?;
            claim_uses.par_extend(expedition_acquisition.uses.par_iter().copied());
            game_parallel_economy_work::reserve(
                &mut busy,
                expedition_acquisition.actors.iter().copied(),
            );
        }
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.planning");
        }
        let expedition =
            game_parallel_economy_expeditions::prepare_world!(self, people, busy, tick);
        let search_plan = self
            .food_search
            .as_ref()
            .zip(self.knowledge.as_ref())
            .zip(self.travel.as_ref())
            .map(|((owner, knowledge), travel)| {
                food_search::Context {
                    owner,
                    knowledge,
                    travel,
                    people: &self.people,
                    goods: self.physical.goods(),
                    view: &view,
                    claims: claims.as_ref(),
                    busy: &busy,
                    now: self.tick(),
                }
                .prepare()
            });
        if let Some(plan) = &search_plan
            && !plan.departures.is_empty()
        {
            busy.par_extend(plan.departures.par_iter().map(|d| d.person));
            busy.par_sort_unstable();
            busy = busy
                .par_chunk_by(|a, b| a == b)
                .map(|group| group[0])
                .collect();
        }
        let (mut farming, mut claim_outputs) = game_parallel_economy_claims_output::prepare_farming!(
            self,
            busy,
            view,
            tick,
            action_base,
            claims,
            player_transfer
        );
        action_base = action_base
            .checked_add(farming.as_ref().map_or(0, |p| p.changes.len() as u64))
            .ok_or("action count overflow")?;
        let manual_gather = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Gather { person, node }) => {
                Some((person, node))
            }
            _ => None,
        };
        if let Some((person, _)) = manual_gather
            && (self.gathering.is_none()
                || self.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                })
                || self.physical.work().iter().any(|w| {
                    w.job.worker == person && w.phase == game_parallel_lot_runtime::Phase::Working
                }))
        {
            return Err("person cannot gather now".into());
        }
        let gathering_plan = self
            .gathering
            .as_ref()
            .map(|owner| {
                gathering::Context {
                    owner,
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    busy: &busy,
                    goods: self.physical.goods(),
                    view: &view,
                    tick_millis: self.physical.tick_millis(),
                    seed: self.seed,
                    tick,
                    manual: manual_gather,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &gathering_plan {
            if !plan.actors.is_empty() {
                busy.par_extend(plan.actors.par_iter().copied());
                busy.par_sort_unstable();
                busy = busy
                    .par_chunk_by(|a, b| a == b)
                    .map(|group| group[0])
                    .collect();
            }
            if let Some(claims) = self
                .dynamics
                .as_ref()
                .and_then(|d| d.claims.as_ref())
                .filter(|c| c.preferences.par_iter().any(|p| p.assert_outputs))
            {
                let outputs = plan.outputs(self.gathering.as_ref().expect("gathering owner"));
                claim_outputs.par_extend(game_parallel_economy_claims_output::gather_outputs(
                    &outputs,
                    &self.people,
                    Some(claims),
                ));
            }
        }
        let requests = self
            .autowork
            .as_ref()
            .and_then(|auto| auto.requests.as_ref());
        let work_offers =
            requests.and_then(|requests| requests.offer_index(self.agreements.as_ref(), tick));
        let practice_offers: Vec<_> = if requests.is_some_and(|requests| {
            requests.preferences.par_iter().any(|p| {
                p.offer_value
                    .as_ref()
                    .is_some_and(|value| value.value_per_unit != 0)
            })
        }) {
            self.giving
                .as_ref()
                .and_then(|giving| giving.practices.as_ref())
                .map_or(&[][..], |p| p.minds.as_slice())
                .par_iter()
                .flat_map_iter(|person| person.work.iter().flat_map(|work| &work.inbox))
                .filter(|received| {
                    received.decision == PracticeDecision::Adopted
                        && received.proposal.expires >= tick
                })
                .map(|received| game_parallel_autowork::requests::PracticeOffer {
                    worker: received.proposal.worker,
                    requester: received.proposal.author,
                    recipe: received.proposal.recipe,
                    good: received.proposal.good,
                    quantity: received.proposal.quantity,
                })
                .collect()
        } else {
            Vec::new()
        };
        let work_offers = game_parallel_autowork::requests::OfferIndex::with_practices(
            work_offers,
            practice_offers,
        );
        let (mut rates, mut abandon) = family::work_rates!(
            self,
            meal_candidates,
            player_work,
            fighters,
            resting,
            work_offers
        );
        if let Some(plan) = &taking_plan {
            game_parallel_economy_combat::interrupt(
                &plan.actors,
                self.physical.work(),
                &mut rates,
                &mut abandon,
            );
        }
        if !held.is_empty() {
            game_parallel_economy_combat::interrupt(
                &held,
                self.physical.work(),
                &mut rates,
                &mut abandon,
            );
        }
        let mut goal_work = None;
        let goal_building = self
            .construction
            .as_ref()
            .is_some_and(|c| c.goal_builders.par_iter().any(|p| p.enabled));
        let goal_fetching = self
            .local_transport
            .as_ref()
            .is_some_and(|owner| owner.goal_fetchers.par_iter().any(|p| p.enabled));
        let mut goal_fetch_needs = Vec::new();
        if !self.work_tools.is_empty() || goal_building || goal_fetching {
            goal_work = self
                .goals
                .as_ref()
                .zip(self.autowork.as_ref())
                .map(|(goals, auto)| goals.policies(auto, tick.saturating_sub(1)));
            if goal_fetching && let (Some(planned), Some(auto)) = (&goal_work, &self.autowork) {
                let explicit: HashSet<_> = auto
                    .policies
                    .par_iter()
                    .map(|policy| policy.person)
                    .collect();
                goal_fetch_needs = planned
                    .policies
                    .par_iter()
                    .filter(|policy| policy.enabled && !explicit.contains(&policy.person))
                    .map(|policy| game_parallel_economy_local_transport::goal::Need {
                        person: policy.person,
                        recipe: policy.recipe,
                    })
                    .collect();
                goal_fetch_needs.par_sort_unstable_by_key(|need| need.person);
            }
            if self.goals.as_ref().is_some_and(|g| g.plan_inputs)
                && let (Some(planned), Some(base)) = (&mut goal_work, &self.autowork)
            {
                let production = game_parallel_lot_projection::production(
                    &self.physical,
                    &view,
                    self.rules.max_work_hunger,
                );
                let review = tick.saturating_sub(1)
                    / self.goals.as_ref().expect("goal planning").retry_interval;
                game_parallel_goals::planning::plan_at(planned, base, &production, &people, review);
            }
        }
        let mut goal_needs: Vec<_> = if goal_building {
            let explicit: std::collections::HashSet<_> = self
                .autowork
                .as_ref()
                .map_or_else(std::collections::HashSet::new, |auto| {
                    auto.policies.par_iter().map(|p| p.person).collect()
                });
            goal_work.as_ref().map_or_else(Vec::new, |planned| {
                planned
                    .policies
                    .par_iter()
                    .filter(|p| p.enabled && !explicit.contains(&p.person))
                    .map(|p| game_parallel_economy_building::GoalNeed {
                        person: p.person,
                        recipe: p.recipe,
                    })
                    .collect()
            })
        } else {
            Vec::new()
        };
        goal_needs.par_sort_unstable_by_key(|need| need.person);
        let mut goal_building_refusals = if goal_building || goal_fetching {
            self.dynamics
                .as_ref()
                .and_then(|d| d.personality.as_ref())
                .map_or_else(Vec::new, |p| p.work_refusals(self.seed, tick))
        } else {
            Vec::new()
        };
        goal_building_refusals.par_sort_unstable();
        if goal_building
            && self.goals.as_ref().is_some_and(|g| g.plan_inputs)
            && !goal_needs.is_empty()
            && let (Some(owner), Some(sites), Some(planned)) = (
                self.construction.as_ref(),
                self.work_sites.as_ref(),
                &mut goal_work,
            )
        {
            let context = game_parallel_economy_building::Context {
                owner,
                sites,
                people: &self.people,
                view: &view,
                opinions: self.opinions.as_ref(),
                claims: claims.as_ref(),
                busy: &busy,
                max_hunger: self.rules.max_work_hunger,
                tick,
                manual: None,
                goal_needs: &goal_needs,
                unwilling: &goal_building_refusals,
            };
            let mut pending: Vec<_> = self
                .physical
                .work()
                .par_iter()
                .filter(|row| row.phase != game_parallel_lot_runtime::Phase::Working)
                .map(|row| row.job.worker)
                .collect();
            pending.par_sort_unstable();
            let production = game_parallel_lot_projection::production(
                &self.physical,
                &view,
                self.rules.max_work_hunger,
            );
            let review =
                tick.saturating_sub(1) / self.goals.as_ref().expect("goal planning").retry_interval;
            let steps = construction_planning::prepare(context, &production, &pending, review);
            planned.policies.par_iter_mut().for_each(|policy| {
                if let Ok(index) = steps.work.binary_search_by_key(&policy.person, |row| row.0) {
                    policy.recipe = steps.work[index].1;
                }
            });
            goal_needs = steps.builds;
        }
        if !self.work_tools.is_empty()
            || self.construction.is_some()
            || goal_fetching
            || matches!(
                player_transfer,
                Some(game_parallel_economy_interaction::PhysicalChoice::Build { .. })
            )
        {
            action_base = action_base
                .checked_add(
                    gathering_plan
                        .as_ref()
                        .map_or(0, |plan| plan.actions.len() as u64),
                )
                .ok_or("action count overflow")?;
        }
        let goal_fetch_plan = if goal_fetching {
            self.local_transport
                .as_ref()
                .map(|owner| {
                    game_parallel_economy_local_transport::goal::Context {
                        owner,
                        needs: &goal_fetch_needs,
                        recipes: self.physical.recipes(),
                        people: &self.people,
                        view: &view,
                        busy: &busy,
                        unwilling: &goal_building_refusals,
                        max_hunger: self.rules.max_work_hunger,
                        tick,
                        plan_inputs: self.goals.as_ref().is_some_and(|goals| goals.plan_inputs),
                        review: self
                            .goals
                            .as_ref()
                            .map_or(0, |goals| tick.saturating_sub(1) / goals.retry_interval),
                    }
                    .prepare(action_base)
                })
                .transpose()?
        } else {
            None
        };
        if let Some(plan) = &goal_fetch_plan {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let tool_pickup = if !self.work_tools.is_empty() {
            {
                let mut unwilling = self
                    .dynamics
                    .as_ref()
                    .and_then(|d| d.personality.as_ref())
                    .map_or_else(Vec::new, |p| p.work_refusals(self.seed, tick));
                unwilling.par_sort_unstable();
                let policies = goal_work
                    .as_ref()
                    .or(self.autowork.as_ref())
                    .map_or(&[][..], |a| a.policies.as_slice());
                let requests = self
                    .autowork
                    .as_ref()
                    .and_then(|a| a.requests.as_ref())
                    .filter(|r| !r.pending.is_empty())
                    .map(|r| {
                        r.prepare_with_offers(
                            &people,
                            self.opinions.as_ref(),
                            &busy,
                            work_offers.as_ref(),
                            tick,
                        )
                    });
                let plan = game_parallel_economy_work::tools::AcquisitionContext {
                    requirements: &self.work_tools,
                    auto: self.autowork.as_ref(),
                    policies,
                    requests: requests.as_ref(),
                    work: self.physical.work(),
                    rates: &rates,
                    people: &self.people,
                    view: &view,
                    claims: claims.as_ref(),
                    busy: &busy,
                    unwilling: &unwilling,
                    max_hunger: self.rules.max_work_hunger,
                }
                .prepare(action_base)?;
                game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
                Some(plan)
            }
        } else {
            None
        };
        if let Some(plan) = &tool_pickup {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
        }
        let manual_build = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::Build {
                person,
                blueprint,
            }) => Some(game_parallel_economy_building::Manual { person, blueprint }),
            _ => None,
        };
        if manual_build.is_some_and(|m| {
            self.family.as_ref().is_some_and(|f| {
                f.members
                    .iter()
                    .any(|member| member.id == m.person && member.age < f.rules.min_work_age)
            })
        }) {
            return Err("person is too young to build".into());
        }
        if manual_build.is_some() && self.construction.is_none() {
            return Err("construction is disabled".into());
        }
        let mut construction_plan = self
            .construction
            .as_ref()
            .map(|owner| {
                let sites = self
                    .work_sites
                    .as_ref()
                    .expect("validated construction sites");
                game_parallel_economy_building::Context {
                    owner,
                    sites,
                    people: &self.people,
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: self.rules.max_work_hunger,
                    tick,
                    manual: manual_build,
                    goal_needs: &goal_needs,
                    unwilling: &goal_building_refusals,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &construction_plan {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let demolition_plan = self
            .construction
            .as_ref()
            .zip(self.work_sites.as_ref())
            .map(|(owner, sites)| {
                game_parallel_economy_building::demolition::Context {
                    policies: &owner.demolition,
                    sites,
                    people: &self.people,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    manual_site: destroy_site.map(|(site, _)| site),
                    max_hunger: self.rules.max_work_hunger,
                    seed: self.seed,
                    tick,
                }
                .prepare()
            });
        if let Some(plan) = &demolition_plan
            && !plan.actors.is_empty()
        {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let manual_salvage = match player_transfer {
            Some(game_parallel_economy_interaction::PhysicalChoice::SalvageSite {
                person,
                site,
                good,
                quantity,
            }) => {
                if self.family.as_ref().is_some_and(|f| {
                    f.members
                        .iter()
                        .any(|m| m.id == person && m.age < f.rules.min_work_age)
                }) || self.physical.work().iter().any(|w| {
                    w.job.worker == person && w.phase == game_parallel_lot_runtime::Phase::Working
                }) {
                    return Err("person cannot salvage while underage or working".into());
                }
                Some(game_parallel_economy_building::salvage::Manual {
                    person,
                    site,
                    good,
                    quantity,
                })
            }
            _ => None,
        };
        if manual_salvage.is_some() && self.construction.is_none() {
            return Err("site salvage is disabled".into());
        }
        if let Some(plan) = &construction_plan {
            action_base = action_base
                .checked_add(plan.actions.len() as u64)
                .ok_or("action count overflow")?;
        }
        let salvage_plan = self
            .construction
            .as_ref()
            .map(|owner| {
                game_parallel_economy_building::salvage::Context {
                    policies: &owner.salvaging,
                    materials: &owner.materials,
                    people: &self.people,
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    claims: claims.as_ref(),
                    busy: &busy,
                    max_hunger: self.rules.max_work_hunger,
                    manual: manual_salvage,
                }
                .prepare(action_base)
            })
            .transpose()?;
        if let Some(plan) = &salvage_plan {
            game_parallel_economy_work::reserve(&mut busy, plan.actors.iter().copied());
        }
        let manual_animal_feed =
            if let Some(game_parallel_economy_interaction::PhysicalChoice::FeedAnimal {
                person,
                animal,
                good,
                quantity,
            }) = player_transfer
            {
                let actor = self
                    .people
                    .binary_search_by_key(&person, |p| p.id)
                    .ok()
                    .map(|i| &self.people[i])
                    .ok_or("unknown person")?;
                let food = game_parallel_fauna::Feeding {
                    animal,
                    person: Some(person),
                    good,
                    quantity,
                };
                if !self.physical.goods().iter().any(|item| item.id == good) {
                    return Err("unknown good".into());
                }
                let region = self
                    .fauna
                    .as_ref()
                    .ok_or("wildlife is disabled")?
                    .check_feeding(food)
                    .map_err(str::to_owned)?;
                if !actor.controlled
                    || actor.health == 0
                    || actor.region != Some(region)
                    || self.family.as_ref().is_some_and(|f| {
                        f.members
                            .iter()
                            .any(|m| m.id == person && m.age < f.rules.min_work_age)
                    })
                    || self.physical.work().iter().any(|w| {
                        w.job.worker == person
                            && w.phase == game_parallel_lot_runtime::Phase::Working
                    })
                {
                    return Err("feeding requires an available local person and animal".into());
                }
                let id = action_base
                    .checked_add(salvage_plan.as_ref().map_or(0, |p| p.actions.len() as u64))
                    .ok_or("action count overflow")?;
                id.checked_add(1).ok_or("action count overflow")?;
                Some((
                    id,
                    food,
                    Transaction {
                        id,
                        movements: vec![game_parallel_lots::Movement {
                            id,
                            from: game_parallel_lots::Place::Person(person),
                            to: None,
                            good,
                            quantity,
                        }],
                        creations: vec![],
                    },
                ))
            } else {
                None
            };
        game_parallel_economy_claims::include_care_farming!(claim_uses, care, farming);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.work_admission");
        }
        let mut actions: Vec<Transaction> = meals.actions().to_vec();
        let taking_actions = taking_plan
            .as_mut()
            .map_or_else(Vec::new, |plan| plan.take_actions());
        if let Some((_, _, action)) = &manual_animal_feed {
            actions.push(action.clone());
        }
        if let Some(plan) = tool_pickup {
            actions.extend(plan.actions);
            claim_uses.extend(plan.uses);
        }
        if let Some(plan) = &mut construction_plan {
            actions.append(&mut plan.actions);
            claim_uses.append(&mut plan.uses);
        }
        if let Some(plan) = &salvage_plan {
            actions.extend_from_slice(&plan.actions);
        }
        if let Some(plan) = &care {
            actions.extend_from_slice(&plan.actions);
        }
        actions.extend(pickup.actions);
        actions.extend(transfer);
        actions.extend_from_slice(game_parallel_economy_exchange::actions(trade.as_ref()));
        if let Some(plan) = &promises.delivery {
            actions.extend_from_slice(plan.actions());
        }
        actions.extend_from_slice(&treatment.actions);
        actions.append(&mut giving_plan.actions);
        actions.append(&mut expedition_acquisition.actions);
        actions.extend_from_slice(&medical_plan.actions);
        if let Some(plan) = &hauling_plan {
            actions.extend_from_slice(&plan.actions);
        }
        if let Some(plan) = &local_plan {
            actions.extend_from_slice(&plan.actions);
        }
        if let Some(plan) = &goal_fetch_plan {
            actions.extend_from_slice(&plan.actions);
        }
        let mut compact =
            game_parallel_economy_agriculture::append_planting(&mut farming, &mut actions);
        // Animal care needs the acceptance receipt, not copies of consumed lot cohorts.
        compact.par_extend(animal_care_plan.actions.par_iter().map(|action| action.id));
        actions.append(&mut animal_care_plan.actions);
        let practice_requests: Vec<_> = self
            .giving
            .as_ref()
            .and_then(|giving| giving.practices.as_ref())
            .map_or(&[][..], |practices| practices.minds.as_slice())
            .par_iter()
            .filter_map(|mind| {
                let work = mind.work.as_ref()?;
                let proposal = work.authored.as_ref()?;
                let index = self
                    .people
                    .binary_search_by_key(&mind.person, |p| p.id)
                    .ok()?;
                (work.ask_willing
                    && work.engagement.is_none()
                    && proposal.expires > tick
                    && self.people[index].health > 0
                    && !self.people[index].controlled)
                    .then_some(game_parallel_economy_work::PracticeRequest {
                        requester: mind.person,
                        worker: proposal.worker,
                        recipe: proposal.recipe,
                        expires: proposal.expires,
                    })
            })
            .collect();
        let mut work = game_parallel_economy_goals::prepare_work!(
            self,
            view,
            people,
            claims,
            busy,
            tick,
            goal_work,
            site_classes,
            work_offers,
            practice_requests
        );
        let starts =
            game_parallel_economy_claims::work_actions!(work, claims, self, claim_uses, busy);
        let (starts, manual_job) = game_parallel_economy_interaction::work_starts!(
            self,
            player_work,
            work,
            starts,
            claims,
            claim_uses
        );
        let starts = if let Some((request, accept)) = manual_response {
            // The physical candidate is already in `starts`; retain it in the
            // request plan too, so only an admitted job produces an outcome.
            let starts = starts.into_owned();
            let start = accept
                .then(|| starts.iter().find(|s| s.worker == request.worker).copied())
                .flatten();
            work.as_mut()
                .expect("validated request planner")
                .record_manual_response(
                    game_parallel_autowork::requests::Decision {
                        request: request.id,
                        worker: request.worker,
                        region: request.region,
                        recipe: request.recipe,
                        help: accept,
                        disregard_claims: true,
                    },
                    start,
                    manual_job,
                );
            std::borrow::Cow::Owned(starts)
        } else {
            starts
        };
        let starts = if self.work_tools.is_empty() {
            starts
        } else {
            rates = rates
                .into_par_iter()
                .filter(|(id, _)| {
                    self.physical
                        .work()
                        .binary_search_by_key(id, |w| w.job.id)
                        .ok()
                        .is_some_and(|i| {
                            let job = &self.physical.work()[i].job;
                            game_parallel_economy_work::tools::available(
                                &self.work_tools,
                                &view,
                                job.worker,
                                job.recipe,
                            )
                        })
                })
                .collect();
            std::borrow::Cow::Owned(
                starts
                    .par_iter()
                    .filter(|start| {
                        game_parallel_economy_work::tools::available(
                            &self.work_tools,
                            &view,
                            start.worker,
                            start.recipe,
                        )
                    })
                    .copied()
                    .collect(),
            )
        };
        let mut site_disregards = work
            .as_ref()
            .map_or_else(Vec::new, |plan| plan.site_disregards.clone());
        if !site_classes.is_empty() {
            site_disregards.extend(player_work.map(|choice| choice.actor()));
            site_disregards.par_sort_unstable();
            site_disregards.dedup();
        }
        let site_plan = self.work_sites.as_ref().map(|sites| {
            sites.prepare_access(
                &starts,
                &game_parallel_economy_work::sites::access::Access {
                    classes: &site_classes,
                    disregards: &site_disregards,
                },
                self.seed,
                tick,
            )
        });
        let starts = match (&self.work_sites, &site_plan) {
            (Some(sites), Some(plan)) => {
                std::borrow::Cow::Owned(sites.filter_starts(&starts, plan))
            }
            _ => starts,
        };
        let farm_harvest = farming.as_ref().map_or(&[][..], |p| p.harvest.as_slice());
        let gathered_harvest = gathering_plan
            .as_ref()
            .map_or(&[][..], |p| p.actions.as_slice());
        let combined_harvest: Option<Vec<_>> = (!gathered_harvest.is_empty()).then(|| {
            farm_harvest
                .par_iter()
                .chain(gathered_harvest.par_iter())
                .cloned()
                .collect()
        });
        let harvest = combined_harvest.as_deref().unwrap_or(farm_harvest);
        let staged_custody = construction_plan
            .as_ref()
            .map_or(&[][..], |plan| plan.custody.as_slice());
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.physical");
        }
        let flat = if taking_actions.len().saturating_add(actions.len()) >= 128
            && rates.is_empty()
            && starts.is_empty()
            && abandon.is_empty()
            && harvest.is_empty()
            && hunting_plan.yields.is_empty()
            && compact.is_empty()
            && staged_custody.is_empty()
            && actions.par_iter().all(|action| {
                action.movements.len() == 1
                    && action.creations.is_empty()
                    && action.movements[0].to.is_some()
            }) {
            let combined =
                (!actions.is_empty()).then(|| {
                    taking_actions
                        .par_iter()
                        .copied()
                        .chain(actions.par_iter().map(|action| {
                            game_parallel_lots::SimpleTransfer {
                                action: action.id,
                                movement: action.movements[0],
                            }
                        }))
                        .collect::<Vec<_>>()
                });
            self.physical.prepare_partitioned_flat_transfers(
                combined.as_deref().unwrap_or(&taking_actions),
                self.seed,
            )?
        } else {
            None
        };
        let mut physical = if let Some(draft) = flat {
            draft
        } else {
            actions.par_extend(taking_actions.into_par_iter().map(|row| Transaction {
                id: row.action,
                movements: vec![row.movement],
                creations: vec![],
            }));
            self.physical.prepare_with_growth_and_custody(
                &rates,
                &actions,
                &starts,
                &abandon,
                (
                    game_parallel_lot_runtime::Growth {
                        harvest,
                        animal_yield: &hunting_plan.yields,
                        compact_consumption: &compact,
                    },
                    staged_custody,
                ),
                self.seed,
            )?
        };
        let fatigue = game_parallel_economy_fatigue::prepare_world!(
            self,
            (rates, starts, physical),
            (
                farming,
                gathering_plan,
                medical_plan.caregivers(),
                hauling_plan,
                care,
                fighters
            ),
            (resting, busy, meal_candidates, player_transfer)
        );
        actions.into_par_iter().for_each(drop);
        let given =
            manual_give.map(|(person, _, _, _)| giving_plan.delivered(person, physical.events()));
        if let Some(giving) = &mut next_giving {
            giving_plan.learn_sharing(giving, physical.events(), self.physical.goods());
            giving_plan.clear_fulfilled_requests(giving, physical.events(), self.physical.goods());
        }
        let gathered = manual_gather.map(|(person, _)| {
            gathering_plan
                .as_ref()
                .is_some_and(|p| p.admitted(person, physical.events()))
        });
        let fed_animal = manual_animal_feed.as_ref().map(|(id, _, _)| {
            physical
                .events()
                .physical_actions
                .iter()
                .any(|event| event.id == *id && event.accepted)
        });
        let hunted_animals = hunting_plan.accepted(physical.events());
        let hunted = manual_hunt.map(|(person, _)| hunting_plan.caught(person, &hunted_animals));
        let mut feedings = animal_care_plan.accepted(physical.events());
        feedings.extend(
            manual_animal_feed
                .as_ref()
                .filter(|_| fed_animal == Some(true))
                .map(|(_, food, _)| *food),
        );
        let animal_feelings_update = self.animal_feelings.as_ref().and_then(|owner| {
            let mut experiences: Vec<_> = feedings
                .par_iter()
                .filter_map(|food| {
                    food.person.map(|person| animal_feelings::Event {
                        person,
                        animal: food.animal,
                        kind: animal_feelings::Experience::Feed,
                    })
                })
                .collect();
            experiences.par_extend(hunting_plan.yields.par_iter().filter_map(|yield_| {
                hunted_animals
                    .binary_search(&yield_.animal)
                    .is_ok()
                    .then_some(animal_feelings::Event {
                        person: yield_.hunter,
                        animal: yield_.animal,
                        kind: animal_feelings::Experience::Hunt,
                    })
            }));
            experiences.par_extend(handling_plan.events.par_iter().filter_map(|event| {
                let kind = match event.outcome {
                    AnimalHandlingOutcome::Accepted => animal_feelings::Experience::HandleAccepted,
                    AnimalHandlingOutcome::Refused => animal_feelings::Experience::HandleRefused,
                    AnimalHandlingOutcome::Unavailable => return None,
                };
                Some(animal_feelings::Event {
                    person: event.person,
                    animal: event.animal,
                    kind,
                })
            }));
            experiences.par_extend(
                next_animal_restraints
                    .as_ref()
                    .map_or(&[][..], |owner| owner.events.as_slice())
                    .par_iter()
                    .filter_map(|event| {
                        let person = event.actor?;
                        let kind = match event.kind {
                            AnimalRestraintEventKind::Yielded => {
                                animal_feelings::Experience::CaptureYielded
                            }
                            AnimalRestraintEventKind::Forced => {
                                animal_feelings::Experience::CaptureForced
                            }
                            AnimalRestraintEventKind::Resisted => {
                                animal_feelings::Experience::CaptureResisted
                            }
                            AnimalRestraintEventKind::Released => {
                                animal_feelings::Experience::Released
                            }
                            AnimalRestraintEventKind::ReleaseBlocked => {
                                animal_feelings::Experience::ReleaseBlocked
                            }
                            _ => return None,
                        };
                        Some(animal_feelings::Event {
                            person,
                            animal: event.animal,
                            kind,
                        })
                    }),
            );
            experiences.par_extend(
                next_animal_restraints
                    .as_ref()
                    .map_or(&[][..], |owner| owner.events.as_slice())
                    .par_iter()
                    .filter_map(|event| {
                        let person = event.defender?;
                        let kind = match event.kind {
                            AnimalRestraintEventKind::ReleaseBlocked => {
                                animal_feelings::Experience::DefenseSucceeded
                            }
                            AnimalRestraintEventKind::Released => {
                                animal_feelings::Experience::DefenseOvercome
                            }
                            _ => return None,
                        };
                        Some(animal_feelings::Event {
                            person,
                            animal: event.animal,
                            kind,
                        })
                    }),
            );
            let relevant = owner.relevant(experiences);
            (!relevant.is_empty()).then(|| owner.advance(&relevant))
        });
        let mut gathering = gathering_plan
            .map(|p| {
                p.publish(
                    self.gathering.as_ref().expect("gathering owner"),
                    physical.events(),
                    self.environment.as_ref().and_then(|env| env.season(tick)),
                )
            })
            .transpose()?;
        // Human gathering and renewal have already been staged. Wildlife now
        // competes for the remaining physical patches in the same draft.
        let fauna = if let Some(fauna) = &self.fauna {
            #[cfg(feature = "profile")]
            let _profile = game_diagnostics::span("economy.fauna.advance");
            let patches: Vec<_> = gathering
                .as_ref()
                .map_or(&[][..], |g| g.nodes.as_slice())
                .par_iter()
                .map(|node| game_parallel_fauna::Patch {
                    id: node.id,
                    region: node.region,
                    good: node.good,
                    available: node.available,
                })
                .collect();
            let season = self.environment.as_ref().and_then(|env| env.season(tick));
            let exposure: Vec<_> = self
                .environment
                .as_ref()
                .map_or(&[][..], |env| env.regions.as_slice())
                .par_iter()
                .map(|hazard| game_parallel_fauna::Exposure {
                    region: hazard.region,
                    severity: season.map_or(hazard.damage, |season| season.exposure(hazard.damage)),
                })
                .collect();
            let quiet = fauna
                .prepare_compact(
                    &patches,
                    &feedings,
                    &exposure,
                    &hunted_animals,
                    self.seed,
                    tick,
                )
                .map_err(str::to_owned)?;
            let mut deduct_grazing = |consumed: &[(u64, u64)]| -> Result<(), String> {
                if let Some(gathering) = &mut gathering {
                    gathering
                        .nodes
                        .par_iter_mut()
                        .try_for_each(|node| -> Result<(), String> {
                            if let Ok(i) = consumed.binary_search_by_key(&node.id, |row| row.0) {
                                node.available = node
                                    .available
                                    .checked_sub(consumed[i].1)
                                    .ok_or("animal grazing exceeds physical resource")?;
                            }
                            Ok(())
                        })?;
                } else if !consumed.is_empty() {
                    return Err("animal grazing requires physical resources".into());
                }
                Ok(())
            };
            if let Some(quiet) = quiet {
                deduct_grazing(quiet.consumed())?;
                Some(FaunaDraft::Quiet {
                    lifecycle: quiet,
                    handling: handling_plan.events,
                    reactions: restraint_plan.reactions,
                })
            } else {
                let mut draft = fauna
                    .advance_with_hunts(
                        &patches,
                        &feedings,
                        &exposure,
                        &hunted_animals,
                        self.seed,
                        tick,
                    )
                    .map_err(str::to_owned)?;
                game_parallel_fauna::handling::apply(&mut draft.fauna, &handling_plan.events);
                game_parallel_fauna::restraint::apply_reactions(
                    &mut draft.fauna,
                    &restraint_plan.reactions,
                );
                deduct_grazing(&draft.consumed)?;
                Some(FaunaDraft::Complete(draft.fauna))
            }
        } else {
            None
        };
        let mut agriculture = game_parallel_economy_agriculture::publish_optional(
            farming,
            self.agriculture.as_ref(),
            physical.events(),
        )?;
        let effects = meals.effects(physical.events())?;
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.person");
        }
        let mut person = person::prepare(
            &self.people,
            &self.rules,
            self.nutrition.as_ref(),
            &self.pending_food,
            effects,
            tick,
        );
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.dynamics");
        }
        let mut combat = self
            .combat
            .as_ref()
            .map(|combat| combat.prepare(&people, &self.people, tick, self.seed, &view));
        let social_enabled = self.dynamics.as_ref().is_some_and(|d| d.social.is_some());
        let mut occupied_held = Vec::new();
        if social_enabled && !available_held.is_empty() {
            occupied_held.par_extend(treatment.actors.par_iter().copied());
            occupied_held.par_extend(asked_actors.par_iter().copied());
            occupied_held.par_extend(giving_plan.actors.par_iter().copied());
            occupied_held.par_extend(medical_plan.actors.par_iter().copied());
            occupied_held.par_extend(handling_plan.actors.par_iter().copied());
            occupied_held.par_extend(restraint_plan.actors.par_iter().copied());
            occupied_held.par_sort_unstable();
        }
        let meeting_held = if occupied_held.is_empty() {
            available_held.as_slice()
        } else {
            &[]
        };
        let meeting_held_filtered = (!occupied_held.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&available_held, &occupied_held));
        let meeting_held = meeting_held_filtered.as_deref().unwrap_or(meeting_held);
        let social_with_children = (social_enabled && !free_underage.is_empty()).then(|| {
            let mut occupied = medical_plan.actors.clone();
            game_parallel_economy_work::reserve(
                &mut occupied,
                handling_plan.actors.iter().copied(),
            );
            game_parallel_economy_work::reserve(
                &mut occupied,
                restraint_plan.actors.iter().copied(),
            );
            let mut available =
                game_parallel_economy_work::exclude_sorted(&free_underage, &occupied);
            game_parallel_economy_work::reserve(&mut available, meeting_held.iter().copied());
            available
        });
        let social_available = social_with_children.as_deref().unwrap_or(meeting_held);
        let social_busy = (social_enabled && !social_available.is_empty())
            .then(|| game_parallel_economy_work::exclude_sorted(&busy, social_available));
        let social_partition = |person| communication.partition(person);
        let social_access: Option<&(dyn Fn(u64) -> (u64, u64) + Sync)> =
            (social_enabled && !held.is_empty()).then_some(&social_partition);
        let social_speak = |a, b| {
            let a = self
                .people
                .binary_search_by_key(&a, |person| person.id)
                .ok()
                .map(|index| &self.people[index]);
            let b = self
                .people
                .binary_search_by_key(&b, |person| person.id)
                .ok()
                .map(|index| &self.people[index]);
            a.zip(b).is_some_and(|(a, b)| {
                a.region.is_some() && a.region == b.region && communication.allows(a.id, b.id)
            })
        };
        let social_opening = |id| {
            let person = &self.people[self
                .people
                .binary_search_by_key(&id, |person| person.id)
                .ok()?];
            let region = person.region?;
            let (before, after) = communication.partition(id);
            if before == 0 && after == 0 {
                Some((region, false))
            } else if before != 0
                && before == after
                && communication
                    .before
                    .is_some_and(|owner| owner.can_receive_supplies(id))
                && communication
                    .after
                    .is_some_and(|owner| owner.can_receive_supplies(id))
            {
                Some((region, true))
            } else {
                None
            }
        };
        let open_conversations = social_access.is_some()
            && communication
                .before
                .is_some_and(|owner| owner.cells.iter().any(|cell| cell.allow_supplies));
        let mut dynamics = game_parallel_economy_dynamics::prepare_dynamics!(
            self,
            person,
            social_busy.as_deref().unwrap_or(&busy),
            tick,
            treatment,
            physical,
            care,
            interaction,
            medical_plan,
            combat,
            social_access,
            social_access.map(|_| &care_contact as &(dyn Fn(u64, u64) -> bool + Sync)),
            social_access.map(|_| &social_speak as &(dyn Fn(u64, u64) -> bool + Sync)),
            open_conversations
                .then_some(&social_opening as &(dyn Fn(u64) -> Option<(u64, bool)> + Sync))
        );
        medical_plan.apply(
            dynamics.as_mut().and_then(|d| d.disease.as_mut()),
            physical.events(),
        );
        let treated_other = manual_medical.map(|(actor, patient, _, _)| {
            medical_plan.attempted(actor, patient, physical.events())
        });
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.purpose_and_health");
        }
        let blocked = game_parallel_economy_goals::blocked(
            self.goals.as_ref(),
            self.autowork.as_ref(),
            work.as_ref(),
            &people,
            self.rules.max_work_hunger,
            physical.events(),
            self.physical.work(),
        );
        let mut purpose = purpose::prepare(
            self.purpose.as_ref(),
            self.physical.work(),
            physical.events(),
            &blocked,
            &mut person.people,
            tick,
        );
        if let Some(combat) = &combat {
            combat.apply(&mut person.people);
        }
        game_parallel_economy_fatigue::apply(fatigue.as_ref(), &mut person.people);
        environment::apply(
            self.environment.as_ref(),
            &view,
            self.travel.as_ref(),
            &mut person.people,
            tick,
        );
        recovery::apply(
            self.recovery.as_ref(),
            &mut person.people,
            &resting,
            fighters,
            player_transfer,
            tick,
            self.rules.max_health,
        );
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.knowledge_and_skills");
        }
        let mut knowledge = game_parallel_economy_knowledge::prepare_world!(self, dynamics);
        let mut skills = game_parallel_economy_skills::advance(
            self.skills.as_ref(),
            self.physical.work(),
            &rates,
            physical.events(),
            (
                dynamics.as_ref().and_then(|d| d.social.as_ref()),
                self.opinions.as_ref(),
            ),
            &self.people,
            tick,
        );
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.travel");
        }
        let mut next_care = self.care.clone();
        if let (Some(plan), Some(care)) = (care, &mut next_care) {
            plan.apply(care, &mut person.people);
        }
        let mut travel = self
            .travel
            .as_ref()
            .map(|travel| travel.advance(&mut person.people));
        let mut food_search = self.food_search.clone();
        let alive_and_local = |d: &Departure| {
            held.binary_search(&d.person).is_err()
                && person
                    .people
                    .binary_search_by_key(&d.person, |p| p.id)
                    .ok()
                    .is_some_and(|i| {
                        person.people[i].health > 0 && person.people[i].region.is_some()
                    })
        };
        let search_departures: Vec<_> = search_plan
            .as_ref()
            .map_or(&[][..], |plan| &plan.departures)
            .par_iter()
            .copied()
            .filter(alive_and_local)
            .collect();
        let mut departures: Vec<_> = hauling_plan
            .as_ref()
            .map_or(&[][..], |plan| &plan.departures)
            .par_iter()
            .copied()
            .filter(alive_and_local)
            .collect();
        departures.par_extend(search_departures.par_iter().copied());
        departures.par_extend(
            guardian_departures
                .par_iter()
                .copied()
                .filter(alive_and_local),
        );
        if !departures.is_empty() {
            let current_travel = travel.as_mut().ok_or("departures require travel")?;
            let working = person::travel::working(self.physical.work());
            let (next_travel, next_people) =
                current_travel.depart(&person.people, &departures, &working)?;
            *current_travel = next_travel;
            person.people = next_people;
        }
        if let (Some(plan), Some(search)) = (search_plan, food_search.as_mut()) {
            search.publish(&search_departures, &plan.targets, &plan.abandoned, tick)?;
        }
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.family");
        }
        let family_partition = |person| communication.partition(person);
        let family_access: Option<&(dyn Fn(u64) -> (u64, u64) + Sync)> =
            (!held.is_empty()).then_some(&family_partition);
        let (family, campaign, births) = family::advance_world!(
            self,
            person,
            travel,
            next_care,
            physical,
            dynamics,
            campaign_draft,
            tick,
            skills,
            knowledge,
            family_access
        );
        if !births.is_empty()
            && let Some((next, previous)) = dynamics
                .as_mut()
                .and_then(|owner| owner.disease.as_mut())
                .zip(
                    self.dynamics
                        .as_ref()
                        .and_then(|owner| owner.disease.as_ref()),
                )
        {
            let pairs: Vec<_> = births
                .par_iter()
                .map(|birth| (birth.id, birth.mother))
                .collect();
            next.admit_births_validated(previous, &pairs, self.seed, tick);
        }
        if let Some(confinement) = &mut next_confinement
            && !births.is_empty()
        {
            let placements: Vec<_> = births
                .par_iter()
                .map(|birth| game_parallel_economy_confinement::Birth {
                    child: birth.id,
                    mother: birth.mother,
                })
                .collect();
            confinement.enroll_births(&placements);
        }
        if let Some(travel) = &mut travel {
            travel.settle_deaths(&mut person.people);
        }
        purpose::enroll(purpose.as_mut(), family.as_ref(), tick)?;
        game_parallel_economy_agriculture::enroll(agriculture.as_mut(), family.as_ref(), tick);
        game_parallel_economy_claims::enroll(
            dynamics.as_mut().and_then(|owner| owner.claims.as_mut()),
            family.as_ref(),
            tick,
        );
        giving_inheritance::enroll(next_giving.as_mut(), family.as_ref(), tick);
        gathering::enroll(gathering.as_mut(), family.as_ref(), tick);
        let hunting_births = hunting::enroll(self.hunting.as_ref(), family.as_ref(), tick);
        let animal_care_births =
            animal_care::enroll(self.animal_care.as_ref(), family.as_ref(), tick);
        let animal_handling_births =
            animal_handling::enroll(self.animal_handling.as_ref(), family.as_ref(), tick);
        animal_restraint_inheritance::enroll(
            next_animal_restraints.as_mut(),
            family.as_ref(),
            tick,
        );
        food_search::enroll(food_search.as_mut(), family.as_ref(), tick);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.sites");
        }
        let incapacitated: Vec<_> = person
            .people
            .par_iter()
            .filter_map(|person| {
                (person.health == 0
                    && self
                        .people
                        .binary_search_by_key(&person.id, |p| p.id)
                        .ok()
                        .is_some_and(|i| self.people[i].health > 0))
                .then_some(person.id)
            })
            .collect();
        self.physical
            .interrupt_workers_in(&mut physical, &incapacitated)?;
        let mut work_sites = self
            .work_sites
            .as_ref()
            .zip(site_plan.as_ref())
            .map(|(sites, plan)| sites.advance(plan, physical.events()));
        let mut construction = self.construction.clone();
        if let (Some(plan), Some(owner), Some(sites)) = (
            construction_plan,
            construction.as_mut(),
            work_sites.as_mut(),
        ) {
            plan.publish(owner, sites, physical.events());
        } else if let Some(owner) = &mut construction {
            owner.events.clear();
        }
        let mut manually_destroyed = None;
        if let (Some((site, was_occupied)), Some(sites)) = (destroy_site, &mut work_sites)
            && !was_occupied
            && site_plan
                .as_ref()
                .is_none_or(|plan| !plan.reserves_site(site))
            && sites.destroy_idle(site)
            && let Some(game_parallel_economy_interaction::PhysicalChoice::DestroySite {
                person,
                ..
            }) = player_transfer
        {
            manually_destroyed = Some((person, site));
        }
        if let (Some(plan), Some(owner), Some(prior), Some(sites)) = (
            demolition_plan,
            construction.as_mut(),
            self.work_sites.as_ref(),
            work_sites.as_mut(),
        ) {
            owner.demolition_events = plan.publish(tick, prior, site_plan.as_ref(), sites);
        }
        if let Some(owner) = &mut construction {
            let mut destroyed: Vec<_> = owner
                .demolition_events
                .par_iter()
                .filter(|event| event.destroyed)
                .map(|event| event.site)
                .chain(manually_destroyed.map(|(_, site)| site).into_par_iter())
                .collect();
            if !destroyed.is_empty() {
                destroyed.par_sort_unstable();
                owner
                    .materials
                    .par_iter_mut()
                    .filter(|material| destroyed.binary_search(&material.site).is_ok())
                    .for_each(|material| material.debris = true);
            }
        }
        if let (Some(confinement), Some(sites)) = (&mut next_confinement, &work_sites) {
            let built: Vec<_> = construction
                .as_ref()
                .map_or(&[][..], |owner| owner.events.as_slice())
                .par_iter()
                .filter_map(|event| event.site)
                .collect();
            confinement.enroll_sites(&built, sites);
            confinement.retain_sites(sites);
        }
        if let Some(owner) = &mut construction {
            owner.salvage_events =
                salvage_plan.map_or_else(Vec::new, |plan| plan.publish(tick, physical.events()));
        }
        if let Some((prior, owner)) = self.construction.as_ref().zip(construction.as_mut())
            && !prior.materials.is_empty()
        {
            let mut exhausted = game_parallel_economy_building::salvage::exhausted(
                &prior.materials,
                &view,
                &owner.salvage_events,
            );
            if !exhausted.is_empty() {
                exhausted.par_sort_unstable();
                self.physical
                    .retire_empty_in(&mut physical, exhausted.clone())?;
                owner.materials = std::mem::take(&mut owner.materials)
                    .into_par_iter()
                    .filter(|row| {
                        exhausted
                            .binary_search(&game_parallel_lots::Place::Site(row.site))
                            .is_err()
                    })
                    .collect();
            }
        }
        if let Some(claims) = dynamics.as_mut().and_then(|d| d.claims.as_mut())
            && let Some(construction) = &construction
            && claims.preferences.par_iter().any(|p| p.assert_built_sites)
        {
            let built: Vec<_> = construction
                .events
                .par_iter()
                .filter_map(|event| event.site.map(|site| (event.person, site)))
                .collect();
            game_parallel_economy_claims::publish_built_site_claims(claims, &built, &person.people);
        }
        if let Some(owner) = dynamics.as_mut().and_then(|d| d.claims.as_mut()) {
            let mut uses = Vec::new();
            if !site_classes.is_empty() {
                let mut workers: Vec<_> = starts
                    .par_iter()
                    .map(|start| (start.id, start.worker))
                    .collect();
                workers.par_sort_unstable_by_key(|row| row.0);
                if let Some(plan) = &site_plan {
                    uses.par_extend(plan.bindings().par_iter().filter_map(|binding| {
                        if physical
                            .events()
                            .started
                            .binary_search(&binding.job)
                            .is_err()
                        {
                            return None;
                        }
                        let person = workers
                            .binary_search_by_key(&binding.job, |row| row.0)
                            .ok()
                            .map(|i| workers[i].1)?;
                        Some(game_parallel_economy_claims::SiteUse {
                            person,
                            site: binding.site,
                            destroyed: false,
                            salvaged: false,
                        })
                    }));
                }
                if let Some((person, site)) = manually_destroyed {
                    uses.push(game_parallel_economy_claims::SiteUse {
                        person,
                        site,
                        destroyed: true,
                        salvaged: false,
                    });
                }
                if let Some(construction) = &construction {
                    uses.par_extend(
                        construction
                            .demolition_events
                            .par_iter()
                            .filter(|e| e.destroyed)
                            .map(|e| game_parallel_economy_claims::SiteUse {
                                person: e.person,
                                site: e.site,
                                destroyed: true,
                                salvaged: false,
                            }),
                    );
                    uses.par_extend(
                        construction
                            .salvage_events
                            .par_iter()
                            .filter(|e| e.recovered)
                            .map(|e| game_parallel_economy_claims::SiteUse {
                                person: e.person,
                                site: e.site,
                                destroyed: false,
                                salvaged: true,
                            }),
                    );
                }
            }
            let mut sites: Vec<_> = if uses.is_empty() {
                Vec::new()
            } else {
                self.work_sites.as_ref().map_or_else(Vec::new, |sites| {
                    sites
                        .sites
                        .par_iter()
                        .map(|site| (site.id, site.region))
                        .collect()
                })
            };
            if !uses.is_empty()
                && let Some(construction) = &self.construction
            {
                sites.par_extend(
                    construction
                        .materials
                        .par_iter()
                        .filter(|material| material.debris)
                        .map(|material| (material.site, material.region)),
                );
                if sites.par_windows(2).any(|pair| pair[0].0 > pair[1].0) {
                    sites.par_sort_unstable_by_key(|row| row.0);
                }
            }
            game_parallel_economy_claims::publish_site_uses(
                owner,
                &uses,
                &sites,
                &self.people,
                tick,
            );
        }
        game_parallel_economy_combat::finish_world!(combat, person);
        let started_confrontations =
            if let Some((combat, selected)) = combat.as_mut().zip(confrontation.as_ref()) {
                #[cfg(feature = "profile")]
                let _confrontation_span = game_diagnostics::span("economy.confrontation_start");
                combat.add_confrontations(selected, &person.people)?
            } else {
                Vec::new()
            };
        let mut next_taking = self
            .taking
            .clone()
            .or_else(|| manual_taking.map(|_| Taking::default()));
        if let (Some(owner), Some(plan)) = (&mut next_taking, &taking_plan) {
            owner.events = plan.publish(physical.events(), &started_confrontations);
        }
        let taking_outcome = manual_taking.and_then(|intent| {
            next_taking
                .as_ref()?
                .events
                .iter()
                .find(|event| event.actor == intent.actor && event.target == intent.target)
                .map(|event| event.outcome)
        });
        let mission_gifts = if self.missions.as_ref().is_some_and(|missions| {
            missions.entries.par_iter().any(|mission| {
                matches!(
                    mission.objective,
                    Some(game_parallel_economy_expeditions::Objective::Deliver { .. })
                ) && !mission.objective_met
            })
        }) {
            let mut transfers = giving_plan.practice_gifts(physical.events());
            transfers.par_extend(mission_deliveries::accepted_promises(
                promises.delivery.as_ref(),
                physical.events(),
            ));
            transfers
        } else {
            Vec::new()
        };
        let missions = game_parallel_economy_expeditions::finish_world!(
            self,
            expedition,
            person,
            travel,
            dynamics,
            mission_gifts,
            held,
            tick
        );
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.social");
        }
        let mut work_reactions = Vec::new();
        if let Some((giving, practices)) = self.giving.as_ref().and_then(|giving| {
            giving
                .practices
                .as_ref()
                .map(|practices| (giving, practices))
        }) {
            let encounters = dynamics
                .as_ref()
                .and_then(|d| d.social.as_ref())
                .map_or(&[][..], |social| social.events.as_slice());
            let track_work = practices.minds.par_iter().any(|mind| {
                mind.work.as_ref().is_some_and(|work| {
                    work.outcome_policy.is_some() || !work.experiences.is_empty()
                })
            });
            let started_work = if track_work {
                work.as_ref()
                    .zip(self.autowork.as_ref())
                    .map_or_else(Vec::new, |(plan, auto)| {
                        plan.started_requests(auto, physical.events())
                    })
            } else {
                Vec::new()
            };
            let (next, reactions) = giving_practices::advance(
                practices,
                giving_practices::Context {
                    giving,
                    people: &self.people,
                    goods: self.physical.goods(),
                    view: &view,
                    opinions: self.opinions.as_ref(),
                    goals: self.goals.as_ref(),
                    requests: self
                        .autowork
                        .as_ref()
                        .and_then(|auto| auto.requests.as_ref()),
                    work_asks: work.as_ref().map_or(&[][..], |plan| plan.asks()),
                    practice_requests: &practice_requests,
                    started_work: &started_work,
                    encounters,
                    gifts: &giving_plan,
                    promise_deliveries: promises
                        .delivery
                        .as_ref()
                        .map_or(&[], |plan| plan.actions()),
                    events: physical.events(),
                    tick,
                },
            )?;
            work_reactions = reactions;
            next_giving
                .as_mut()
                .expect("giving remains enabled")
                .practices = Some(next);
        }
        let mut next_market = self.market.clone();
        let mut next_opinions = game_parallel_economy_dynamics::next_opinions(&self.opinions);
        let mut report_changes = Vec::new();
        if let (Some(previous), Some(owner)) = (&self.taking, &mut next_taking)
            && !previous.report_policies.is_empty()
        {
            let contacts: Vec<_> = dynamics
                .as_ref()
                .and_then(|d| d.social.as_ref())
                .map_or(&[][..], |social| social.events.as_slice())
                .par_iter()
                .filter(|event| event.tick == tick)
                .map(|event| (event.a, event.b))
                .collect();
            if !contacts.is_empty() {
                let needs_feelings = previous
                    .report_policies
                    .par_iter()
                    .any(|policy| !policy.willing && policy.pressure_threshold.is_some())
                    || previous
                        .receptions
                        .par_iter()
                        .any(|rule| matches!(rule.stance, TakingReportStance::TrustAtLeast(_)));
                let opinions = self
                    .opinions
                    .as_ref()
                    .filter(|_| needs_feelings)
                    .map(Opinions::index);
                let feelings = |listener, speaker| {
                    opinions.as_ref().map_or((0, 0, 0), |index| {
                        let (fear, obligation) = index.feelings(speaker, listener);
                        (index.trust(listener, speaker), fear, obligation)
                    })
                };
                report_changes = owner.receive_reports(previous, &contacts, tick, &feelings);
            }
        }
        let mut witnessed_changes = Vec::new();
        if let Some(owner) = &mut next_taking
            && !owner.witnesses.is_empty()
            && !owner.events.is_empty()
        {
            let locations = Colocation::new(self.travel.as_ref());
            let can_observe = |observer, actor| {
                let find = |id| {
                    self.people
                        .binary_search_by_key(&id, |p| p.id)
                        .ok()
                        .map(|index| &self.people[index])
                };
                find(observer)
                    .zip(find(actor))
                    .is_some_and(|(observer, actor)| {
                        observer.health > 0
                            && actor.health > 0
                            && locations
                                .region((observer.id, observer.region), (actor.id, actor.region))
                                .is_some()
                            && self
                                .confinement
                                .as_ref()
                                .is_none_or(|c| c.can_touch(observer.id, actor.id))
                            && next_confinement
                                .as_ref()
                                .is_none_or(|c| c.can_touch(observer.id, actor.id))
                    })
            };
            let events = std::mem::take(&mut owner.events);
            witnessed_changes = owner.observe_witnesses(&events, &can_observe);
            owner.events = events;
        }
        if let (Some(opinions), Some(owner)) = (&mut next_opinions, &next_taking) {
            let mut changes = owner.personal_changes(&owner.events);
            changes.par_extend(witnessed_changes);
            changes.par_extend(report_changes);
            opinions.apply_personal_changes(&changes, tick);
            if opinions.tracks_opposed_action_reactions() {
                use game_parallel_opinions::{
                    OpposedActionExperience as Experience, OpposedActionOutcome as Outcome,
                };
                let experiences: Vec<_> = owner
                    .events
                    .par_iter()
                    .filter(|event| {
                        matches!(
                            event.outcome,
                            TakingOutcome::Resisted | TakingOutcome::Conflict
                        )
                    })
                    .flat_map_iter(|event| {
                        [
                            Experience {
                                observer: event.actor,
                                other: event.target,
                                outcome: Outcome::Blocked,
                            },
                            Experience {
                                observer: event.target,
                                other: event.actor,
                                outcome: Outcome::BlockedOther,
                            },
                        ]
                    })
                    .collect();
                opinions.learn_opposed_actions(&experiences, tick);
            }
        }
        if let Some(opinions) = &mut next_opinions {
            let changes: Vec<_> = work_reactions
                .par_iter()
                .map(|reaction| game_parallel_opinions::PersonalChange {
                    observer: reaction.observer,
                    target: reaction.target,
                    trust: reaction.trust,
                    affinity: reaction.affinity,
                    fear: reaction.fear,
                    obligation: reaction.obligation,
                })
                .collect();
            opinions.apply_personal_changes(&changes, tick);
        }
        if let (Some(opinions), Some(previous), Some(next)) =
            (&mut next_opinions, self.skills.as_ref(), skills.as_ref())
            && let Some(lessons) = next.lessons.get(previous.lessons.len()..)
            && !lessons.is_empty()
            && opinions.tracks_teaching_reactions()
        {
            let pairs: Vec<_> = lessons
                .par_iter()
                .map(|lesson| (lesson.teacher, lesson.student))
                .collect();
            teaching::interpret(opinions, &pairs, tick);
        }
        if let (Some(opinions), Some(confinement)) = (&mut next_opinions, &next_confinement)
            && confinement.events.par_iter().any(|event| {
                matches!(
                    event.kind,
                    game_parallel_economy_confinement::EventKind::Confined
                        | game_parallel_economy_confinement::EventKind::CaptureResisted
                        | game_parallel_economy_confinement::EventKind::Released
                )
            })
            && opinions.tracks_confinement_reactions()
        {
            let experiences: Vec<_> = confinement
                .events
                .par_iter()
                .filter_map(|event| {
                    use game_parallel_economy_confinement::EventKind;
                    let kind = match event.kind {
                        EventKind::Confined => {
                            game_parallel_opinions::ConfinementExperienceKind::Captured
                        }
                        EventKind::CaptureResisted => {
                            game_parallel_opinions::ConfinementExperienceKind::Resisted
                        }
                        EventKind::Released => {
                            game_parallel_opinions::ConfinementExperienceKind::Released
                        }
                        _ => return None,
                    };
                    let actor = event.actor?;
                    let observer = self
                        .people
                        .binary_search_by_key(&event.person, |person| person.id)
                        .ok()
                        .map(|i| &self.people[i])?;
                    (observer.health > 0).then_some(game_parallel_opinions::ConfinementExperience {
                        observer: observer.id,
                        actor,
                        kind,
                    })
                })
                .collect();
            opinions.learn_confinement(&experiences, tick);
        }
        if let (Some(opinions), Some(restraints)) = (&mut next_opinions, &next_animal_restraints)
            && restraints
                .events
                .par_iter()
                .any(|event| event.defender.is_some())
            && opinions.tracks_opposed_action_reactions()
        {
            #[cfg(feature = "profile")]
            let _reaction_timer = game_diagnostics::span("economy.opposed_relationships");
            use game_parallel_opinions::{
                OpposedActionExperience as Experience, OpposedActionOutcome as Outcome,
            };
            let (mut experiences, actors): (Vec<_>, Vec<_>) = rayon::join(
                || {
                    restraints
                        .events
                        .par_iter()
                        .filter_map(|event| {
                            let outcome = match event.kind {
                                AnimalRestraintEventKind::ReleaseBlocked => Outcome::BlockedOther,
                                AnimalRestraintEventKind::Released => Outcome::WasOvercome,
                                _ => return None,
                            };
                            Some(Experience {
                                observer: event.defender?,
                                other: event.actor?,
                                outcome,
                            })
                        })
                        .collect()
                },
                || {
                    restraints
                        .events
                        .par_iter()
                        .filter_map(|event| {
                            let outcome = match event.kind {
                                AnimalRestraintEventKind::ReleaseBlocked => Outcome::Blocked,
                                AnimalRestraintEventKind::Released => Outcome::Overcame,
                                _ => return None,
                            };
                            Some(Experience {
                                observer: event.actor?,
                                other: event.defender?,
                                outcome,
                            })
                        })
                        .collect()
                },
            );
            experiences.par_extend(actors);
            opinions.learn_opposed_actions(&experiences, tick);
        }
        development::enroll(&mut next_opinions, family.as_ref(), tick);
        if let Some(claims) = dynamics
            .as_ref()
            .and_then(|d| d.claims.as_ref())
            .filter(|c| !c.site_events.is_empty())
            && let Some(opinions) = next_opinions.as_mut()
        {
            let witnessed: Vec<_> = claims
                .site_events
                .par_iter()
                .filter(|e| !e.salvaged)
                .map(|e| (e.claimant, e.person, e.destroyed))
                .collect();
            opinions.learn_site_affinity(&witnessed);
            let recovered: Vec<_> = claims
                .site_events
                .par_iter()
                .filter(|e| e.salvaged)
                .map(|e| (e.claimant, e.person))
                .collect();
            opinions.learn_salvage_affinity(&recovered);
        }
        if let Some(opinions) = next_opinions
            .as_mut()
            .filter(|o| giving_plan.has_actions() && o.tracks_help())
        {
            opinions.learn_help(&giving_plan.help(physical.events()));
        }
        if let Some(opinions) = next_opinions
            .as_mut()
            .filter(|o| !medical_plan.actions.is_empty() && o.tracks_help())
        {
            opinions.learn_help(&medical_plan.help(physical.events()));
        }
        game_parallel_economy_combat::learn_world!(combat, next_opinions);
        game_parallel_economy_exchange::publish(
            trade.as_ref(),
            proposals,
            &mut next_market,
            &mut next_opinions,
            physical.events(),
        )?;
        let mut next_agreements = self.agreements.clone();
        if let Some(agreements) = &mut next_agreements {
            agreement::advance(
                agreements,
                promises.delivery.as_ref(),
                &promises.decisions,
                physical.events(),
                &people,
                Some(&locations),
                next_opinions.as_mut(),
            )?;
        }
        if let Some(agreements) = &mut next_agreements {
            game_parallel_economy_promises::offers::publish(
                agreements,
                &promise_offers,
                tick,
                next_opinions.as_mut(),
            )?;
        }
        game_parallel_economy_dynamics::learn_social(&mut next_opinions, dynamics.as_ref(), tick);
        if let Some(opinions) = next_opinions
            .as_mut()
            .filter(|o| o.tracks_report_discrepancy())
            && let Some((before, after)) = self.knowledge.as_ref().zip(knowledge.as_ref())
        {
            opinions.learn_report_discrepancies(
                &game_parallel_economy_knowledge::discrepancies(before, after, &self.people),
                tick,
            );
        }
        game_parallel_economy_claims::publish_claims!(
            dynamics,
            claim_uses,
            physical.events(),
            self.people,
            next_opinions
        );
        game_parallel_economy_claims_output::publish!(dynamics, claim_outputs, physical.events());
        development::advance(
            &mut next_opinions,
            next_care.as_ref(),
            self.family.as_ref(),
            tick,
        );
        agreement::decay(next_opinions.as_mut());
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.work_publish");
        }
        let mut autowork = self.autowork.clone();
        if let (Some(auto), Some(plan)) = (&mut autowork, work) {
            if let Some(opinions) = next_opinions.as_mut().filter(|o| {
                plan.may_have_work_experiences(physical.events()) && o.tracks_work_affinity()
            }) {
                opinions.learn_work_affinity(&plan.work_experiences(
                    auto,
                    physical.events(),
                    &people,
                ));
            }
            plan.publish(auto, physical.events(), &people);
        }
        if let Some((requests, family)) = autowork
            .as_mut()
            .and_then(|auto| auto.requests.as_mut())
            .zip(family.as_ref())
        {
            requests.enroll(family, tick);
        }
        game_parallel_economy_interaction::publish_work!(autowork, manual_job);
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.goals");
        }
        let (goals, mut dreams) = game_parallel_economy_goals::advance(
            self.goals.as_ref(),
            self.dreams.as_ref(),
            self.physical.work(),
            physical.events(),
            &person.people,
            &blocked,
            autowork.as_ref().and_then(|auto| auto.requests.as_ref()),
        )?;
        development::enroll_dream_learning(&mut dreams, family.as_ref(), tick);
        if !physical.events().finished.is_empty()
            && let Some(owner) = dreams.as_mut().filter(|owner| !owner.learning.is_empty())
            && let Some(requests) = autowork.as_ref().and_then(|auto| auto.requests.as_ref())
        {
            let observations: Vec<_> = requests
                .memories
                .par_iter()
                .filter(|memory| memory.last_success_at == Some(tick))
                .filter(|memory| {
                    person
                        .people
                        .binary_search_by_key(&memory.observer, |p| p.id)
                        .ok()
                        .is_some_and(|i| {
                            person.people[i].health > 0 && !person.people[i].controlled
                        })
                })
                .map(|memory| (memory.observer, memory.recipe))
                .collect();
            owner.learn(&observations, tick);
        }
        let mut next_local_transport = self.local_transport.clone();
        if let Some(owner) = &mut next_local_transport {
            owner.events =
                local_plan.map_or_else(Vec::new, |plan| plan.publish(tick, physical.events()));
            if let Some(plan) = goal_fetch_plan {
                owner
                    .events
                    .par_extend(plan.publish(tick, physical.events()));
                owner.events.par_sort_unstable_by_key(|event| event.person);
            }
        }
        if let (Some(owner), Some(fauna)) = (&mut next_animal_restraints, &fauna) {
            let available = |person| {
                next_confinement
                    .as_ref()
                    .is_none_or(|confinement| !confinement.held(person))
            };
            match fauna {
                FaunaDraft::Complete(fauna) => {
                    owner.reconcile(&person.people, fauna, &available, tick)
                }
                FaunaDraft::Quiet { lifecycle, .. } => owner.reconcile_with_deaths(
                    &person.people,
                    self.fauna.as_ref().expect("quiet fauna owner"),
                    &available,
                    tick,
                    lifecycle.deaths(),
                ),
            }
        }
        let animal_captured = manual_restraint.map(|choice| match choice {
            game_parallel_animal_restraint::Manual::Capture { person, animal } => {
                next_animal_restraints.as_ref().is_some_and(|owner| {
                    owner.events.iter().any(|event| {
                        event.animal == animal
                            && event.actor == Some(person)
                            && matches!(
                                event.kind,
                                AnimalRestraintEventKind::Yielded
                                    | AnimalRestraintEventKind::Forced
                            )
                    }) && owner
                        .holds
                        .binary_search_by_key(&animal, |hold| hold.animal)
                        .ok()
                        .is_some_and(|index| owner.holds[index].handler == person)
                })
            }
            _ => false,
        });
        let animal_defended = manual_restraint.map(|choice| match choice {
            game_parallel_animal_restraint::Manual::Defend { person, animal } => {
                next_animal_restraints.as_ref().is_some_and(|owner| {
                    owner.events.iter().any(|event| {
                        event.animal == animal
                            && event.defender == Some(person)
                            && event.kind == AnimalRestraintEventKind::ReleaseBlocked
                    }) && owner
                        .holds
                        .binary_search_by_key(&animal, |hold| hold.animal)
                        .ok()
                        .is_some_and(|index| owner.holds[index].handler == person)
                })
            }
            _ => false,
        });
        #[cfg(feature = "profile")]
        drop(profile_phase);
        Ok(Draft {
            epoch: self.epoch,
            physical,
            people: person.people,
            market: next_market,
            agreements: next_agreements,
            opinions: next_opinions,
            autowork,
            work_sites,
            construction,
            confinement: next_confinement,
            goals,
            dreams,
            nutrition: person.nutrition,
            pending_food: person.pending,
            travel,
            family,
            care: next_care,
            agriculture,
            dynamics,
            missions,
            campaign,
            skills,
            knowledge,
            combat,
            fatigue,
            purpose,
            gathering,
            fauna,
            animal_handling: next_animal_handling,
            animal_restraints: next_animal_restraints,
            taking: next_taking,
            gathered,
            food_search,
            giving: next_giving,
            hauling: next_hauling,
            local_transport: next_local_transport,
            given,
            treated_other,
            fed_animal,
            hunted,
            handled,
            animal_captured,
            animal_released,
            animal_defended,
            taking_outcome,
            hunting_births,
            animal_care_births,
            animal_handling_births,
            animal_feelings_update,
        })
    }
    pub fn commit(&mut self, draft: Draft) -> Result<Events, String> {
        #[cfg(feature = "profile")]
        let mut profile_phase = game_diagnostics::span("economy.commit_physical");
        if draft.epoch != self.epoch {
            return Err("stale economic draft".into());
        }
        let epoch = self
            .epoch
            .checked_add(1)
            .ok_or("economic generation overflow")?;
        self.physical.validate_advance(&draft.physical)?;
        if let Some(FaunaDraft::Quiet { lifecycle, .. }) = &draft.fauna {
            self.fauna
                .as_ref()
                .ok_or("missing fauna owner")?
                .validate_quiet(lifecycle)
                .map_err(str::to_owned)?;
        }
        let events = self.physical.commit_advance(draft.physical)?;
        #[cfg(feature = "profile")]
        {
            drop(profile_phase);
            profile_phase = game_diagnostics::span("economy.commit_owners");
        }
        // Large independent owner generations can be reclaimed together; join keeps cleanup inside commit.
        let parallel_reclaim = self.people.len() >= 16_384 && rayon::current_num_threads() > 1;
        self.people = draft.people;
        self.market = draft.market;
        self.agreements = draft.agreements;
        let old_opinions = std::mem::replace(&mut self.opinions, draft.opinions);
        self.autowork = draft.autowork;
        self.work_sites = draft.work_sites;
        self.construction = draft.construction;
        self.confinement = draft.confinement;
        self.goals = draft.goals;
        self.dreams = draft.dreams;
        self.nutrition = draft.nutrition;
        self.pending_food = draft.pending_food;
        self.travel = draft.travel;
        let old_family = std::mem::replace(&mut self.family, draft.family);
        self.care = draft.care;
        self.agriculture = draft.agriculture;
        let old_dynamics = std::mem::replace(&mut self.dynamics, draft.dynamics);
        self.missions = draft.missions;
        self.campaign = draft.campaign;
        self.skills = draft.skills;
        let old_knowledge = std::mem::replace(&mut self.knowledge, draft.knowledge);
        game_parallel_economy_combat::publish_world!(self, draft.combat);
        self.fatigue = draft.fatigue.map(|fatigue| fatigue.state);
        self.purpose = draft.purpose;
        self.gathering = draft.gathering;
        match draft.fauna {
            Some(FaunaDraft::Complete(fauna)) => self.fauna = Some(fauna),
            Some(FaunaDraft::Quiet {
                lifecycle,
                handling,
                reactions,
            }) => {
                let fauna = self.fauna.as_mut().expect("validated fauna owner");
                fauna.publish_quiet_validated(lifecycle);
                game_parallel_fauna::handling::apply(fauna, &handling);
                game_parallel_fauna::restraint::apply_reactions(fauna, &reactions);
            }
            None => self.fauna = None,
        }
        self.animal_handling = draft.animal_handling;
        self.animal_restraints = draft.animal_restraints;
        self.taking = draft.taking;
        if let Some(feelings) = draft.animal_feelings_update {
            self.animal_feelings = Some(feelings);
        }
        if !draft.hunting_births.is_empty() {
            self.hunting
                .as_mut()
                .expect("new hunting preferences have an owner")
                .policies
                .par_extend(draft.hunting_births);
        }
        if !draft.animal_care_births.is_empty() {
            self.animal_care
                .as_mut()
                .expect("new animal care preferences have an owner")
                .policies
                .par_extend(draft.animal_care_births);
        }
        if !draft.animal_handling_births.is_empty() {
            self.animal_handling
                .as_mut()
                .expect("new animal handling preferences have an owner")
                .policies
                .par_extend(draft.animal_handling_births);
        }
        self.food_search = draft.food_search;
        let old_giving = std::mem::replace(&mut self.giving, draft.giving);
        self.hauling = draft.hauling;
        self.local_transport = draft.local_transport;
        self.epoch = epoch;
        if parallel_reclaim {
            rayon::join(
                || {
                    rayon::join(
                        || drop(old_family),
                        || game_parallel_economy_knowledge::reclaim(old_knowledge),
                    );
                },
                || {
                    rayon::join(
                        || {
                            drop(old_dynamics);
                            game_parallel_economy_dynamics::reclaim_opinions(old_opinions);
                        },
                        || giving_practices::reclaim(old_giving),
                    );
                },
            );
        } else {
            drop((
                old_family,
                old_knowledge,
                old_dynamics,
                old_opinions,
                old_giving,
            ));
        }
        #[cfg(feature = "profile")]
        drop(profile_phase);
        Ok(events)
    }
    pub fn advance(&mut self) -> Result<Events, String> {
        game_parallel_economy_campaign::preflight!(self);
        let draft = self.prepare()?;
        self.commit(draft)
    }
}
#[cfg(test)]
#[path = "../tests/support/behavior.rs"]
mod tests;
