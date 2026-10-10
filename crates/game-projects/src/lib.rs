//! Concrete workshop plans, chosen support, and locally known enterprise records.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
mod transitions;
pub use transitions::*;
#[cfg(test)]
mod transition_tests;

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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectKind {
    #[default]
    Workshop,
    Farm {
        plot: SiteId,
        water: InventoryId,
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
    #[serde(default)]
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
    #[serde(default)]
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
    pub recipe_timber: Quantity,
    pub recipe_tools: Quantity,
    pub recipe_time: SimDuration,
    pub workshops_per_area: usize,
}
impl Default for Rules {
    fn default() -> Self {
        Self {
            construction_timber: Quantity(5_000),
            construction_labor: SimDuration(300_000),
            recipe_timber: Quantity(2_000),
            recipe_tools: Quantity(1_000),
            recipe_time: MINUTE,
            workshops_per_area: 3,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
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
    #[serde(default)]
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
    pub timber_consumed: u64,
    pub tools_produced: u64,
    pub labor: BTreeMap<PersonId, u64>,
    pub operating_agreements: BTreeSet<AgreementId>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub rules: Rules,
    pub projects: BTreeMap<ProjectId, Project>,
    pub enterprises: BTreeMap<EnterpriseId, Enterprise>,
    pub jobs: BTreeMap<PersonId, Construction>,
    pub known_projects: BTreeMap<PersonId, BTreeMap<ProjectId, ProjectView>>,
    pub known_enterprises: BTreeMap<PersonId, BTreeMap<EnterpriseId, EnterpriseView>>,
    #[serde(default)]
    pub timber_hints: BTreeMap<PersonId, BTreeSet<AreaId>>,
    pub next_search: BTreeMap<PersonId, SimTime>,
    pub last_event: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnterpriseWork {
    pub actor: PersonId,
    pub site: SiteId,
}
impl World {
    /// The caller supplies actual eligible enterprise work. Construction jobs
    /// are owned here; neither path reads a physical actor or creates an asset.
    pub fn advance(
        &self,
        from: SimTime,
        next: SimTime,
        work: &[EnterpriseWork],
    ) -> Result<Self, Error> {
        let elapsed = next.0.checked_sub(from.0).ok_or(Error::Overflow)?;
        let mut actors = BTreeSet::new();
        let mut building = BTreeSet::new();
        if work
            .iter()
            .any(|w| !actors.insert(w.actor) || self.jobs.contains_key(&w.actor))
            || self.jobs.values().any(|j| !building.insert(j.project))
        {
            return Err(Error::Unavailable);
        }
        let mut world = self.clone();
        for (actor, job) in &world.jobs {
            let project = world
                .projects
                .get_mut(&job.project)
                .ok_or(Error::Unavailable)?;
            project.progress_ms = project
                .progress_ms
                .checked_add(elapsed)
                .ok_or(Error::Overflow)?;
            let credit = project.contributions.entry(*actor).or_default();
            *credit = credit.checked_add(elapsed).ok_or(Error::Overflow)?;
        }
        for input in work {
            if let Some(enterprise) = world
                .enterprises
                .values_mut()
                .find(|e| e.site == input.site)
            {
                let credit = enterprise.labor.entry(input.actor).or_default();
                *credit = credit.checked_add(elapsed).ok_or(Error::Overflow)?;
            }
        }
        Ok(world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eligible_enterprise_labor_is_explicit_and_cannot_credit_one_body_twice() {
        let enterprise = Enterprise {
            inputs: BTreeMap::new(),
            kind: ProjectKind::Workshop,
            produced: BTreeMap::new(),
            id: EnterpriseId(1),
            project: ProjectId(2),
            sponsor: PersonId(3),
            area: AreaId(4),
            site: SiteId(5),
            beneficiaries: BTreeSet::from([PersonId(3)]),
            workers: BTreeSet::from([PersonId(3)]),
            status: EnterpriseStatus::Ready,
            blocker: None,
            timber_consumed: 0,
            tools_produced: 0,
            labor: BTreeMap::new(),
            operating_agreements: BTreeSet::new(),
        };
        let world = World {
            rules: Rules::default(),
            projects: BTreeMap::new(),
            enterprises: BTreeMap::from([(enterprise.id, enterprise)]),
            jobs: BTreeMap::new(),
            known_projects: BTreeMap::new(),
            known_enterprises: BTreeMap::new(),
            timber_hints: BTreeMap::new(),
            next_search: BTreeMap::new(),
            last_event: 0,
        };
        let work = EnterpriseWork {
            actor: PersonId(3),
            site: SiteId(5),
        };
        let one = world.advance(SimTime(0), SimTime(60_000), &[work]).unwrap();
        assert_eq!(
            one.enterprises[&EnterpriseId(1)].labor[&PersonId(3)],
            60_000
        );
        assert_eq!(
            world.advance(SimTime(0), SimTime(60_000), &[work, work]),
            Err(Error::Unavailable)
        );
        let half = world.advance(SimTime(0), SimTime(17_001), &[work]).unwrap();
        assert_eq!(
            one,
            half.advance(SimTime(17_001), SimTime(60_000), &[work])
                .unwrap()
        );
    }
}
