use game_core::*;
use game_workshop::RecipeId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectAction {
    ProposeFarm {
        plot: SiteId,
        materials: InventoryId,
        tool: InventoryId,
        seed: InventoryId,
        water: InventoryId,
        food: InventoryId,
        beneficiaries: BTreeSet<PersonId>,
        workers: BTreeSet<PersonId>,
        finish_by: SimTime,
    },
    Propose {
        recipe: RecipeId,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    Workshop { recipe: RecipeId },
    Farm { plot: SiteId, water: InventoryId },
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
    pub kind: ProjectKind,
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
    #[serde(default)]
    pub inputs: BTreeMap<Good, u64>,
    #[serde(default)]
    pub produced: BTreeMap<Good, u64>,
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
    pub kind: ProjectKind,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rules {
    pub construction_timber: Quantity,
    pub construction_labor: SimDuration,
    pub workshops_per_area: usize,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            construction_timber: Quantity(5_000),
            construction_labor: SimDuration(300_000),
            workshops_per_area: 3,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub kind: ProjectKind,
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
    pub contributions: BTreeMap<PersonId, u64>,
    pub materials_consumed: bool,
    pub finish_by: SimTime,
    pub enterprise: Option<EnterpriseId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Construction {
    pub project: ProjectId,
    pub completes: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enterprise {
    #[serde(default)]
    pub inputs: BTreeMap<Good, u64>,
    pub kind: ProjectKind,
    #[serde(default)]
    pub produced: BTreeMap<Good, u64>,
    pub id: EnterpriseId,
    pub project: ProjectId,
    pub sponsor: PersonId,
    pub area: AreaId,
    pub site: SiteId,
    pub beneficiaries: BTreeSet<PersonId>,
    pub workers: BTreeSet<PersonId>,
    pub status: EnterpriseStatus,
    pub blocker: Option<ProjectBlocker>,
    pub labor: BTreeMap<PersonId, u64>,
    pub operating_agreements: BTreeSet<AgreementId>,
}

impl Project {
    pub fn observed_view(&self, at: SimTime) -> ProjectView {
        ProjectView {
            kind: self.kind.clone(),
            id: self.id,
            sponsor: self.sponsor,
            area: self.area,
            materials: self.materials,
            tool: self.tool,
            input: self.input,
            output: self.output,
            beneficiaries: self.beneficiaries.clone(),
            workers: self.workers.clone(),
            support: self.support.clone(),
            status: self.status,
            blocker: self.blocker,
            progress_ms: self.progress_ms,
            materials_consumed: self.materials_consumed,
            finish_by: self.finish_by,
            enterprise: self.enterprise,
            observed_at: at,
        }
    }
}
impl Enterprise {
    pub fn observed_view(
        &self,
        at: SimTime,
        accounts: Option<EnterpriseAccountView>,
    ) -> EnterpriseView {
        EnterpriseView {
            kind: self.kind.clone(),
            id: self.id,
            project: self.project,
            sponsor: self.sponsor,
            area: self.area,
            site: self.site,
            beneficiaries: self.beneficiaries.clone(),
            workers: self.workers.clone(),
            status: self.status,
            blocker: self.blocker,
            accounts,
            observed_at: at,
        }
    }
}
