use crate::*;
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub water: BTreeMap<InventoryId, WaterLot>,
    pub rules: Rules,
    pub clinical: BTreeMap<PersonId, Clinical>,
    pub shelters: BTreeMap<ShelterId, Shelter>,
    pub occupancy: BTreeMap<PersonId, ShelterId>,
    #[serde(with = "offer_rows")]
    pub offers: BTreeMap<(PersonId, PersonId), Offer>,
    pub jobs: BTreeMap<PersonId, Job>,
    pub known: BTreeMap<PersonId, Knowledge>,
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
pub struct Job {
    pub actor: PersonId,
    pub area: AreaId,
    pub completes: SimTime,
    pub work: Work,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Knowledge {
    /// Actual local searches, never inferred from another person's report.
    pub people: BTreeMap<PersonId, SymptomsView>,
    pub shelters: BTreeMap<ShelterId, ShelterView>,
    pub water: BTreeMap<InventoryId, WaterSafetyView>,
}

mod offer_rows {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        offers: &BTreeMap<(PersonId, PersonId), Offer>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        offers.values().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<(PersonId, PersonId), Offer>, D::Error> {
        let rows = Vec::<Offer>::deserialize(deserializer)?;
        let mut result = BTreeMap::new();
        for offer in rows {
            if result
                .insert((offer.caregiver, offer.patient), offer)
                .is_some()
            {
                return Err(serde::de::Error::custom("duplicate treatment offer"));
            }
        }
        Ok(result)
    }
}
