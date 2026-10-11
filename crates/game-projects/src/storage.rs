use crate::*;
use game_ecs::{Schema, StableId, Store};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    pub projects: BTreeMap<ProjectId, ProjectView>,
    pub enterprises: BTreeMap<EnterpriseId, EnterpriseView>,
    pub timber_hints: BTreeSet<AreaId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requests {
    pub next_search: SimTime,
}
fn invalid() -> Error {
    Error::InvalidConfiguration("invalid project component".into())
}
fn kind_valid(kind: &ProjectKind) -> bool {
    match kind {
        ProjectKind::Workshop { recipe } => recipe.validate().is_ok(),
        ProjectKind::Farm { plot, water } => plot.0 > 0 && water.0 > 0,
    }
}
impl Project {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.sponsor.0 == 0
            || self.area.0 == 0
            || !kind_valid(&self.kind)
            || [self.materials, self.tool, self.input, self.output]
                .iter()
                .any(|id| id.0 == 0)
            || self.beneficiaries.is_empty()
            || self.beneficiaries.len() > 8
            || self.workers.is_empty()
            || self.workers.len() > 8
            || !self.beneficiaries.contains(&self.sponsor)
            || !self.workers.contains(&self.sponsor)
            || self
                .beneficiaries
                .iter()
                .chain(&self.workers)
                .any(|id| id.0 == 0)
            || self.support.iter().any(|id| id.0 == 0)
            || self.contributions.keys().any(|id| id.0 == 0)
            || self
                .contributions
                .values()
                .map(|q| u128::from(*q))
                .sum::<u128>()
                != u128::from(self.progress_ms)
            || !self.materials_consumed && self.progress_ms != 0
            || self.tool == self.output
            || (self.status == ProjectStatus::Completed) != self.enterprise.is_some()
            || self.enterprise.is_some_and(|id| id.0 == 0)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Enterprise {
    pub fn validate(&self) -> Result<(), Error> {
        if self.id.0 == 0
            || self.project.0 == 0
            || self.sponsor.0 == 0
            || self.area.0 == 0
            || self.site.0 == 0
            || !kind_valid(&self.kind)
            || self.beneficiaries.is_empty()
            || self.beneficiaries.len() > 8
            || self.workers.is_empty()
            || self.workers.len() > 8
            || !self.beneficiaries.contains(&self.sponsor)
            || !self.workers.contains(&self.sponsor)
            || self
                .beneficiaries
                .iter()
                .chain(&self.workers)
                .chain(self.labor.keys())
                .any(|id| id.0 == 0)
            || self.operating_agreements.iter().any(|id| id.0 == 0)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Construction {
    pub fn validate(&self) -> Result<(), Error> {
        if self.project.0 == 0 || self.completes.0 == 0 {
            return Err(invalid());
        }
        Ok(())
    }
}
impl Knowledge {
    pub fn validate(&self) -> Result<(), Error> {
        if self
            .projects
            .iter()
            .any(|(id, row)| id.0 == 0 || *id != row.id || !kind_valid(&row.kind))
            || self
                .enterprises
                .iter()
                .any(|(id, row)| id.0 == 0 || *id != row.id || !kind_valid(&row.kind))
            || self.timber_hints.iter().any(|id| id.0 == 0)
        {
            return Err(invalid());
        }
        Ok(())
    }
}
pub fn register_components(schema: &mut Schema) -> Result<(), Error> {
    schema.register::<Project>("projects/project", Project::validate)?;
    schema.register::<Enterprise>("projects/enterprise", Enterprise::validate)?;
    schema.register::<Construction>("projects/construction", Construction::validate)?;
    schema.register::<Knowledge>("projects/knowledge", Knowledge::validate)?;
    schema.register::<Requests>("projects/requests", |_| Ok(()))
}
macro_rules! ports {
    ($row:ty,$key:ident,$get:ident,$mut:ident,$iter:ident,$insert:ident,$remove:ident) => {
        pub fn $get(store: &Store, id: $key) -> Option<&$row> {
            store.get(StableId(id.0))
        }
        pub fn $mut(store: &mut Store, id: $key) -> Option<&mut $row> {
            store.get_mut(StableId(id.0))
        }
        pub fn $iter(store: &Store) -> impl Iterator<Item = ($key, &$row)> {
            store.iter::<$row>().map(|(id, row)| ($key(id.0), row))
        }
        pub fn $insert(store: &mut Store, id: $key, row: $row) -> Result<(), Error> {
            store.insert_or_spawn(StableId(id.0), row)
        }
        pub fn $remove(store: &mut Store, id: $key) -> Result<Option<$row>, Error> {
            store.remove(StableId(id.0))
        }
    };
}
ports!(
    Construction,
    PersonId,
    construction,
    construction_mut,
    constructions,
    insert_construction,
    remove_construction
);
ports!(
    Knowledge,
    PersonId,
    knowledge,
    knowledge_mut,
    knowledges,
    insert_knowledge,
    remove_knowledge
);
ports!(
    Requests,
    PersonId,
    requests,
    requests_mut,
    request_rows,
    insert_requests,
    remove_requests
);
pub fn project(store: &Store, id: ProjectId) -> Option<&Project> {
    store.get(StableId(id.0))
}
pub fn project_mut(store: &mut Store, id: ProjectId) -> Option<&mut Project> {
    store.get_mut(StableId(id.0))
}
pub fn projects(store: &Store) -> impl Iterator<Item = (ProjectId, &Project)> {
    store
        .iter::<Project>()
        .map(|(id, row)| (ProjectId(id.0), row))
}
pub fn insert_project(store: &mut Store, id: ProjectId, row: Project) -> Result<(), Error> {
    if id != row.id {
        return Err(invalid());
    }
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn remove_project(store: &mut Store, id: ProjectId) -> Result<Option<Project>, Error> {
    store.remove(StableId(id.0))
}
pub fn enterprise(store: &Store, id: EnterpriseId) -> Option<&Enterprise> {
    store.get(StableId(id.0))
}
pub fn enterprise_mut(store: &mut Store, id: EnterpriseId) -> Option<&mut Enterprise> {
    store.get_mut(StableId(id.0))
}
pub fn enterprises(store: &Store) -> impl Iterator<Item = (EnterpriseId, &Enterprise)> {
    store
        .iter::<Enterprise>()
        .map(|(id, row)| (EnterpriseId(id.0), row))
}
pub fn insert_enterprise(
    store: &mut Store,
    id: EnterpriseId,
    row: Enterprise,
) -> Result<(), Error> {
    if id != row.id {
        return Err(invalid());
    }
    store.insert_or_spawn(StableId(id.0), row)
}
pub fn remove_enterprise(store: &mut Store, id: EnterpriseId) -> Result<Option<Enterprise>, Error> {
    store.remove(StableId(id.0))
}
