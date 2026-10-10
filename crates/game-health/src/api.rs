//! Ordinary physiology and care. Permissions describe acknowledged access,
//! independently of any people's chosen property or household institutions.
use game_core::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthRules {
    pub drink_quantity: Quantity,
    pub drink_relief: u64,
    /// How long a personally searched dry place remains an unpromising lead.
    pub drinking_time: SimDuration,
    pub thirst_harm: u64,
    pub thirst_damage: u16,
    pub cold_threshold_c: i32,
    pub cold_accumulation: u16,
    pub warmth_recovery: u16,
    pub cold_harm: u64,
    pub cold_damage: u16,
    pub sheltered_sleep_bonus: u16,
    pub infection_threshold: u64,
    pub contact_dose: u16,
    pub incubation: SimDuration,
    pub illness_duration: SimDuration,
    pub illness_damage: u16,
    pub illness_fatigue: u16,
    pub boiling_time: SimDuration,
    pub boiling_fuel: Quantity,
    pub treatment_time: SimDuration,
    pub treatment_herbs: Quantity,
    pub treatment_water: Quantity,
    pub treatment_duration: SimDuration,
    pub treatment_healing: u16,
    pub offer_duration: SimDuration,
    pub shelter_time_per_place: SimDuration,
    pub shelter_timber_per_place: Quantity,
}
impl HealthRules {
    pub fn human() -> Self {
        let hour = 3_600_000;
        Self {
            drink_quantity: Quantity(1_000),
            drink_relief: 8 * hour,
            drinking_time: SimDuration(30_000),
            thirst_harm: 24 * hour,
            thirst_damage: 2_000,
            cold_threshold_c: 5,
            cold_accumulation: 25,
            warmth_recovery: 1_000,
            cold_harm: hour,
            cold_damage: 2_000,
            sheltered_sleep_bonus: 500,
            infection_threshold: 100_000,
            contact_dose: 1,
            incubation: SimDuration(24 * hour),
            illness_duration: SimDuration(5 * 24 * hour),
            illness_damage: 100,
            illness_fatigue: 100,
            boiling_time: SimDuration(900_000),
            boiling_fuel: Quantity(1_000),
            treatment_time: SimDuration(1_800_000),
            treatment_herbs: Quantity(1_000),
            treatment_water: Quantity(1_000),
            treatment_duration: SimDuration(24 * hour),
            treatment_healing: 1_000,
            offer_duration: SimDuration(hour),
            shelter_time_per_place: SimDuration(2 * hour),
            shelter_timber_per_place: Quantity(1_000),
        }
    }
    pub fn fixture() -> Self {
        Self {
            drink_relief: 300_000,
            drinking_time: SimDuration(10_000),
            thirst_harm: 180_000,
            cold_accumulation: 250,
            warmth_recovery: 2_000,
            cold_harm: 60_000,
            infection_threshold: 30_000,
            contact_dose: 1_000,
            incubation: SimDuration(30_000),
            illness_duration: SimDuration(240_000),
            illness_damage: 2_000,
            illness_fatigue: 500,
            boiling_time: SimDuration(30_000),
            treatment_time: SimDuration(60_000),
            treatment_duration: SimDuration(120_000),
            offer_duration: SimDuration(180_000),
            shelter_time_per_place: SimDuration(30_000),
            ..Self::human()
        }
    }
}
impl Default for HealthRules {
    fn default() -> Self {
        Self::human()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum IllnessKind {
    Enteric,
    Respiratory,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthWorkKind {
    Drink,
    Boil,
    Shelter,
    Treat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthAction {
    Drink {
        source: InventoryId,
    },
    Boil {
        source: InventoryId,
        fuel: InventoryId,
        tool: InventoryId,
    },
    BuildShelter {
        timber: InventoryId,
        tool: InventoryId,
        capacity: u32,
        insulation: u16,
    },
    ShelterAccess {
        shelter: ShelterId,
        person: PersonId,
        allowed: bool,
    },
    EnterShelter {
        shelter: ShelterId,
    },
    LeaveShelter,
    RequestTreatment {
        caregiver: PersonId,
    },
    OfferTreatment {
        patient: PersonId,
        herbs: InventoryId,
        water: InventoryId,
    },
    AcceptTreatment {
        caregiver: PersonId,
    },
    RefuseTreatment {
        caregiver: PersonId,
    },
    Treat {
        patient: PersonId,
        herbs: InventoryId,
        water: InventoryId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymptomsView {
    pub person: PersonId,
    pub fever: bool,
    pub nausea: bool,
    pub cough: bool,
    pub shivering: bool,
    pub wounded: bool,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShelterView {
    pub id: ShelterId,
    pub area: AreaId,
    pub capacity: u32,
    pub insulation: u16,
    pub custodian: PersonId,
    pub accessible: bool,
    pub known_occupants: BTreeSet<PersonId>,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreatmentOfferView {
    pub caregiver: PersonId,
    pub patient: PersonId,
    pub herbs: InventoryId,
    pub water: InventoryId,
    pub expires: SimTime,
    pub accepted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaterSafetyView {
    pub inventory: InventoryId,
    pub purified: bool,
    pub suspected_unsafe: bool,
    pub observed_at: SimTime,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthView {
    pub thirst: u64,
    pub cold_exposure: u64,
    pub symptoms: SymptomsView,
    pub sheltered: Option<ShelterId>,
    pub treatment_until: Option<SimTime>,
    pub offers: Vec<TreatmentOfferView>,
    pub observed_people: Vec<SymptomsView>,
    pub shelters: Vec<ShelterView>,
    pub water: Vec<WaterSafetyView>,
}
