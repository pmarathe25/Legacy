//! Renderer-independent commands and committed observations. Time is in milliseconds;
//! quantities use 1,000 fixed-point units per whole good.
use serde::{Deserialize, Serialize};
pub mod agreements;
pub use agreements::*;
pub mod property;
pub use property::*;
pub mod institutions;
pub use institutions::*;
pub mod skills;
pub use skills::*;
pub mod projects;
pub use projects::*;
pub mod invocation;
pub use invocation::*;
pub mod force;
pub use force::*;
pub mod missions;
pub use missions::*;
pub mod ecology;
pub use ecology::*;

macro_rules! id {
    ($($name:ident),*) => {$(
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(pub u64);
    )*};
}
id!(
    ResourceId,
    MissionId,
    RecruitmentId,
    PersonId,
    AreaId,
    InventoryId,
    SiteId,
    ConceptId,
    AgreementId,
    WarningId,
    RecordId,
    ProjectId,
    EnterpriseId,
    InstitutionId,
    RuleProposalId,
    CaseId,
    ClaimId,
    IncidentId,
    AmendmentId,
    ObligationId,
    PregnancyId,
    EncounterId,
    CombatMapId,
    EventId,
    SimTime,
    SimDuration,
    Quantity
);

pub const MINUTE: SimDuration = SimDuration(60_000);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Ecology {
        action: EcologyAction,
    },
    Mission {
        action: MissionAction,
    },
    Escape,
    Restrain {
        target: PersonId,
    },
    Yield {
        captor: PersonId,
    },
    WithdrawYield,
    Invocation {
        action: InvocationAction,
    },
    Carry {
        source: InventoryId,
        quantity: Quantity,
    },
    Project {
        action: ProjectAction,
    },
    Institution {
        action: InstitutionAction,
    },
    Learn {
        teacher: PersonId,
        topic: LessonTopic,
    },
    RequestGuardian {
        guardian: PersonId,
        request: GuardianRequest,
    },
    Property {
        action: PropertyAction,
    },
    Agreement {
        action: AgreementAction,
    },
    Cultivate {
        field: SiteId,
        operation: FarmOperation,
    },
    Move {
        destination: AreaId,
    },
    Rest,
    Eat {
        source: InventoryId,
    },
    Load {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Work {
        site: SiteId,
    },
    Cancel,
    Tell {
        recipient: PersonId,
        inventory: InventoryId,
    },
    Conceive {
        partner: PersonId,
    },
    Care {
        child: PersonId,
        source: InventoryId,
    },
    Engage {
        target: PersonId,
    },
    FreeCaptive {
        target: PersonId,
    },
    Comfort {
        child: PersonId,
    },
    Signal {
        preference: ChildhoodInterest,
    },
    Play {
        interest: ChildhoodInterest,
    },
    CombatTurn {
        encounter: EncounterId,
        movement: Option<Point>,
        action: CombatAction,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    pub caller_sequence: u64,
    pub command: Command,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub caller_sequence: u64,
    pub authoritative_sequence: u64,
    pub accepted_at: SimTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Error {
    UnknownArea,
    NoRoute,
    AlreadyThere,
    SequenceConflict,
    InvalidConfiguration(String),
    InvalidFixture(String),
    InvalidSave(String),
    Overflow,
    WorkerFailure,
    HostDisconnected,
    Unavailable,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Ruler,
    Guardian,
    Citizen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Location {
    AtArea(AreaId),
    InTransit {
        from: AreaId,
        to: AreaId,
        departed: SimTime,
        arrives: SimTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AreaView {
    pub id: AreaId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    pub ecology: EcologyView,
    pub missions: Vec<MissionView>,
    pub force: ForceView,
    pub invocation: Option<InvocationView>,
    pub projects: ProjectsView,
    pub skills: Vec<SkillView>,
    pub institutions: Vec<InstitutionView>,
    pub social_cases: Vec<SocialCaseView>,
    pub property: PropertyView,
    pub lineage: Option<LineageView>,
    pub agreements: Vec<AgreementView>,
    pub environment: Option<EnvironmentView>,
    pub revision: u64,
    pub time: SimTime,
    pub ruler: PersonId,
    pub location: Location,
    pub known_areas: Vec<AreaView>,
    pub available_commands: Vec<Command>,
    pub physical: Option<PhysicalView>,
    pub known_stocks: Vec<StockKnowledge>,
    pub nearby_people: Vec<(PersonId, Role)>,
    pub nearby_life_stages: Vec<(PersonId, LifeStage)>,
    pub combat: Option<TacticalView>,
    pub interests: Vec<(ChildhoodInterest, u64)>,
    pub life_stage: Option<LifeStage>,
    pub preference: Option<ChildhoodInterest>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Good {
    Herb,
    Food,
    Timber,
    Tool,
    Binding,
    Seed,
    Water,
    Record,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vitals {
    /// Milliseconds of fixture hunger/fatigue exposure, not setting biology.
    pub hunger: u64,
    pub fatigue: u64,
    pub health: u64,
    pub alive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Activity {
    Gather {
        resource: ResourceId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    MissionTalk {
        completes: SimTime,
    },
    Escape {
        tool: InventoryId,
        completes: SimTime,
    },
    Restrain {
        target: PersonId,
        binding: InventoryId,
        completes: SimTime,
    },
    ReportAgreement {
        recipient: PersonId,
        snapshot: Box<AgreementView>,
        completes: SimTime,
    },
    WritingMemory {
        completes: SimTime,
    },
    ReadingMemory {
        completes: SimTime,
    },
    TeachingMemory {
        completes: SimTime,
    },
    MovingRecord {
        completes: SimTime,
    },
    Construct {
        project: ProjectId,
        completes: SimTime,
    },
    Lesson {
        teacher: PersonId,
        student: PersonId,
        topic: LessonTopic,
        completes: SimTime,
    },
    Take {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    ReportIncident {
        recipient: PersonId,
        incident: IncidentId,
        completes: SimTime,
    },
    Cultivate {
        field: SiteId,
        operation: FarmOperation,
        completes: SimTime,
    },
    Comfort {
        child: PersonId,
        completes: SimTime,
    },
    Idle,
    Rest,
    Eat {
        source: InventoryId,
        completes: SimTime,
    },
    Load {
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
        completes: SimTime,
    },
    Work {
        site: SiteId,
    },
    Tell {
        recipient: PersonId,
        fact: StockKnowledge,
        completes: SimTime,
    },
    Conceive {
        pregnancy: PregnancyId,
        completes: SimTime,
    },
    Care {
        child: PersonId,
        source: InventoryId,
        completes: SimTime,
    },
    Combat {
        encounter: EncounterId,
    },
    Unbind {
        target: PersonId,
        completes: SimTime,
    },
    Express {
        preference: ChildhoodInterest,
        completes: SimTime,
    },
    Play {
        interest: ChildhoodInterest,
        completes: SimTime,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockView {
    pub id: InventoryId,
    pub good: Good,
    pub quantity: Quantity,
    pub can_access: bool,
    pub capacity: Quantity,
    pub position: StockPosition,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StockPosition {
    Area(AreaId),
    Carried(PersonId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhysicalView {
    pub vitals: Vitals,
    pub activity: Activity,
    pub local_stocks: Vec<StockView>,
    pub local_sites: Vec<SiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnowledgeSource {
    Recorded(RecordId),
    Observed(PersonId),
    Reported(PersonId),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockKnowledge {
    pub inventory: InventoryId,
    pub area: AreaId,
    pub good: Good,
    pub quantity: Quantity,
    pub accessible: bool,
    pub observed_at: SimTime,
    pub learned_at: SimTime,
    pub source: KnowledgeSource,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    ObligationAccrued {
        obligation: ObligationId,
        actor: PersonId,
        quantity: Quantity,
    },
    ResourceAccessChanged {
        resource: ResourceId,
        actor: PersonId,
        recipient: PersonId,
        allowed: bool,
    },
    ResourceAccessRequested {
        resource: ResourceId,
        actor: PersonId,
        custodian: PersonId,
    },
    ResourceAccessRefused {
        resource: ResourceId,
        actor: PersonId,
        custodian: PersonId,
    },
    GatheringStarted {
        actor: PersonId,
        resource: ResourceId,
        unauthorized: bool,
    },
    ResourceCollected {
        actor: PersonId,
        resource: ResourceId,
        destination: InventoryId,
        good: Good,
        quantity: Quantity,
    },
    CombatReinforcement {
        encounter: EncounterId,
        actor: PersonId,
        joins_round: u64,
    },
    MissionProposed {
        mission: MissionId,
        sponsor: PersonId,
    },
    MissionRecruitmentChanged {
        mission: MissionId,
        person: PersonId,
        status: CompanionStatus,
    },
    MissionDeparted {
        mission: MissionId,
    },
    MissionObjectiveChanged {
        mission: MissionId,
        leader: PersonId,
    },
    MissionFinished {
        mission: MissionId,
        status: MissionStatus,
    },
    MissionReported {
        mission: MissionId,
        speaker: PersonId,
        recipient: PersonId,
    },
    AgreementReported {
        sender: PersonId,
        recipient: PersonId,
        agreement: AgreementId,
    },
    Yielded {
        actor: PersonId,
        captor: PersonId,
    },
    YieldWithdrawn {
        actor: PersonId,
    },
    EscapeAttempted {
        actor: PersonId,
    },
    EscapeInterrupted {
        actor: PersonId,
    },
    Restrained {
        actor: PersonId,
        target: PersonId,
        binding: InventoryId,
    },
    InvocationStarted {
        actor: PersonId,
        lives: std::collections::BTreeSet<PersonId>,
    },
    InvocationDismissed {
        actor: PersonId,
    },
    InvocationWarningRaised {
        warning: WarningId,
        actor: PersonId,
    },
    InvocationCostAccepted {
        warning: WarningId,
        actor: PersonId,
    },
    InvocationCostApplied {
        actor: PersonId,
        cost: InvocationCost,
        severity: u8,
        permanent: bool,
    },
    InvocationControlChanged {
        actor: PersonId,
        controlled: bool,
        until: Option<SimTime>,
    },
    MemoryRecorded {
        record: RecordId,
        actor: PersonId,
        inventory: InventoryId,
    },
    MemoryRead {
        record: RecordId,
        actor: PersonId,
    },
    MemoryTaught {
        actor: PersonId,
        recipient: PersonId,
        life: PersonId,
    },
    ProjectProposed {
        project: ProjectId,
        sponsor: PersonId,
    },
    ProjectSupported {
        project: ProjectId,
        actor: PersonId,
        agreement: AgreementId,
    },
    ConstructionStarted {
        project: ProjectId,
        actor: PersonId,
    },
    ProjectCompleted {
        project: ProjectId,
        enterprise: EnterpriseId,
        site: SiteId,
    },
    ProjectCancelled {
        project: ProjectId,
        actor: PersonId,
    },
    ProjectFailed {
        project: ProjectId,
    },
    EnterpriseInputUsed {
        enterprise: EnterpriseId,
        site: SiteId,
        quantity: Quantity,
    },
    InstitutionProposed {
        institution: InstitutionId,
        proposer: PersonId,
    },
    InstitutionFounded {
        institution: InstitutionId,
    },
    InstitutionChanged {
        institution: InstitutionId,
        proposal: RuleProposalId,
    },
    InstitutionLeft {
        institution: InstitutionId,
        actor: PersonId,
    },
    OfficeSupportChanged {
        institution: InstitutionId,
        office: u16,
        actor: PersonId,
        support: bool,
    },
    InstitutionRuleApplied {
        institution: InstitutionId,
        rule: u16,
        actor: PersonId,
        agreement: Option<AgreementId>,
    },
    SocialCaseRecorded {
        case: CaseId,
        actor: PersonId,
    },
    LessonCompleted {
        teacher: PersonId,
        student: PersonId,
        topic: LessonTopic,
        gain: u64,
    },
    GuardianRefused {
        guardian: PersonId,
        requester: PersonId,
        reason: String,
    },
    GuardianAccepted {
        guardian: PersonId,
        requester: PersonId,
    },
    PropertyClaimed {
        claim: ClaimId,
        actor: PersonId,
        inventory: InventoryId,
    },
    ClaimRecognized {
        claim: ClaimId,
        actor: PersonId,
        support: bool,
    },
    PermissionShared {
        actor: PersonId,
        inventory: InventoryId,
        recipient: PersonId,
        allowed: bool,
    },
    TakingAttempted {
        incident: IncidentId,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    Taken {
        incident: IncidentId,
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    TakingInterrupted {
        incident: IncidentId,
        actor: PersonId,
    },
    IncidentReported {
        sender: PersonId,
        recipient: PersonId,
        incident: IncidentId,
    },
    AccessRequested {
        actor: PersonId,
        custodian: PersonId,
        inventory: InventoryId,
    },
    AccessRefused {
        actor: PersonId,
        custodian: PersonId,
        inventory: InventoryId,
    },
    AgreementProposed {
        agreement: AgreementId,
        proposer: PersonId,
    },
    AgreementActivated {
        agreement: AgreementId,
    },
    AgreementRejected {
        agreement: AgreementId,
        actor: PersonId,
    },
    AgreementAmended {
        agreement: AgreementId,
        amendment: AmendmentId,
    },
    ObligationPerformed {
        obligation: ObligationId,
        actor: PersonId,
        quantity: Quantity,
    },
    ObligationDefaulted {
        obligation: ObligationId,
    },
    Cultivated {
        actor: PersonId,
        field: SiteId,
        operation: FarmOperation,
        labor_ms: u64,
    },
    CropHarvested {
        actor: PersonId,
        field: SiteId,
        food: Quantity,
        seed: Quantity,
    },
    CropFailed {
        field: SiteId,
    },
    Comforted {
        caregiver: PersonId,
        child: PersonId,
    },
    Expressed {
        actor: PersonId,
        preference: ChildhoodInterest,
    },
    Practiced {
        actor: PersonId,
        interest: ChildhoodInterest,
        gain: u64,
    },
    EncounterStarted {
        encounter: EncounterId,
        area: AreaId,
    },
    CombatMoved {
        encounter: EncounterId,
        actor: PersonId,
        position: Point,
    },
    Struck {
        encounter: EncounterId,
        actor: PersonId,
        target: PersonId,
        damage: u64,
    },
    Surrendered {
        encounter: EncounterId,
        actor: PersonId,
    },
    Captured {
        encounter: EncounterId,
        actor: PersonId,
        target: PersonId,
        binding: InventoryId,
    },
    Released {
        actor: PersonId,
        target: PersonId,
    },
    ObstacleDamaged {
        map: CombatMapId,
        obstacle: u32,
        damage: u64,
    },
    RoundEnded {
        encounter: EncounterId,
        round: u64,
    },
    EncounterEnded {
        encounter: EncounterId,
    },
    Departed {
        actor: PersonId,
        from: AreaId,
        to: AreaId,
        arrives: SimTime,
    },
    Arrived {
        actor: PersonId,
        area: AreaId,
    },
    Transferred {
        actor: PersonId,
        source: InventoryId,
        destination: InventoryId,
        quantity: Quantity,
    },
    TransferFailed {
        actor: PersonId,
        source: InventoryId,
    },
    Consumed {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Ate {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Spoiled {
        inventory: InventoryId,
        quantity: Quantity,
    },
    Produced {
        actor: PersonId,
        site: SiteId,
        output: InventoryId,
        quantity: Quantity,
    },
    Died {
        actor: PersonId,
    },
    Interrupted {
        actor: PersonId,
    },
    Recovered {
        actor: PersonId,
    },
    Reported {
        sender: PersonId,
        recipient: PersonId,
        inventory: InventoryId,
    },
    ConceptionRefused {
        actor: PersonId,
        partner: PersonId,
    },
    Conceived {
        pregnancy: PregnancyId,
        parents: [PersonId; 2],
    },
    PregnancyEnded {
        pregnancy: PregnancyId,
    },
    Born {
        pregnancy: PregnancyId,
        child: PersonId,
        parents: [PersonId; 2],
    },
    Cared {
        caregiver: PersonId,
        child: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    Developed {
        actor: PersonId,
        stage: LifeStage,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifeStage {
    Infant,
    Child,
    Adult,
    Elder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ChildhoodInterest {
    Food,
    Company,
    Movement,
    Craft,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub time: SimTime,
    pub kind: EventKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopReason {
    InvocationWarning { warning: WarningId },
    SuccessorBorn { ruler: PersonId },
    TargetReached,
    Paused,
    CombatInput { encounter: EncounterId },
}

/// Continuous placement in millimetres, independent of any rendering grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombatAction {
    MissionObjective {
        mission: MissionId,
        objective: MissionObjective,
    },
    Attack {
        target: PersonId,
    },
    Defend,
    Wait,
    Surrender,
    Capture {
        target: PersonId,
    },
    Release {
        target: PersonId,
    },
    Retreat {
        destination: AreaId,
    },
    Breach {
        obstacle: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObstacleView {
    pub index: u32,
    pub minimum: Point,
    pub maximum: Point,
    pub integrity: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FighterView {
    pub actor: PersonId,
    pub position: Point,
    pub team: u32,
    pub incapacitated: bool,
    pub surrendered: bool,
    pub captured: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TacticalView {
    pub encounter: EncounterId,
    pub active: bool,
    pub map: CombatMapId,
    pub area: AreaId,
    pub round: u64,
    pub round_started: SimTime,
    pub round_ends: SimTime,
    pub turn: Option<PersonId>,
    pub ready_to_advance: bool,
    pub width: i64,
    pub height: i64,
    pub obstacles: Vec<ObstacleView>,
    pub exits: Vec<(AreaId, Point)>,
    pub fighters: Vec<FighterView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceResult {
    pub requested: SimTime,
    pub reached: SimTime,
    pub revision: u64,
    pub reason: StopReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenarioInfo {
    pub id: String,
    pub version: u32,
    pub debug_fixture: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FarmOperation {
    Fallow,
    Prepare,
    Plant,
    Tend,
    Irrigate,
    Harvest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentView {
    pub day: u64,
    pub day_of_year: u64,
    pub season: u8,
    pub temperature_c: i32,
    pub rain: u64,
    pub fields: Vec<FieldView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldView {
    pub id: SiteId,
    pub phase: String,
    pub soil_fertility: u16,
    pub soil_water: u64,
    pub degree_days_milli: u64,
    pub preparation_ms: u64,
    pub planting_ms: u64,
    pub harvest_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineageView {
    pub invocation_learning: u64,
    pub invocation_ready: bool,
    pub current: Option<PersonId>,
    pub incarnations: Vec<PersonId>,
    pub waiting_since: Option<SimTime>,
    pub guardians: Vec<GuardianSearchView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardianSearchView {
    pub ready: bool,
    pub assessment: String,
    pub explanation: String,
    pub guardian: PersonId,
    pub location: Location,
    pub goal: GuardianGoalView,
    pub target: Option<PersonId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianGoalView {
    Waiting,
    Searching,
    Mentoring,
    Confined,
    Recovering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LessonTopic {
    Invocation,
    Craft,
    Travel,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuardianRequest {
    SupplyFood {
        source: InventoryId,
        quantity: Quantity,
    },
    Move {
        destination: AreaId,
    },
    Rest,
    Work {
        site: SiteId,
    },
}
