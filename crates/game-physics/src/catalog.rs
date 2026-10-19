use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    materials: BTreeMap<String, SpecMaterial>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecMaterial {
    span: u32,
    limit: u64,
    efficiency: u16,
    #[serde(default)]
    supports: BTreeMap<String, u64>,
}
pub fn load(store: &mut game_content::Store, objects: &game_objects::Objects) -> Result<Config> {
    let mut materials = BTreeMap::new();
    for path in store.scan("mechanics")? {
        let document: game_content::Document<Spec> = store.load(&path)?;
        for (key, m) in document.data.materials {
            let good = objects.resolve(&key)?;
            let supports = m
                .supports
                .into_iter()
                .map(|(key, n)| Ok((objects.resolve(&key)?, n)))
                .collect::<Result<_>>()?;
            require(
                materials
                    .insert(
                        good,
                        Material {
                            span: m.span,
                            limit: m.limit,
                            efficiency: m.efficiency,
                            supports,
                        },
                    )
                    .is_none(),
                "duplicate mechanical material",
            )?;
        }
    }
    Ok(Config { materials })
}
pub fn validate(config: &Config, parts: &[Part]) -> Result<()> {
    require(
        !parts.is_empty() && parts.len() <= 32,
        "invalid apparatus size",
    )?;
    for (i, p) in parts.iter().enumerate() {
        let m = config
            .materials
            .get(&p.good)
            .ok_or_else(|| Error::Missing("mechanical material".into()))?;
        require(
            p.drive > 0
                && p.load > 0
                && p.drive.saturating_add(p.load) <= m.span
                && p.input.is_none_or(|input| input < i),
            "invalid part dimensions or connection",
        )?;
    }
    Ok(())
}
pub fn needs(config: &Config, parts: &[Part]) -> Result<BTreeMap<GoodId, u64>> {
    validate(config, parts)?;
    let mut needs = BTreeMap::<GoodId, u64>::new();
    for part in parts {
        for (good, n) in std::iter::once((part.good, 1)).chain(
            config.materials[&part.good]
                .supports
                .iter()
                .map(|(g, n)| (*g, *n)),
        ) {
            let value = needs.entry(good).or_default();
            *value = value.checked_add(n).ok_or(Error::Overflow)?;
        }
    }
    Ok(needs)
}
