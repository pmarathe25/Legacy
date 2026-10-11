use crate::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Runtime {
    pub rules: Rules,
    pub last_review: SimTime,
}
pub type Rules = HealthRules;
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dose {
    pub amount: u64,
    pub remainder: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Infection {
    pub kind: IllnessKind,
    pub acquired: SimTime,
    pub onset: SimTime,
    pub recovers: SimTime,
    pub symptomatic: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Course {
    pub until: SimTime,
    pub healing: u16,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clinical {
    pub thirst: u64,
    pub cold: u64,
    pub cold_remainder: u16,
    pub doses: BTreeMap<IllnessKind, Dose>,
    pub illnesses: BTreeMap<IllnessKind, Infection>,
    pub treatment: Option<Course>,
    pub last_drink: Option<(InventoryId, SimTime)>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shelter {
    pub id: ShelterId,
    pub area: AreaId,
    pub custodian: PersonId,
    pub access: BTreeSet<PersonId>,
    pub capacity: u32,
    pub insulation: u16,
    pub built_at: SimTime,
    pub timber_used: Quantity,
    pub labor_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub herbs: InventoryId,
    pub water: InventoryId,
    pub expires: SimTime,
    pub accepted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Work {
    Drink {
        source: InventoryId,
        quantity: Quantity,
    },
    Boil {
        source: InventoryId,
        quantity: Quantity,
        tool: InventoryId,
    },
    Shelter {
        timber_used: Quantity,
        tool: InventoryId,
        capacity: u32,
        insulation: u16,
        started: SimTime,
    },
    Treat {
        patient: PersonId,
        skill_ms: u128,
        elapsed: u64,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub actor: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub work: Work,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Knowledge {
    /// Actual local searches, never inferred from another person's report.
    pub people: BTreeMap<PersonId, SymptomsView>,
    pub shelters: BTreeMap<ShelterId, ShelterView>,
    pub water: BTreeMap<InventoryId, WaterSafetyView>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occupancy {
    pub shelter: ShelterId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offers {
    pub caregiver: PersonId,
    pub patients: BTreeMap<PersonId, Offer>,
}
