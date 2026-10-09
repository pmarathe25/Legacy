//! Renderer-independent commands and committed observations. Time is in milliseconds;
//! quantities use 1,000 fixed-point units per whole good.
pub use game_core::*;
use serde::{Deserialize, Serialize};
pub mod agreements;
pub use agreements::*;
pub mod property;
pub use property::*;
pub mod institutions;
pub use institutions::*;
pub mod projects;
pub mod skills;
pub use projects::*;
pub mod invocation;
pub use invocation::*;
pub mod force;
pub use force::*;
pub mod missions;
pub use missions::*;
pub mod ecology;
pub use ecology::*;
pub mod health;
pub use health::*;
pub mod social;
pub use social::*;
pub mod enforcement;
pub use enforcement::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Social {
        action: SocialAction,
    },
    Enforcement {
        action: EnforcementAction,
    },
    Health {
        action: HealthAction,
    },
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
pub struct AreaView {
    pub id: AreaId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerView {
    pub social: Option<SocialView>,
    pub enforcement: EnforcementView,
    pub health: Option<HealthView>,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Activity {
    Social {
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
    EnforcementTalk {
        case: EnforcementCaseId,
        recipient: PersonId,
        completes: SimTime,
    },
    HealthWork {
        kind: HealthWorkKind,
        completes: SimTime,
    },
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
pub struct PhysicalView {
    pub vitals: Vitals,
    pub activity: Activity,
    pub local_stocks: Vec<StockView>,
    pub local_sites: Vec<SiteId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
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
    SocialRefused {
        actor: PersonId,
        recipient: PersonId,
        reason: String,
    },
    EnforcementPolicyChanged {
        institution: InstitutionId,
        office: u16,
    },
    EnforcementFiling {
        case: EnforcementCaseId,
        actor: PersonId,
        recipient: PersonId,
    },
    EnforcementFiled {
        case: EnforcementCaseId,
        actor: PersonId,
        institution: InstitutionId,
    },
    EnforcementReported {
        case: EnforcementCaseId,
        sender: PersonId,
        recipient: PersonId,
    },
    EnforcementDecided {
        case: EnforcementCaseId,
        judgment: JudgmentId,
        proposer: PersonId,
    },
    EnforcementResponded {
        case: EnforcementCaseId,
        actor: PersonId,
        comply: bool,
    },
    EnforcementDispatched {
        case: EnforcementCaseId,
        actor: PersonId,
        mission: MissionId,
    },
    EnforcementOutcome {
        case: EnforcementCaseId,
        status: EnforcementStatus,
    },
    Drank {
        actor: PersonId,
        source: InventoryId,
        quantity: Quantity,
    },
    WaterPurified {
        actor: PersonId,
        source: InventoryId,
    },
    ShelterBuilt {
        shelter: ShelterId,
        builder: PersonId,
    },
    ShelterAccessChanged {
        shelter: ShelterId,
        actor: PersonId,
        person: PersonId,
        allowed: bool,
    },
    ShelterEntered {
        shelter: ShelterId,
        actor: PersonId,
    },
    ShelterLeft {
        shelter: ShelterId,
        actor: PersonId,
    },
    TreatmentOffered {
        caregiver: PersonId,
        patient: PersonId,
    },
    TreatmentAccepted {
        caregiver: PersonId,
        patient: PersonId,
    },
    TreatmentRefused {
        caregiver: PersonId,
        patient: PersonId,
    },
    TreatmentCompleted {
        caregiver: PersonId,
        patient: PersonId,
    },
    IllnessStarted {
        actor: PersonId,
        kind: IllnessKind,
    },
    IllnessRecovered {
        actor: PersonId,
        kind: IllnessKind,
    },
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

pub use game_conflict::ObstacleView;

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

pub use game_economy::environment::FarmOperation;

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

pub use game_lineage::{
    GuardianGoalView, GuardianRequest, GuardianSearchView, LessonTopic, LineageView,
};
