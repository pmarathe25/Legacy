//! Natural deposits, personally observed opportunities and physical collection.
use crate::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceKind {
    Coppice,
    Berries,
    Herbs,
    Spring,
}
impl ResourceKind {
    pub fn good(self) -> Good {
        match self {
            Self::Coppice => Good::Timber,
            Self::Berries => Good::Food,
            Self::Herbs => Good::Herb,
            Self::Spring => Good::Water,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EcologyAction {
    Gather {
        resource: ResourceId,
        quantity: Quantity,
        unauthorized: bool,
    },
    RequestAccess {
        resource: ResourceId,
    },
    GrantAccess {
        resource: ResourceId,
        recipient: PersonId,
        allowed: bool,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceView {
    pub id: ResourceId,
    pub area: AreaId,
    pub kind: ResourceKind,
    pub available: Quantity,
    pub accessible: bool,
    pub custodian: Option<PersonId>,
    pub contamination: u16,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlotView {
    pub id: SiteId,
    pub area: AreaId,
    pub soil_fertility: u16,
    pub soil_water: u64,
    pub accessible: bool,
    pub custodian: Option<PersonId>,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EcologyView {
    pub resources: Vec<ResourceView>,
    pub plots: Vec<PlotView>,
}
