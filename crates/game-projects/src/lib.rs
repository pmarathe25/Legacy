//! In-kind contributions and construction consume real goods and worker time.
use game_core::*;
use game_sim::*;
use game_storage::Records;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Proposed,
    Building { worker: PersonId, job: ScheduleId },
    Operating,
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub sponsor: PersonId,
    pub area: AreaId,
    pub store: InventoryId,
    pub materials: BTreeMap<GoodId, u64>,
    pub duration: SimDuration,
    pub contributions: BTreeMap<PersonId, u64>,
    pub status: Status,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projects(pub Records<ProjectId, Project>);
impl Resource for Projects {
    fn allocated_ids(&self) -> Vec<u64> {
        self.0.keys().map(|id| id.0).collect()
    }
    const KEY: &'static str = "projects.sites";
    fn validate(&self) -> Result<()> {
        self.validate_rows(self.0.values())
    }
    fn validate_changes(&self, previous: &Self) -> Result<()> {
        self.validate_rows(self.0.changed(&previous.0).map(|(_, row)| row))
    }
}
#[derive(Serialize, Deserialize)]
pub struct Propose(pub Project);
impl Request for Propose {
    const KEY: &'static str = "projects.propose";
    type Response = ProjectId;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Contribute {
    pub person: PersonId,
    pub project: ProjectId,
    pub from: InventoryId,
    pub good: GoodId,
    pub quantity: u64,
}
impl Request for Contribute {
    const KEY: &'static str = "projects.contribute";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Build {
    pub person: PersonId,
    pub project: ProjectId,
}
impl Request for Build {
    const KEY: &'static str = "projects.build";
    type Response = SimTime;
}
#[derive(Serialize, Deserialize)]
struct Finish {
    project: ProjectId,
}
impl Request for Finish {
    const KEY: &'static str = "projects.finish";
    type Response = ();
}
#[derive(Serialize, Deserialize)]
struct Cancel {
    project: ProjectId,
}
impl Request for Cancel {
    const KEY: &'static str = "projects.cancel";
    type Response = ();
}
pub fn plugin(b: &mut Builder) -> Result<()> {
    b.plugin("projects", 1)?;
    b.resource::<Projects>()?;
    b.request::<Propose>(|tx, r| {
        require(
            r.0.status == Status::Proposed && r.0.contributions.is_empty(),
            "invalid new project",
        )?;
        require(
            game_cargo::locality(tx, r.0.store)? == r.0.area,
            "project store is elsewhere",
        )?;
        let id = ProjectId(tx.allocate()?);
        tx.edit::<Projects>()?.0.insert(id, r.0);
        Ok(id)
    })?;
    b.request::<Contribute>(|tx, r| {
        let p = tx
            .get::<Projects>()?
            .0
            .get(&r.project)
            .ok_or_else(|| Error::Missing("project".into()))?
            .clone();
        require(p.status == Status::Proposed, "project not accepting inputs")?;
        tx.call(game_property::Take {
            actor: r.person,
            transfer: game_cargo::Transfer {
                from: r.from,
                to: p.store,
                good: r.good,
                quantity: r.quantity,
            },
            unauthorized: false,
        })?;
        let n = tx
            .edit::<Projects>()?
            .0
            .get_mut(&r.project)
            .unwrap()
            .contributions
            .entry(r.person)
            .or_default();
        *n = n.checked_add(r.quantity).ok_or(Error::Overflow)?;
        Ok(())
    })?;
    b.request::<Build>(|tx, r| {
        game_physiology::capable(tx, r.person)?;
        let p = tx
            .get::<Projects>()?
            .0
            .get(&r.project)
            .ok_or_else(|| Error::Missing("project".into()))?
            .clone();
        require(p.status == Status::Proposed, "cannot start project")?;
        require(
            game_transport::area(tx.get()?, r.person)? == p.area,
            "builder is elsewhere",
        )?;
        require(
            tx.get::<game_property::Property>()?
                .allowed(r.person, p.store),
            "no project supply access",
        )?;
        for (good, quantity) in p.materials {
            tx.call(game_cargo::Consume {
                inventory: p.store,
                good,
                quantity,
            })?;
        }
        let until = tx.now().after(p.duration)?;
        tx.start_action(
            r.person,
            "build",
            Some(until),
            Cancel { project: r.project },
        )?;
        let job = tx.schedule(until, 10, Finish { project: r.project })?;
        tx.edit::<Projects>()?.0.get_mut(&r.project).unwrap().status = Status::Building {
            worker: r.person,
            job,
        };
        Ok(until)
    })?;
    b.request::<Finish>(|tx, r| {
        let p = tx.get::<Projects>()?.0[&r.project].clone();
        let Status::Building { worker, job } = p.status else {
            return Err(Error::Rejected("project not building".into()));
        };
        tx.finish_action(worker, "build")?;
        tx.edit::<Projects>()?.0.get_mut(&r.project).unwrap().status = Status::Operating;
        tx.call(game_skills::Practice {
            person: worker,
            skill: "construction".into(),
            effort: p.duration.0 / MINUTE.0 + 1,
            attribution: job.0,
        })?;
        Ok(())
    })?;
    b.request::<Cancel>(|tx, r| {
        if let Status::Building { job, .. } = tx.get::<Projects>()?.0[&r.project].status {
            tx.unschedule(job);
            tx.edit::<Projects>()?.0.get_mut(&r.project).unwrap().status = Status::Failed;
        }
        Ok(())
    })
}

impl Projects {
    fn validate_rows<'a>(&'a self, mut rows: impl Iterator<Item = &'a Project>) -> Result<()> {
        require(
            rows.all(|p| {
                p.duration.0 > 0 && !p.materials.is_empty() && p.materials.values().all(|q| *q > 0)
            }),
            "invalid project",
        )
    }
}
