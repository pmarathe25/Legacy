//! Summarize recorded allele values at authored loci among living people.
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Sample {
    year: u64,
    mean: f64,
    people: u64,
    variants: u64,
}

struct Series {
    name: String,
    first: Sample,
    latest: Sample,
}

#[derive(Default)]
pub(super) struct History {
    loci: BTreeMap<u64, Series>,
}

impl History {
    pub(super) fn observe(&mut self, state: &Value, year: u64) {
        let Some(loci) = state
            .pointer("/family/traits/loci")
            .and_then(Value::as_array)
        else {
            return;
        };
        let Some(members) = state.pointer("/family/members").and_then(Value::as_array) else {
            return;
        };
        let mut traits: BTreeMap<_, _> = loci
            .iter()
            .filter_map(|row| {
                Some((
                    row["id"].as_u64()?,
                    (
                        row["name"].as_str()?,
                        allele_pair(&row["default_alleles"])?,
                        0i128,
                        0u64,
                    ),
                ))
            })
            .collect();
        let mut living = 0u64;
        for person in members
            .iter()
            .filter(|row| row["alive"].as_bool().unwrap_or(true))
        {
            living += 1;
            for variant in person
                .pointer("/genome/named_loci")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(id) = variant["locus"].as_u64() else {
                    continue;
                };
                let Some(pair) = allele_pair(&variant["alleles"]) else {
                    continue;
                };
                if let Some((_, default, difference, count)) = traits.get_mut(&id)
                    && pair != *default
                {
                    *difference += allele_sum(pair) - allele_sum(*default);
                    *count += 1;
                }
            }
        }
        if living == 0 {
            return;
        }
        for (id, (name, default, difference, variants)) in traits {
            let mean = (allele_sum(default) * i128::from(living) + difference) as f64
                / (2.0 * living as f64);
            let sample = Sample {
                year,
                mean,
                people: living,
                variants,
            };
            self.loci
                .entry(id)
                .and_modify(|row| row.latest = sample)
                .or_insert_with(|| Series {
                    name: name.to_owned(),
                    first: sample,
                    latest: sample,
                });
        }
    }

    pub(super) fn summary(&self) -> Vec<Value> {
        self.loci.iter().map(|(&locus, row)| {
            let sample = |row: Sample| json!({ "year": row.year, "mean": row.mean, "people": row.people, "variants": row.variants });
            json!({ "locus": locus, "name": row.name, "first": sample(row.first), "latest": sample(row.latest) })
        }).collect()
    }
}

fn allele_pair(value: &Value) -> Option<[u64; 2]> {
    let pair = value.as_array()?;
    if pair.len() != 2 {
        return None;
    }
    Some([pair[0].as_u64()?, pair[1].as_u64()?])
}

fn allele_sum(pair: [u64; 2]) -> i128 {
    i128::from(pair[0]) + i128::from(pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_counts_catalog_defaults_and_living_variants() {
        let mut history = History::default();
        let snapshot = |alive| {
            json!({ "family": {
                "traits": { "loci": [{ "id": 19, "name": "caution", "default_alleles": [300, 500] }] },
                "members": [
                    { "genome": {}, "alive": true },
                    { "genome": { "named_loci": [{ "locus": 19, "alleles": [500, 700] }] }, "alive": alive },
                ]
            }})
        };
        history.observe(&snapshot(true), 2);
        history.observe(&snapshot(false), 5);
        assert_eq!(
            history.summary(),
            [json!({ "locus": 19, "name": "caution",
                "first": { "year": 2, "mean": 500.0, "people": 2, "variants": 1 },
                "latest": { "year": 5, "mean": 400.0, "people": 1, "variants": 0 }
            })]
        );
    }
}
