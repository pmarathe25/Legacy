use super::*;
#[derive(Deserialize)]
pub(super) struct SavedEcology {
    patches: Table<u64, Patch>,
    gathering: Table<PersonId, Gathering>,
    renewing: bool,
    #[serde(default)]
    renewal: Table<u64, Renewal>,
}
impl TryFrom<SavedEcology> for Ecology {
    type Error = Error;
    fn try_from(saved: SavedEcology) -> Result<Self> {
        let (by_area, reserved) = indexes(&saved.patches, &saved.gathering)?;
        Ok(Self {
            patches: saved.patches,
            gathering: saved.gathering,
            renewing: saved.renewing,
            renewal: saved.renewal,
            by_area,
            reserved,
        })
    }
}
type AreaIndex = Records<AreaId, Table<u64, ()>>;
fn indexes(
    patches: &Table<u64, Patch>,
    gathering: &Table<PersonId, Gathering>,
) -> Result<(AreaIndex, Table<u64, u64>)> {
    let mut by_area = AreaIndex::new();
    let mut reserved = Table::<u64, u64>::new();
    for (id, patch) in patches {
        by_area.entry(patch.area).or_default().insert(*id, ());
    }
    for work in gathering.values() {
        require(
            patches.contains_key(&work.patch) && work.quantity > 0,
            "invalid patch reservation",
        )?;
        let total = reserved.entry(work.patch).or_default();
        *total = total.checked_add(work.quantity).ok_or(Error::Overflow)?;
        require(
            *total <= patches[&work.patch].stock,
            "patch reservations exceed stock",
        )?;
    }
    Ok((by_area, reserved))
}
impl Ecology {
    pub fn in_area(&self, area: AreaId) -> impl Iterator<Item = u64> + '_ {
        self.by_area
            .get(&area)
            .into_iter()
            .flat_map(|ids| ids.keys().copied())
    }
    pub(super) fn release(&mut self, patch: u64, quantity: u64) {
        let remaining = self.reserved.get_mut(&patch).expect("reserved patch");
        *remaining -= quantity;
        if *remaining == 0 {
            self.reserved.remove(&patch);
        }
    }
    pub(super) fn validate_indexes(&self) -> Result<()> {
        require(
            self.renewal.keys().all(|id| self.patches.contains_key(id)),
            "renewal without patch",
        )?;
        let (areas, reservations) = indexes(&self.patches, &self.gathering)?;
        require(
            areas == self.by_area && reservations == self.reserved,
            "invalid ecology index",
        )
    }
}
