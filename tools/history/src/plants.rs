use super::*;
/// Keep actual observed plant identities, rather than every ungerminated fruit family.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Plants(BTreeMap<u64, BTreeMap<u64, u64>>);
impl Plants {
    pub(crate) fn founding(h: &game_sim::Host) -> Result<Self> {
        Ok(Self(
            h.get::<game_botany::Botany>()?
                .stands
                .iter()
                .map(|(patch, s)| {
                    (
                        *patch,
                        s.plants.iter().map(|p| (p.id, p.strain.id)).collect(),
                    )
                })
                .collect(),
        ))
    }
    pub(crate) fn enrich(&mut self, h: &game_sim::Host, data: &mut Value) -> Result<()> {
        let patch = data["patch"].as_u64().ok_or("plant event lacks patch")?;
        let s = h
            .get::<game_botany::Botany>()?
            .stands
            .get(&patch)
            .ok_or("missing plant stand")?;
        let current: BTreeMap<_, _> = s.plants.iter().map(|p| (p.id, p.strain.id)).collect();
        let previous = self.0.entry(patch).or_default();
        data["observed_plants"]=json!(s.plants.iter().filter(|p|!previous.contains_key(&p.id)).map(|p|json!({"plant":p.id,"strain":p.strain,"born":p.born,"age_offset":p.age_offset})).collect::<Vec<_>>());
        data["retired_plants"] = json!(
            previous
                .iter()
                .filter(|(id, _)| !current.contains_key(id))
                .map(|(id, strain)| json!({"plant":id,"strain":strain}))
                .collect::<Vec<_>>()
        );
        *previous = current;
        Ok(())
    }
}
