//! Concrete workshop plans, chosen support, and locally known enterprise records.
use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectAction {
    Propose {
        area: AreaId,
        materials: InventoryId,
        tool: InventoryId,
        input: InventoryId,
        output: InventoryId,
        beneficiaries: BTreeSet<PersonId>,
        workers: BTreeSet<PersonId>,
        finish_by: SimTime,
    },
    Support {
        project: ProjectId,
        agreement: AgreementId,
    },
    Work {
        project: ProjectId,
    },
    Cancel {
        project: ProjectId,
    },
    Operate {
        enterprise: EnterpriseId,
        agreement: AgreementId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectStatus {
    SeekingSupport,
    Ready,
    Active,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectBlocker {
    Space,
    Materials,
    Tool,
    Access,
    Workers,
    OutputCapacity,
    Deadline,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnterpriseStatus {
    Ready,
    Active,
    Blocked,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    pub id: ProjectId,
    pub sponsor: PersonId,
    pub area: AreaId,
    pub materials: InventoryId,
    pub tool: InventoryId,
    pub input: InventoryId,
    pub output: InventoryId,
    pub beneficiaries: BTreeSet<PersonId>,
    pub workers: BTreeSet<PersonId>,
    pub support: BTreeSet<AgreementId>,
    pub status: ProjectStatus,
    pub blocker: Option<ProjectBlocker>,
    pub progress_ms: u64,
    pub materials_consumed: bool,
    pub finish_by: SimTime,
    pub enterprise: Option<EnterpriseId>,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnterpriseAccountView {
    pub timber_consumed: u64,
    pub tools_produced: u64,
    pub labor: BTreeMap<PersonId, u64>,
    pub operating_agreements: BTreeSet<AgreementId>,
    pub promised_labor_ms: u128,
    pub performed_labor_ms: u128,
    pub promised_payments: BTreeMap<Good, u128>,
    pub performed_payments: BTreeMap<Good, u128>,
    pub defaults: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnterpriseView {
    pub id: EnterpriseId,
    pub project: ProjectId,
    pub sponsor: PersonId,
    pub area: AreaId,
    pub site: SiteId,
    pub beneficiaries: BTreeSet<PersonId>,
    pub workers: BTreeSet<PersonId>,
    pub status: EnterpriseStatus,
    pub blocker: Option<ProjectBlocker>,
    pub accounts: Option<EnterpriseAccountView>,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectsView {
    pub projects: Vec<ProjectView>,
    pub enterprises: Vec<EnterpriseView>,
}
