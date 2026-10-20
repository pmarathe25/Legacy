use super::*;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    materials: BTreeMap<String, SpecMaterial>,
    #[serde(default)]
    supports: BTreeMap<Mechanism, BTreeMap<String, u64>>,
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
    let mut supports = BTreeMap::new();
    for path in store.scan("mechanics")? {
        let document: game_content::Document<Spec> = store.load(&path)?;
        for (kind, goods) in document.data.supports {
            let goods = goods
                .into_iter()
                .map(|(key, n)| Ok((objects.resolve(&key)?, n)))
                .collect::<Result<_>>()?;
            require(
                supports.insert(kind, goods).is_none(),
                "duplicate mechanism supports",
            )?;
        }
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
    Ok(Config {
        materials,
        supports,
    })
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
            p.mechanism.ratio(p.drive, p.load, m.span).is_some()
                && p.input.is_none_or(|input| input < i)
                && p.input.map_or(Motion::Linear, |input| {
                    parts
                        .get(input)
                        .map_or(Motion::Linear, |p| p.mechanism.output())
                }) == p.mechanism.input(),
            "invalid part dimensions or connection",
        )?;
    }
    Ok(())
}
pub fn needs(config: &Config, parts: &[Part]) -> Result<BTreeMap<GoodId, u64>> {
    validate(config, parts)?;
    let mut needs = BTreeMap::<GoodId, u64>::new();
    for part in parts {
        for (good, n) in std::iter::once((part.good, 1))
            .chain(
                config.materials[&part.good]
                    .supports
                    .iter()
                    .map(|(g, n)| (*g, *n)),
            )
            .chain(
                config
                    .supports
                    .get(&part.mechanism)
                    .into_iter()
                    .flat_map(|r| r.iter())
                    .map(|(g, n)| (*g, *n)),
            )
        {
            let value = needs.entry(good).or_default();
            *value = value.checked_add(n).ok_or(Error::Overflow)?;
        }
    }
    Ok(needs)
}
