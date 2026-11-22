//! Person-driven economic activity over authoritative dated physical goods.
pub use game_agreements::OfferPolicy as PromiseOfferPolicy;
pub use game_agreements::WorkOfferPolicy as WorkPromiseOfferPolicy;
pub use game_autowork::requests::OfferValue as WorkOfferValue;
pub use game_economy_building::demolition::{Event as DemolitionEvent, Policy as DemolitionPolicy};
pub use game_economy_building::maintenance::{
    Condition as SiteCondition, Event as MaintenanceEvent, Maintenance,
    Policy as MaintenancePolicy, Rule as MaintenanceRule,
};
pub use game_economy_building::salvage::{Event as SalvageEvent, Policy as SalvagePolicy};
pub use game_economy_building::{
    Blueprint as BuildingBlueprint, Construction, Event as ConstructionEvent, GoalBuilder,
    Policy as ConstructionPolicy,
};
pub use game_economy_building::StorageDesign;
pub use game_economy_campaign::{Campaign, ReleaseDecision};
pub use game_economy_combat::Combat;
pub use game_economy_confinement::{
    CapturePolicy as ConfinementCapturePolicy, CaptureResponse as ConfinementCaptureResponse,
    Cell as ConfinementCell, CellKind as ConfinementCellKind, Confinement, EscapePreference,
    GuardPolicy as ConfinementGuardPolicy, Hold as ConfinementHold,
    ReleasePolicy as ConfinementReleasePolicy,
};
pub use game_economy_dynamics::Dynamics;
pub use game_economy_expeditions::{
    DeliveryAcquisition as MissionDeliveryAcquisition,
    DeliveryPreference as MissionDeliveryPreference, MissionPhase, Missions,
    Objective as MissionObjective, Start as MissionStart,
};
pub use game_economy_family::Families;
pub use game_economy_fatigue::Fatigue;
pub use game_economy_goals::{Dream, Dreams, Goal, Goals, Learning};
pub use game_economy_knowledge::{
    Knowledge, Misreport, PersonReport, RecallTrace, Reporting, Retention, SeenPerson, StockSurvey,
};
pub use game_economy_local_transport::goal::{
    Preference as GoalFetchPreference, SearchPreference as GoalSearchPreference,
};
pub use game_economy_local_transport::preservation::{
    Destination as StorageDestination, OwnCellar as OwnCellarPreference,
    Preference as StoragePreference, VisitPreference as StorageVisitPreference,
};
pub use game_economy_local_transport::walking::{
    Event as LocalWalkEvent, Location as LocalLocation, Outcome as LocalWalkOutcome,
    Position as LocalPosition, Preference as LocalWalkPreference, Route as LocalWalkRoute,
    Walking as LocalWalking,
};
pub use game_economy_local_transport::{
    Endpoint as LocalEndpoint, Event as LocalTransportEvent, LocalTransport,
    Policy as LocalTransportPolicy, Store as LocalStore, StoreStructure,
};
pub use game_economy_person::travel::{
    Closure as RouteClosure, Colocation, Departure, Journey, Route, RouteObservation, RouteSharing,
    Travel,
};
pub use game_economy_promises::{Decision as PromiseDecision, Proposal as PromiseProposal};
pub use game_economy_skills::Skills;
pub use game_economy_taking::{
    AssessmentPreference as TakingAssessmentPreference, Complaint as TakingComplaint,
    ComplaintDecision as TakingComplaintDecision, ComplaintPreference as TakingComplaintPreference,
    Event as TakingEvent, Intent as TakingIntent, Interpretation as TakingInterpretation,
    Outcome as TakingOutcome, PersonalCase as TakingPersonalCase, Policy as TakingPolicy,
    Reaction as TakingReaction, Reception as TakingReception, Report as TakingReport,
    ReportBelief as TakingReportBelief, ReportDetails as TakingReportDetails,
    ReportPolicy as TakingReportPolicy, ReportSource as TakingReportSource,
    Response as TakingResponse, Stance as TakingReportStance, Taking, Witness as TakingWitness,
};
pub use game_economy_work::care::DependentCare;
pub use game_economy_work::shared::{
    CrewRule as SharedCrewRule, Event as SharedWorkEvent, Preference as SharedWorkPreference,
    SharedWork,
};
pub use game_economy_work::sites::{
    Binding as WorkSiteBinding, Facilities as WorkSites, Requirement as WorkSiteRequirement,
    Site as WorkSite,
};
pub use game_economy_work::succession::{
    Preference as WorkSuccessionPreference, Succession as WorkSuccession,
};
pub use game_economy_work::tool_wear::{
    Condition as ToolCondition, RepairChoice as ToolRepairChoice, RepairEvent as ToolRepairEvent,
    Rule as ToolWearRule, ToolWear,
};
pub use game_economy_work::tools::Requirement as WorkTool;
pub use game_fauna::{Animal, Fauna, Pregnancy as AnimalPregnancy, Species as AnimalSpecies};
pub use game_opinions::{
    ConfinementExperienceKind, ConfinementReaction, ObligationDevelopment, Opinion,
    RelationshipCase, ReportReception, Sensitivity as OpinionSensitivity, SubjectiveReaction,
    TeachingReaction, TeachingRole,
};
pub use game_practices::work::Evaluation as WorkPracticeEvaluation;
pub use game_practices::work::outcome::Policy as WorkOutcomePolicy;
pub use game_practices::{
    Belief as PracticeBelief, Case as PracticeCase, Decision as PracticeDecision,
    Mind as PracticeMind, Practices as PersonalPractices, PrecedentReasoning as PracticePrecedent,
    ReportAttitude as PracticeReportAttitude, Reporting as PracticeReporting,
    ValueLearning as PracticeValueLearning,
};
mod animal_care;
mod animal_feelings;
mod animal_handling;
mod animal_products;
mod animal_restraint_inheritance;
pub use animal_care::{AnimalCare, Config as AnimalCareConfig, Policy as AnimalCarePolicy};
pub use animal_feelings::{
    AnimalFeelings, Mind as AnimalFeelingMind, Opinion as AnimalFeelingOpinion,
};
pub use animal_handling::{Handling as AnimalHandling, Policy as AnimalHandlingPolicy};
pub use animal_products::{
    AnimalProducts, Config as AnimalProductsConfig, Event as AnimalProductEvent,
    Outcome as AnimalProductOutcome, Policy as AnimalProductPolicy,
    Progress as AnimalProductProgress, Rule as AnimalProductRule,
};
pub use game_animal_restraint::{
    DefensePolicy as AnimalDefensePolicy, Event as AnimalRestraintEvent,
    EventKind as AnimalRestraintEventKind, Hold as AnimalHold, Policy as AnimalCapturePolicy,
    ReleasePolicy as AnimalReleasePolicy, Restraints as AnimalRestraints,
};
pub use game_fauna::handling::{
    Event as AnimalHandlingEvent, Outcome as AnimalHandlingOutcome,
    Response as AnimalHandlingResponse,
};
pub use game_fauna::restraint::Response as AnimalRestraintResponse;
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
mod local_walking;
mod medical;
mod mission_deliveries;
mod route_sharing;
pub use food_search::{
    DestinationValue as FoodDestinationValue, FoodSearch, Preference as FoodSearchPreference,
    TravelChoice as FoodTravelChoice,
};
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
pub mod core_interval;
mod purpose;
mod recovery;
pub use game_purpose::Purpose;
pub use gathering::{
    Gathering, Harvested as GatheredResource, Node as ResourceNode, Policy as GatherPolicy,
    RenewalRate as ResourceRenewalRate,
};
pub use recovery::{ElapsedRecovery, Rules as RecoveryRules};
mod state;
mod writing;
mod writing_memory;
pub use game_economy_agriculture::irrigation::{
    Choice as WaterChoice, Event as WaterEvent, Irrigation, Moisture as FieldMoisture,
};
pub use game_economy_agriculture::requests as cultivation_requests;
pub use game_economy_agriculture::{
    Agriculture, Crop, Field, Phase as CropPhase, Policy as FarmPolicy,
};
pub use game_environment::{Clothing, ClothingChoice, Environment, Hazard, Season};
pub use game_nutrition::Nutrition;
pub use state::{Agreements, Immortal, Person, Rules, State};
pub use writing::{
    Author as WritingAuthor, Literacy, Page as WrittenPage, Practice as WritingPractice,
    Study as WritingStudy, TextAuthor as WritingTextAuthor, TextTemplate as WritingTextTemplate,
    Writing,
};
pub use writing::{
    Composition as WritingComposition, Surface as WritingSurface,
    SurfaceChoice as WritingSurfaceChoice, Work as WrittenWork,
};
pub use writing_memory::{TextMemory, TextRetention};
