//! Authored starting places and assets. No live-world mutation or game systems.
use game_content::{Document, Stamp};
use game_core::*;
use game_ecology::ResourceKind;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 128
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c))
}
fn invalid(reason: &str) -> Error {
    Error::InvalidConfiguration(reason.into())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaDefinition {
    pub key: String,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkDefinition {
    pub from: String,
    pub to: String,
    pub duration: SimDuration,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Placement {
    Area { area: String },
    Resident { index: usize },
    EachResident,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Access {
    Residents,
    Holder,
    Selected { residents: Vec<usize> },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Custodian {
    Holder,
    Resident { index: usize },
    Unclaimed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockDefinition {
    pub key: String,
    pub good: Good,
    pub placement: Placement,
    pub access: Access,
    pub quantity: Quantity,
    pub capacity: Quantity,
    #[serde(default)]
    pub quantity_per_resident: Quantity,
    #[serde(default)]
    pub capacity_per_resident: Quantity,
    pub custodian: Custodian,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceDefinition {
    pub key: String,
    pub area: String,
    pub kind: ResourceKind,
    pub capacity: Quantity,
    pub daily_growth: Quantity,
    pub contamination: u16,
    pub labor_per_unit: SimDuration,
    pub access: Access,
    pub custodian: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StockAmounts {
    pub quantity: Quantity,
    pub capacity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkshopDefinition {
    pub key: String,
    pub area: String,
    pub recipe: String,
    pub materials: String,
    pub input: String,
    pub tool: String,
    pub output: String,
    pub access: Access,
    pub sponsor: usize,
    pub beneficiaries: Vec<usize>,
    pub workers: Vec<usize>,
    pub craftsperson: usize,
    pub historical_batches: u64,
    pub historical_materials: Option<StockAmounts>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettlementDefinition {
    pub attitude: game_property::Attitude,
    pub minimum_residents: usize,
    pub centre: String,
    pub fields: String,
    pub wood: String,
    pub areas: Vec<AreaDefinition>,
    pub links: Vec<LinkDefinition>,
    pub stocks: Vec<StockDefinition>,
    pub resources: Vec<ResourceDefinition>,
    pub workshops: Vec<WorkshopDefinition>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Opening {
    pub label: String,
    pub settlement: String,
    pub initial_residents: usize,
    pub established_workshop: bool,
    pub starting_customs: bool,
    pub child_successor: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub random_order: Vec<String>,
    pub openings: BTreeMap<String, Opening>,
    pub settlements: BTreeMap<String, SettlementDefinition>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Definitions {
    pub catalog: Catalog,
    pub content: Stamp,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedStock {
    pub definition: StockDefinition,
    pub quantity: Quantity,
    pub capacity: Quantity,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthoredPlan {
    pub residents: usize,
    pub definition: SettlementDefinition,
    pub stocks: Vec<ResolvedStock>,
}
fn check_access(
    access: &Access,
    residents: usize,
    placement: Option<&Placement>,
) -> Result<(), Error> {
    match access {
        Access::Residents => Ok(()),
        Access::Holder
            if matches!(
                placement,
                Some(Placement::Resident { .. } | Placement::EachResident)
            ) =>
        {
            Ok(())
        }
        Access::Selected { residents: indices }
            if !indices.is_empty()
                && indices.iter().all(|index| *index < residents)
                && indices.iter().collect::<BTreeSet<_>>().len() == indices.len() =>
        {
            Ok(())
        }
        _ => Err(invalid("invalid opening access selector")),
    }
}
impl Access {
    pub fn resolve(
        &self,
        residents: &[PersonId],
        holder: Option<PersonId>,
    ) -> Result<BTreeSet<PersonId>, Error> {
        match self {
            Self::Residents => Ok(residents.iter().copied().collect()),
            Self::Holder => holder
                .map(|id| BTreeSet::from([id]))
                .ok_or(Error::Unavailable),
            Self::Selected { residents: indices } => indices
                .iter()
                .map(|index| residents.get(*index).copied().ok_or(Error::Unavailable))
                .collect(),
        }
    }
}
impl Custodian {
    pub fn resolve(
        &self,
        residents: &[PersonId],
        holder: Option<PersonId>,
    ) -> Result<Option<PersonId>, Error> {
        match self {
            Self::Holder => holder.map(Some).ok_or(Error::Unavailable),
            Self::Resident { index } => residents
                .get(*index)
                .copied()
                .map(Some)
                .ok_or(Error::Unavailable),
            Self::Unclaimed => Ok(None),
        }
    }
}
impl SettlementDefinition {
    pub fn validate(&self) -> Result<(), Error> {
        self.attitude.validate()?;
        if !(15..=128).contains(&self.minimum_residents) {
            return Err(invalid("invalid settlement resident minimum"));
        }
        let keys: BTreeSet<_> = self.areas.iter().map(|area| area.key.as_str()).collect();
        if keys.len() != self.areas.len()
            || self.areas.iter().any(|area| {
                !valid_key(&area.key) || area.name.trim().is_empty() || area.name.len() > 256
            })
            || [
                self.centre.as_str(),
                self.fields.as_str(),
                self.wood.as_str(),
            ]
            .into_iter()
            .collect::<BTreeSet<_>>()
            .len()
                != 3
            || [&self.centre, &self.fields, &self.wood]
                .into_iter()
                .any(|key| !keys.contains(key.as_str()))
        {
            return Err(invalid("invalid authored settlement areas"));
        }
        for link in &self.links {
            if link.from == link.to
                || !keys.contains(link.from.as_str())
                || !keys.contains(link.to.as_str())
                || link.duration.0 == 0
            {
                return Err(invalid("invalid authored settlement link"));
            }
        }
        let mut stock_keys = BTreeSet::new();
        for stock in &self.stocks {
            if !valid_key(&stock.key)
                || !stock_keys.insert(stock.key.as_str())
                || match stock.custodian {
                    Custodian::Resident { index } => index >= self.minimum_residents,
                    Custodian::Holder => matches!(stock.placement, Placement::Area { .. }),
                    Custodian::Unclaimed => false,
                }
                || match &stock.placement {
                    Placement::Area { area } => !keys.contains(area.as_str()),
                    Placement::Resident { index } => *index >= self.minimum_residents,
                    Placement::EachResident => false,
                }
            {
                return Err(invalid("invalid authored stock reference"));
            }
            check_access(
                &stock.access,
                self.minimum_residents,
                Some(&stock.placement),
            )?;
            for residents in [self.minimum_residents, 128] {
                let q = stock.quantity.0.max(
                    stock
                        .quantity_per_resident
                        .0
                        .checked_mul(residents as u64)
                        .ok_or(Error::Overflow)?,
                );
                let cap = stock.capacity.0.max(
                    stock
                        .capacity_per_resident
                        .0
                        .checked_mul(residents as u64)
                        .ok_or(Error::Overflow)?,
                );
                if q > cap {
                    return Err(invalid("authored stock exceeds capacity"));
                }
            }
        }
        let mut resource_keys = BTreeSet::new();
        for resource in &self.resources {
            if !valid_key(&resource.key)
                || !resource_keys.insert(&resource.key)
                || !keys.contains(resource.area.as_str())
                || resource.capacity.0 == 0
                || resource.contamination > 1_000
                || resource.labor_per_unit.0 == 0
                || resource.custodian >= self.minimum_residents
            {
                return Err(invalid("invalid authored resource"));
            }
            check_access(&resource.access, self.minimum_residents, None)?;
        }
        let mut workshop_keys = BTreeSet::new();
        for workshop in &self.workshops {
            if !valid_key(&workshop.key)
                || !workshop_keys.insert(&workshop.key)
                || !keys.contains(workshop.area.as_str())
                || !valid_key(&workshop.recipe)
                || [
                    &workshop.materials,
                    &workshop.input,
                    &workshop.tool,
                    &workshop.output,
                ]
                .into_iter()
                .any(|key| !stock_keys.contains(key.as_str()))
                || workshop.beneficiaries.is_empty()
                || workshop.workers.is_empty()
                || workshop.beneficiaries.len() > 8
                || workshop.workers.len() > 8
                || !workshop.beneficiaries.contains(&workshop.sponsor)
                || !workshop.workers.contains(&workshop.sponsor)
                || workshop
                    .beneficiaries
                    .iter()
                    .chain(&workshop.workers)
                    .any(|actor| *actor >= self.minimum_residents)
                || workshop.beneficiaries.iter().collect::<BTreeSet<_>>().len()
                    != workshop.beneficiaries.len()
                || workshop.workers.iter().collect::<BTreeSet<_>>().len() != workshop.workers.len()
                || workshop.sponsor >= self.minimum_residents
                || workshop.craftsperson >= self.minimum_residents
                || workshop
                    .historical_materials
                    .as_ref()
                    .is_some_and(|stock| stock.quantity > stock.capacity)
            {
                return Err(invalid("invalid authored workshop reference"));
            }
            check_access(&workshop.access, self.minimum_residents, None)?;
        }
        Ok(())
    }
    pub fn plan(&self, residents: usize) -> Result<AuthoredPlan, Error> {
        self.validate()?;
        if !(self.minimum_residents..=128).contains(&residents) {
            return Err(invalid("invalid authored settlement population"));
        }
        let stocks = self
            .stocks
            .iter()
            .map(|definition| {
                let quantity = Quantity(
                    definition.quantity.0.max(
                        definition
                            .quantity_per_resident
                            .0
                            .checked_mul(residents as u64)
                            .ok_or(Error::Overflow)?,
                    ),
                );
                let capacity = Quantity(
                    definition.capacity.0.max(
                        definition
                            .capacity_per_resident
                            .0
                            .checked_mul(residents as u64)
                            .ok_or(Error::Overflow)?,
                    ),
                );
                if quantity > capacity {
                    return Err(invalid("authored stock exceeds capacity"));
                }
                Ok(ResolvedStock {
                    definition: definition.clone(),
                    quantity,
                    capacity,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(AuthoredPlan {
            residents,
            definition: self.clone(),
            stocks,
        })
    }
}
impl Catalog {
    pub fn validate(&self) -> Result<(), Error> {
        if self.random_order.is_empty()
            || self.random_order.iter().collect::<BTreeSet<_>>().len() != self.random_order.len()
            || self
                .random_order
                .iter()
                .any(|key| !self.openings.contains_key(key))
        {
            return Err(invalid("invalid random opening list"));
        }
        for (key, settlement) in &self.settlements {
            if !valid_key(key) {
                return Err(invalid("invalid settlement key"));
            }
            settlement.validate()?;
        }
        for (key, opening) in &self.openings {
            let settlement = self
                .settlements
                .get(&opening.settlement)
                .ok_or_else(|| invalid("unknown opening settlement"))?;
            if !valid_key(key)
                || opening.label.trim().is_empty()
                || opening.label.len() > 256
                || !(settlement.minimum_residents..=128).contains(&opening.initial_residents)
                || opening.established_workshop
                    && (settlement.workshops.is_empty()
                        || settlement
                            .workshops
                            .iter()
                            .any(|workshop| workshop.historical_materials.is_none()))
            {
                return Err(invalid("invalid authored opening"));
            }
        }
        Ok(())
    }
    pub fn choose(&self, seed: u64) -> Result<&str, Error> {
        self.validate()?;
        let mixed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        Ok(&self.random_order[((mixed >> 32) % self.random_order.len() as u64) as usize])
    }
}
impl Definitions {
    pub fn from_content(document: Document<Catalog>) -> Result<Self, Error> {
        document.validate_header()?;
        document.data.validate()?;
        Ok(Self {
            content: document.stamp()?,
            catalog: document.data,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        self.catalog.validate()?;
        if (Document {
            id: self.content.id.clone(),
            version: self.content.version,
            data: self.catalog.clone(),
        })
        .stamp()?
            != self.content
        {
            return Err(Error::InvalidSave("changed opening definitions".into()));
        }
        Ok(())
    }
    pub fn opening_plan(&self, id: &str) -> Result<(&Opening, AuthoredPlan), Error> {
        self.validate()?;
        let opening = self
            .catalog
            .openings
            .get(id)
            .ok_or_else(|| invalid("unknown opening"))?;
        Ok((
            opening,
            self.catalog.settlements[&opening.settlement].plan(opening.initial_residents)?,
        ))
    }
}
#[cfg(test)]
mod tests;
/// Scale authored existing residents at startup. Later population changes use
/// the simulation's ordinary births and deaths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopulationPlan {
    pub people: usize,
    pub settlement_size: usize,
}
impl PopulationPlan {
    pub fn small() -> Self {
        Self {
            people: 15,
            settlement_size: 15,
        }
    }
    pub fn validate(self) -> Result<(), Error> {
        if !(15..=100_000).contains(&self.people) || !(15..=128).contains(&self.settlement_size) {
            return Err(Error::InvalidConfiguration(
                "population requires 15..100000 people and 15..128 residents per settlement".into(),
            ));
        }
        Ok(())
    }
}
