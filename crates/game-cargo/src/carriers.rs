use super::*;
pub(super) fn index(
    inventories: &Records<InventoryId, Inventory>,
) -> Table<PersonId, Table<InventoryId, ()>> {
    let mut index = Table::<PersonId, Table<InventoryId, ()>>::default();
    for (id, inventory) in inventories {
        if let Custody::Carried(person) = inventory.custody {
            index.entry(person).or_default().insert(*id, ());
        }
    }
    index
}
impl<'de> Deserialize<'de> for Cargo {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let inventories = Records::deserialize(deserializer)?;
        let carriers = index(&inventories);
        Ok(Self(inventories, carriers))
    }
}
impl Cargo {
    pub fn carried(&self, person: PersonId) -> impl Iterator<Item = InventoryId> + '_ {
        self.1
            .get(&person)
            .into_iter()
            .flat_map(|ids| ids.keys().copied())
    }
}
