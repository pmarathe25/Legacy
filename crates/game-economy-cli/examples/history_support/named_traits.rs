//! Summarize recorded allele values at authored loci in living populations.
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
    species: Option<String>,
    first: Sample,
    latest: Sample,
    empty_at: Option<u64>,
}

struct Cohort<'snapshot> {
    species: Option<(u64, &'snapshot str)>,
    loci: BTreeMap<u64, Locus<'snapshot>>,
    living: u64,
}

struct Locus<'snapshot> {
    name: &'snapshot str,
    default: [u64; 2],
    difference: i128,
    variants: u64,
}

impl<'snapshot> Cohort<'snapshot> {
    fn new(traits: &'snapshot Value, species: Option<(u64, &'snapshot str)>) -> Option<Self> {
        let loci = traits["loci"]
            .as_array()?
            .iter()
            .filter_map(|row| {
                Some((
                    row["id"].as_u64()?,
                    Locus {
                        name: row["name"].as_str()?,
                        default: allele_pair(&row["default_alleles"])?,
                        difference: 0,
                        variants: 0,
                    },
                ))
            })
            .collect();
        Some(Self {
            species,
            loci,
            living: 0,
        })
    }

    fn add_living(&mut self, actor: &Value) {
        self.living += 1;
        for variant in actor
            .pointer("/genome/named_loci")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(locus) = variant["locus"]
                .as_u64()
                .and_then(|id| self.loci.get_mut(&id))
            else {
                continue;
            };
            let Some(pair) = allele_pair(&variant["alleles"]) else {
                continue;
            };
            if pair != locus.default {
                locus.difference += allele_sum(pair) - allele_sum(locus.default);
                locus.variants += 1;
            }
        }
    }
}

#[derive(Default)]
pub(super) struct History {
    loci: BTreeMap<(Option<u64>, u64), Series>,
}

impl History {
    pub(super) fn observe(&mut self, state: &Value, year: u64) {
        if let Some(mut people) = Cohort::new(&state["family"]["traits"], None) {
            for person in state["family"]["members"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|row| row["alive"].as_bool().unwrap_or(true))
            {
                people.add_living(person);
            }
            self.finish(year, people);
        }
        let fauna = &state["fauna"];
        let mut cohorts: BTreeMap<_, _> = fauna["species"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|row| {
                let id = row["id"].as_u64()?;
                let name = row["name"].as_str()?;
                Some((id, Cohort::new(&row["traits"], Some((id, name)))?))
            })
            .collect();
        if cohorts.is_empty() {
            return;
        }
        for animal in fauna["animals"].as_array().into_iter().flatten() {
            if animal["health"].as_u64().is_some_and(|health| health > 0)
                && let Some(cohort) = animal["species"]
                    .as_u64()
                    .and_then(|id| cohorts.get_mut(&id))
            {
                cohort.add_living(animal);
            }
        }
        for cohort in cohorts.into_values() {
            self.finish(year, cohort);
        }
    }

    fn finish(&mut self, year: u64, cohort: Cohort<'_>) {
        for (id, locus) in cohort.loci {
            let key = (cohort.species.map(|row| row.0), id);
            if cohort.living == 0 {
                if let Some(row) = self.loci.get_mut(&key) {
                    row.empty_at.get_or_insert(year);
                }
                continue;
            }
            let mean = (allele_sum(locus.default) * i128::from(cohort.living) + locus.difference)
                as f64
                / (2.0 * cohort.living as f64);
            let sample = Sample {
                year,
                mean,
                people: cohort.living,
                variants: locus.variants,
            };
            self.loci
                .entry(key)
                .and_modify(|row| {
                    row.latest = sample;
                    row.empty_at = None;
                })
                .or_insert_with(|| Series {
                    name: locus.name.to_owned(),
                    species: cohort.species.map(|row| row.1.to_owned()),
                    first: sample,
                    latest: sample,
                    empty_at: None,
                });
        }
    }

    pub(super) fn summary(&self) -> Vec<Value> {
        self.loci.iter().map(|(&(species, locus), row)| {
            let sample = |row: Sample| json!({ "year": row.year, "mean": row.mean, "people": row.people, "variants": row.variants });
            let mut value = json!({ "locus": locus, "name": row.name, "first": sample(row.first), "latest": sample(row.latest) });
            if let Some(species) = species {
                value["species"] = json!(species);
                value["species_name"] = json!(row.species);
            }
            if let Some(year) = row.empty_at {
                value["first_year_without_living_members"] = json!(year);
            }
            value
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

    #[test]
    fn wildlife_loci_are_tracked_by_species_and_report_empty_cohorts() {
        let mut history = History::default();
        let traits =
            json!({ "loci": [{ "id": 18, "name": "hardiness", "default_alleles": [300, 500] }] });
        let snapshot = |grazer_health| {
            json!({
                "fauna": {
                    "species": [
                        { "id": 1, "name": "grazer", "traits": traits },
                        { "id": 2, "name": "browser", "traits": traits },
                    ],
                    "animals": [
                        { "species": 1, "health": grazer_health, "genome": {
                            "named_loci": [{ "locus": 18, "alleles": [900, 900] }]
                        }},
                        { "species": 2, "health": 100, "genome": {} },
                    ]
                }
            })
        };
        history.observe(&snapshot(100), 0);
        history.observe(&snapshot(0), 3);
        let records = history.summary();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["species_name"], "grazer");
        assert_eq!(records[0]["latest"]["mean"], 900.0);
        assert_eq!(records[0]["latest"]["year"], 0);
        assert_eq!(records[0]["first_year_without_living_members"], 3);
        assert_eq!(records[1]["species_name"], "browser");
        assert_eq!(records[1]["latest"]["mean"], 400.0);
        assert_eq!(records[1]["latest"]["year"], 3);
    }
}
