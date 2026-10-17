use super::*;
/// A physical action can defy a claim. Standing permission affects social consequences.
#[derive(Clone, Serialize, Deserialize)]
pub struct Use {
    pub actor: PersonId,
    pub inventory: InventoryId,
    pub purpose: String,
}
impl Request for Use {
    const KEY: &'static str = "property.use";
    type Response = ();
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Interfered {
    pub actor: PersonId,
    pub inventory: InventoryId,
    pub purpose: String,
    pub witnesses: Vec<PersonId>,
}
impl Event for Interfered {
    const KEY: &'static str = "property.interfered";
}
/// The perception owner supplies actual local observers, rather than treating presence as awareness.
#[derive(Clone, Serialize, Deserialize)]
pub struct Observers {
    pub area: AreaId,
}
impl Request for Observers {
    const KEY: &'static str = "property.observers";
    type Response = Vec<PersonId>;
}
pub(super) fn register(b: &mut Builder) -> Result<()> {
    b.request::<Use>(|tx, r| {
        require(!r.purpose.is_empty(), "use needs a purpose")?;
        let area = game_cargo::locality(tx, r.inventory)?;
        require(
            game_transport::area(tx.get()?, r.actor)? == area,
            "goods are elsewhere",
        )?;
        if tx.get::<Property>()?.allowed(r.actor, r.inventory)
            || tx
                .get::<Property>()?
                .claims
                .get(&r.inventory)
                .is_none_or(|c| c.is_empty())
        {
            return Ok(());
        }
        let witnesses = if tx.registered::<Observers>() {
            tx.call(Observers { area })?
        } else {
            vec![r.actor]
        };
        tx.emit(
            Interfered {
                actor: r.actor,
                inventory: r.inventory,
                purpose: r.purpose,
                witnesses: witnesses.clone(),
            },
            witnesses,
        )
    })
}

/// One actor's assessments for a read-only query. Rebuild after effects change their evidence.
pub struct ClaimCosts<'a> {
    person: PersonId,
    property: Option<&'a Property>,
    permitted: Option<&'a game_storage::Table<InventoryId, ()>>,
    relationships: Option<&'a game_storage::Table<PersonId, game_relationships::Relationship>>,
    honesty: i64,
    learned: i64,
    legitimacy: i64,
}
impl<'a> ClaimCosts<'a> {
    pub fn new(tx: &'a Transaction<'_>, person: PersonId) -> Self {
        let property = tx.get::<Property>().ok();
        let social = tx.get::<game_relationships::Social>().ok();
        let beliefs = social.and_then(|s| s.beliefs.get(&person));
        Self {
            person,
            property,
            permitted: property.and_then(|p| p.permitted_set(person)),
            relationships: social.and_then(|s| s.relationships.get(&person)),
            honesty: game_vitality::disposition(tx, person, "honesty"),
            learned: social
                .and_then(|s| s.values.get(&person)?.get("respect-claims"))
                .copied()
                .map_or(0, i64::from),
            legitimacy: beliefs.map_or(0, |b| i64::from(b.authority - b.sharing)),
        }
    }
    pub fn cost(&self, inventory: InventoryId) -> i64 {
        let Some(property) = self.property else {
            return 0;
        };
        if self.permitted.is_some_and(|p| p.contains_key(&inventory)) {
            return 0;
        }
        let Some(claims) = property.claims.get(&inventory).filter(|c| !c.is_empty()) else {
            return 0;
        };
        let recognized = claims
            .iter()
            .any(|c| c.recognized_by.contains(&self.person));
        let fear = claims
            .iter()
            .filter_map(|c| self.relationships?.get(&c.claimant))
            .map(|r| i64::from(r.fear) * 2 - i64::from(r.grievance) / 2)
            .max()
            .unwrap_or(0);
        (self.honesty / if recognized { 10 } else { 30 } + self.learned + fear + self.legitimacy)
            .max(0)
    }
}
/// Conscience and expected consequences affect preference, not physical permission.
pub fn reluctance(tx: &Transaction<'_>, person: PersonId, inventory: InventoryId) -> i64 {
    ClaimCosts::new(tx, person).cost(inventory)
}
