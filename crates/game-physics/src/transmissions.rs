use super::*;
pub fn transmissions(
    config: &Config,
    parts: &[Part],
    broken: &BTreeSet<usize>,
    goods: impl Iterator<Item = (GoodId, u64)>,
) -> Result<Vec<Transmission>> {
    let mut held: BTreeMap<GoodId, u64> = goods.collect();
    let mut required = needs(config, parts)?;
    for index in broken {
        if let Some(p) = parts.get(*index) {
            *required.entry(p.good).or_default() = required[&p.good].saturating_sub(1);
        }
    }
    let supports = required
        .iter()
        .all(|(g, n)| held.get(g).copied().unwrap_or(0) >= *n);
    Ok(parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let material = &config.materials[&p.good];
            let count = held.entry(p.good).or_default();
            let intact = supports && !broken.contains(&i) && *count > 0;
            if intact {
                *count -= 1;
            }
            let (drive, load) = p.mechanism.ratio(p.drive, p.load, material.span).unwrap();
            Transmission {
                input: p.input,
                drive,
                load,
                efficiency: if intact { material.efficiency } else { 0 },
                limit: if intact { material.limit } else { u64::MAX },
            }
        })
        .collect())
}
